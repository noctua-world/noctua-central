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
//! ([`crate::assets`]): the document is loaded from its root, and its policy
//! ([`crate::page::POLICY`]) refuses every load from anywhere else; a
//! navigation to any address outside it is refused, a new window is never
//! opened, and a download never starts. **And it keeps nothing of its own**:
//! its store is ephemeral, and where a system's webview keeps a folder even
//! so, it is the install's `webview` directory (`nc_platform::webview::Kept`).
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
//! **Its footer is the state's, drawn again whenever the state moves it**
//! ([`crate::footer`]): each state the daemon answers is drawn as the
//! footer's markup, and the page is brought to it — once its document has
//! loaded, and only where the markup changed. What the page says the person
//! did is what any client could do: a control activated is its command,
//! dispatched as the person ([`Ask::Run`]), and an edit of the prompt is a
//! draft this window says ([`Fact::Draft`]), told to the daemon as a note on
//! the asking connection, one in flight at a time, and taken back when the
//! daemon refuses it.
//!
//! **Its size and its place are the state's** ([`asked_anew`]): a size the
//! state asks that the window has not applied — `window.resize` — sizes it so
//! its page is that many CSS pixels, through the one door that never makes
//! it less than its least ([`Opened::size_to`]), and a place — `window.move`
//! — puts its corner there, each once per change, so a window started again
//! comes back at the size and the place the state holds. **What the system's
//! own pointer is over is said, and every other seat drawn**
//! ([`hover_said`], [`crate::seat`]): the system's own pointer as seat 1's
//! hover, a note told once per target it moves onto, and every other seat the
//! state holds written onto the page after every draw as the design system's
//! seat attributes.
//!
//! **The keys are run here** ([`crate::keys`]): the Leader is asked of the
//! system on this thread where the desktop is its listener — never for a
//! test's install — and a refusal, or a registration lost, is said on the
//! info line by `info.say` in the window's locale, each time a daemon is
//! reached, never dropped ([`refused_said`], [`bound_said`]). A press, a key
//! held with the Leader, an entry of the leader menu and a row of a list each
//! run the command it names, dispatched as the person, and a refusal of one
//! is said on the info line too.
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
//! first frame and a settled frame painted, how large its page is when it is
//! asked to be smaller than the least, and whether its footer and its drawer
//! do what they are for in the system's own webview ([`Exercised`]) — the
//! drawer standing below the footer inside the window however much it holds
//! — and closed, so an
//! installer or a test can ask it of any machine with a display.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! The webview's message channel carries a closed set of messages
//! ([`said`]) and anything else is read as nothing; the webview loads the
//! one document this build wrote, from bytes this build embeds, loads and
//! connects to nothing else (`crate::page::POLICY`), is let to navigate
//! nowhere else, and writes nothing under the person's own directories. What the page says it shows ([`Witnessed`]) is
//! of a closed shape as well, runs nothing, and is printed for a test's
//! install alone. **What it does not answer:** a process running
//! as the same user, which can drive the interface through the CLI as the
//! person can, or inject into this one.

use core::time::Duration;
use std::time::Instant;

use nc_channel::found::Found;
use noctua_design::model::Mode;
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn as _;
use tao::window::{Window, WindowBuilder};

use crate::client::{Asker, Flow, Heard};
use crate::commands::Commands;
use crate::drawer::{Control, List};
use crate::footer::{Action, Context, Drawing};
use crate::keys::{Act, Keymap, key_of};
use crate::mirror::{Fact, Mirror, Region, Shell};
use crate::theme::Theme;
use nc_schema::keys::Key;

/// The window's title, which a window list shows though the window draws no
/// title bar.
pub const TITLE: &str = "Noctua";

/// What the document says through the webview's message channel: a closed
/// set, each written one way.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// `action <command>`: a control was activated, the footer's
    /// [`Action`] whose command it names.
    Action(Action),
    /// `action <command>`: a control of the drawer's controls rail was
    /// activated, the [`Control`] whose command it names.
    Control(Control),
    /// `draft <text>`: an edit changed the prompt's text, which is
    /// everything after the word and its one space.
    Draft(String),
    /// `exercised <enhanced> <kept> <written> <rails> <capped> <pointed>
    /// <refused>`, each `1` or `0`: what a probe's exercise of the footer and
    /// the drawer found (`EXERCISE`) — said by a probe's exercise alone.
    Exercised([bool; 7]),
    /// `leader down`: the page saw the Leader's chord go down.
    LeaderDown,
    /// `leader up`: one of the Leader's modifiers was let go while it was
    /// held, or the page was told of a hold whose modifiers were.
    LeaderUp,
    /// `key held <code> <chord>`: a key went down with the Leader's
    /// modifiers held, and the chord modifier as well where `1`.
    Held(Key, bool),
    /// `key menu <code>`: a key of the menu's table went down alone on the
    /// leader menu.
    InMenu(Key),
    /// `choose <list> <value>`: an option of a list was chosen — Enter, Space
    /// or a press.
    Choose(List, String),
    /// `send`, or `send <value>` with the option the keyboard is on in the
    /// list the prompt filters: the prompt was sent.
    Send(Option<String>),
    /// `active <list> <value>`: the keyboard moved to an option of a list.
    Active(List, String),
    /// `focus <region>`: the keyboard went to a part of the window — the
    /// prompt, the drawer, the sidebar, the info line or the modeline — or,
    /// given up to nothing, to the workspace area.
    Focus(Region),
    /// `escape`: Esc on the drawer, or on a prompt that filters a list.
    Escape,
    /// `hover <target>`, or `hover` alone: the system's own pointer moved
    /// onto what names that target, or onto nothing that names one.
    Hover(Option<String>),
    /// `seen …`: what the page shows, as its witness says it — asked of a
    /// test's install alone ([`Witnessed`]).
    Seen(Witnessed),
}

/// What a page says it shows when the window asks it (`window.noctua.witness`
/// in [`crate::page::SCRIPT`]): asked, and printed, only for a test's install.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "four facts a witness says of a page, each independent of the others: there is \
              no state they move through, so no machine to write"
)]
pub struct Witnessed {
    /// Its width and height in CSS pixels.
    pub page: (u32, u32),
    /// Whether every element drawn stands inside the window, or inside
    /// something that clips it, each way.
    pub fits: bool,
    /// Whether every line of the footer stands wholly inside the window and
    /// inside every box that clips it: the footer's lines stay (§12.19).
    pub lines: bool,
    /// Whether every control can be reached, by a touch and by the keyboard.
    ///
    /// Every control drawn is at least `--noctua-target-min` each way, and
    /// every box that scrolls what it holds past what it shows is a stop of
    /// the keyboard or holds one.
    pub reachable: bool,
    /// Whether every seat's attribute changes how the element it is on is
    /// drawn, where the system's own seat is not in that state there.
    pub styled: bool,
    /// The part the system's own keyboard is in: `prompt`, `drawer`,
    /// `sidebar`, `info`, `modeline`, `workspace`, `elsewhere`, or `-` for
    /// none — on nothing, or on an element that can no longer take it, which
    /// the system's webview drops it from a moment later.
    pub keyboard: String,
    /// The parts that draw anything, of `footer`, `drawer`, `sidebar` and
    /// `workspace`, in that order.
    pub shows: Vec<String>,
    /// Every seat's state on the page, as `<seat>:<state>:<where>`: the
    /// seat's number, `hover`, `focus`, `active` or `drag`, and the target the
    /// element it is on names, or the part that element is in.
    pub seats: Vec<String>,
}

/// The parts of the page a witness says draw anything, in their order.
const PARTS: [&str; 4] = ["footer", "drawer", "sidebar", "workspace"];

/// The parts the system's own keyboard may be said to be in.
const KEYBOARDS: [&str; 8] = [
    "prompt",
    "drawer",
    "sidebar",
    "info",
    "modeline",
    "workspace",
    "elsewhere",
    "-",
];

/// The states a seat may be said to be in.
const STATES: [&str; 4] = ["hover", "focus", "active", "drag"];

/// What a witness's words say, where they are what one says: its size, four
/// bits, the keyboard, the parts and at most four states of each seat the
/// kernel allows, each word of its closed shape.
fn witnessed(words: &[&str]) -> Option<Witnessed> {
    let [
        width,
        height,
        fits,
        lines,
        reachable,
        styled,
        keyboard,
        shows,
        seats @ ..,
    ] = words
    else {
        return None;
    };
    let shows: Vec<String> = if *shows == "-" {
        Vec::new()
    } else {
        shows.split(',').map(str::to_owned).collect()
    };
    let ordered = shows
        .iter()
        .map(|part| PARTS.iter().position(|known| known == part))
        .collect::<Option<Vec<usize>>>()?;
    let seats_max = usize::try_from(nc_catalog::KERNEL.shell_seats_max).ok()? * STATES.len();
    let seats = seats
        .iter()
        .map(
            |seat| match seat.split(':').collect::<Vec<_>>().as_slice() {
                [id, state, rest @ ..] if number::<u64>(id).is_some() && STATES.contains(state) => {
                    let place = rest.join(":");
                    is_target(&place).then(|| (*seat).to_owned())
                }
                _ => None,
            },
        )
        .collect::<Option<Vec<String>>>()?;
    (ordered.windows(2).all(|pair| pair[0] < pair[1])
        && KEYBOARDS.contains(keyboard)
        && seats.len() <= seats_max)
        .then(|| {
            Some(Witnessed {
                page: (number(width)?, number(height)?),
                fits: bit(fits)?,
                lines: bit(lines)?,
                reachable: bit(reachable)?,
                styled: bit(styled)?,
                keyboard: (*keyboard).to_owned(),
                shows,
                seats,
            })
        })
        .flatten()
}

/// What the system says of a window, as a test's install prints it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Windowed {
    /// Whether it is shown.
    pub shown: bool,
    /// Whether it draws the system's decorations.
    pub decorated: bool,
    /// Where its corner is, in the system's logical pixels, where the system
    /// says.
    pub at: Option<(i64, i64)>,
}

/// The line a test's install prints of what its window and its page show:
/// what the system says of the window, the `drawn` digest of what its page
/// holds ([`crate::footer::Drawing::digest`]), and what the page's witness
/// says, each a `name=value` word.
#[must_use]
pub fn witnessed_line(window: Windowed, drawn: u64, seen: &Witnessed) -> String {
    let bit = |held: bool| if held { '1' } else { '0' };
    let listed = |words: &[String]| {
        if words.is_empty() {
            "-".to_owned()
        } else {
            words.join(",")
        }
    };
    let at = window
        .at
        .map_or_else(|| "-".to_owned(), |(x, y)| format!("{x},{y}"));
    format!(
        "witnessed shown={} decorated={} at={at} drawn={drawn:016x} page={}x{} fits={} \
         lines={} reachable={} styled={} keyboard={} shows={} seats={}",
        bit(window.shown),
        bit(window.decorated),
        seen.page.0,
        seen.page.1,
        bit(seen.fits),
        bit(seen.lines),
        bit(seen.reachable),
        bit(seen.styled),
        seen.keyboard,
        listed(&seen.shows),
        listed(&seen.seats),
    )
}

/// The most bytes the value of an option the page says may hold: a
/// registered command's name, or a parameter's or a value's word.
const VALUE_BYTES_MAX: usize = 256;

/// `rest`, the value of an option, where it is one: a text of at most
/// [`VALUE_BYTES_MAX`] bytes holding no control character.
fn value(rest: &str) -> Option<String> {
    (!rest.is_empty() && rest.len() <= VALUE_BYTES_MAX && !rest.chars().any(char::is_control))
        .then(|| rest.to_owned())
}

/// The list `rest` names first, and the option's value after one space.
fn option(rest: &str) -> Option<(List, String)> {
    let (list, rest) = rest.split_once(' ')?;
    Some((List::of(list)?, value(rest)?))
}

/// A number written in ASCII digits alone, of at most ten.
fn number<T: core::str::FromStr>(word: &str) -> Option<T> {
    (!word.is_empty() && word.len() <= 10 && word.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| word.parse().ok())
        .flatten()
}

/// A bit written as `1` or `0`.
fn bit(word: &str) -> Option<bool> {
    match word {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
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
/// else separates or surrounds them, but for a draft, whose text is the
/// whole of the message after its word and one space.
#[must_use]
pub fn said(message: &str) -> Option<Said> {
    if let Some(text) = message
        .strip_prefix(crate::page::DRAFT)
        .and_then(|rest| rest.strip_prefix(' '))
    {
        return Some(Said::Draft(text.to_owned()));
    }
    if let Some(rest) = message.strip_prefix("choose ") {
        return option(rest).map(|(list, value)| Said::Choose(list, value));
    }
    if let Some(rest) = message.strip_prefix("active ") {
        return option(rest).map(|(list, value)| Said::Active(list, value));
    }
    if let Some(rest) = message.strip_prefix("send ") {
        return value(rest).map(|value| Said::Send(Some(value)));
    }
    let words: Vec<&str> = message.split(' ').collect();
    match words.as_slice() {
        [action, command] if *action == crate::page::ACTION => Action::of(command)
            .map(Said::Action)
            .or_else(|| Control::of(command).map(Said::Control)),
        [loaded] if *loaded == crate::page::LOADED => Some(Said::Loaded),
        [scale, thousandths] if *scale == crate::page::SCALE => {
            number(thousandths).map(Said::Scale)
        }
        ["first-frame", rest @ ..] => channels(rest).map(Said::FirstFrame),
        ["settled", rest @ ..] => channels(rest).map(Said::Settled),
        ["viewport", width, height] => Some(Said::Viewport(number(width)?, number(height)?)),
        [
            "exercised",
            enhanced,
            kept,
            written,
            rails,
            capped,
            pointed,
            refused,
        ] => Some(Said::Exercised([
            bit(enhanced)?,
            bit(kept)?,
            bit(written)?,
            bit(rails)?,
            bit(capped)?,
            bit(pointed)?,
            bit(refused)?,
        ])),
        ["send"] => Some(Said::Send(None)),
        ["escape"] => Some(Said::Escape),
        ["leader", "down"] => Some(Said::LeaderDown),
        ["leader", "up"] => Some(Said::LeaderUp),
        ["key", "held", code, chord] => Some(Said::Held(key_of(code)?, bit(chord)?)),
        ["key", "menu", code] => key_of(code).map(Said::InMenu),
        ["focus", "prompt"] => Some(Said::Focus(Region::Prompt)),
        ["focus", "drawer"] => Some(Said::Focus(Region::Drawer)),
        ["focus", "sidebar"] => Some(Said::Focus(Region::Sidebar)),
        ["focus", "info"] => Some(Said::Focus(Region::Info)),
        ["focus", "modeline"] => Some(Said::Focus(Region::Modeline)),
        ["focus", "workspace"] => Some(Said::Focus(Region::Workspace)),
        ["hover"] => Some(Said::Hover(None)),
        ["hover", target] => is_target(target).then(|| Said::Hover(Some((*target).to_owned()))),
        ["seen", rest @ ..] => witnessed(rest).map(Said::Seen),
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
    /// The daemon answered the note in flight: taken, or refused.
    Told(bool),
    /// The system's registration heard the Leader go down.
    Pressed,
    /// Asking the system for the Leader was refused, or the registration
    /// held stopped delivering, as the info line says it.
    Unheard(String),
    /// The registry, asked in each locale: the commands the list shows.
    Registry(Commands),
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
    /// The folder a probe's webview keeps its own in was not made.
    Profile(std::io::Error),
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
            Self::Profile(error) => write!(
                formatter,
                "the folder a probe's webview keeps its own in was not made: {error}"
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
    /// What the webview keeps of its own (`nc_platform::webview::Kept`),
    /// held as long as the webview and dropped after it.
    kept: nc_platform::webview::Kept,
    /// The theme it was opened in.
    pub theme: Theme,
    /// The least the window is, in logical pixels: the catalog's least,
    /// fitted to the page's scale once the page has said it ([`fitted`]).
    least: LogicalSize<f64>,
    /// How many device pixels one CSS pixel of the page is, in thousandths,
    /// once the page has said it.
    thousandths: Option<u32>,
    /// The size in CSS pixels the interface's state last asked the window's
    /// page to be, once it asked one.
    asked: Option<LogicalSize<f64>>,
    /// What the page holds of the footer, and what brings it to the state.
    pub drawing: Drawing,
}

impl Opened {
    /// Runs each of `scripts` in the page, in order.
    pub fn send(&self, scripts: Vec<String>) {
        for script in scripts {
            // A page that cannot run it has gone with its window.
            let _ = self.webview.evaluate_script(&script);
        }
    }

    /// The window's least size fitted to a page whose CSS pixel is
    /// `thousandths` device pixels, so its page is never smaller than the
    /// catalog's least; a window already smaller is made that size. A scale
    /// the page had not said before sizes the window again to the CSS pixels
    /// the state asked for, which a system scaling its page by more than its
    /// own factor would otherwise draw at another size.
    pub fn fit(&mut self, thousandths: u32) {
        let scale_factor = self.window.scale_factor();
        let rescaled = self.thousandths.replace(thousandths) != Some(thousandths);
        let fitted = fitted(least(), thousandths, scale_factor);
        if fitted != self.least {
            self.least = fitted;
            self.window.set_min_inner_size(Some(fitted));
            let inner = self.window.inner_size().to_logical::<f64>(scale_factor);
            if inner.width < fitted.width || inner.height < fitted.height {
                self.window.set_inner_size(at_least(inner, fitted));
            }
        }
        if rescaled && let Some(asked) = self.asked {
            self.resize(self.logical(asked));
        }
    }

    /// The window asked for `asked`, made its least where it falls short:
    /// the one way this process sizes a window.
    pub fn resize(&self, asked: LogicalSize<f64>) {
        self.window.set_inner_size(at_least(asked, self.least));
    }

    /// The size in logical pixels whose page is `css` CSS pixels, at the
    /// scale the page said; `css` itself before it said one.
    fn logical(&self, css: LogicalSize<f64>) -> LogicalSize<f64> {
        self.thousandths.map_or(css, |thousandths| {
            fitted(css, thousandths, self.window.scale_factor())
        })
    }

    /// The window sized so its page is `css` CSS pixels, as the interface's
    /// state asks (`window.resize`), through [`Opened::resize`], which never
    /// makes it less than its least.
    pub fn size_to(&mut self, css: LogicalSize<f64>) {
        self.asked = Some(css);
        self.resize(self.logical(css));
    }

    /// The window's corner put at `place`, in the system's logical pixels,
    /// as the interface's state asks (`window.move`).
    pub fn place_at(&self, place: LogicalPosition<f64>) {
        self.window.set_outer_position(place);
    }
}

/// What the interface's state asks of a window that the window has not
/// applied yet: `wanted`, where it is something and not `applied`; nothing
/// where it is what was applied, and nothing where the state asks nothing,
/// which leaves the window as it is.
#[must_use]
pub fn asked_anew<T: Copy + PartialEq>(wanted: Option<T>, applied: Option<T>) -> Option<T> {
    wanted.filter(|wanted| Some(*wanted) != applied)
}

impl core::fmt::Debug for Opened {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Opened")
            .field("window", &self.window.id())
            .field("webview", &self.webview.id())
            .field("kept", &self.kept)
            .field("theme", &self.theme)
            .field("least", &self.least)
            .field("thousandths", &self.thousandths)
            .field("asked", &self.asked)
            .field("drawing", &self.drawing)
            .finish()
    }
}

/// What a probe's observer says, from the first moment of the document: the
/// page's background at its first frame and once settled, each painted into
/// a canvas of one pixel and read back (`NOCTUA.md §9.6`, lesson 5), and the
/// page's size whenever it changes; and every load the document's policy
/// refuses, kept for the exercise (`EXERCISE`) to read.
const OBSERVES: &str = "(() => {\
    const refused = [];\
    window.noctuaRefused = refused;\
    document.addEventListener('securitypolicyviolation', (event) => refused.push(event.blockedURI));\
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
/// handed the document in `context`'s locale, its footer the first run's
/// for `context`; what the document says reaches the loop through `proxy`,
/// tagged `index`, and a probe's observer watches it when `observed`.
///
/// **The webview keeps nothing of its own** (`NOCTUA.md §12.1`): its store is
/// ephemeral, since what the page shows is the daemon's, and where a system's
/// webview needs a folder even so — `WebView2`'s user data folder — it is
/// `profile`, the install's `webview` directory (`nc_channel::Layout`) or one a
/// probe makes and removes, never one the webview would name under the
/// user's own directories after the executable.
///
/// # Errors
///
/// [`Unopened::Window`] for a window the system refused, and
/// [`Unopened::Webview`] for a webview it did not build.
pub fn open(
    target: &EventLoopWindowTarget<Arrived>,
    proxy: EventLoopProxy<Arrived>,
    (context, profile): (Context, &std::path::Path),
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
    let footer = crate::footer::view(None, context);
    let drawing = Drawing::new(crate::footer::markup(&footer));
    let document = crate::page::document(context.locale, theme, &footer);
    let origin = crate::assets::origin();
    let allowed = origin.clone();
    let mut kept = nc_platform::webview::Kept::in_folder(profile);
    let mut builder = kept
        .builder()
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
        kept,
        theme,
        least: least(),
        thousandths: None,
        asked: None,
        drawing,
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

    /// Whether its document said it loaded.
    #[must_use]
    pub const fn is_loaded(self) -> bool {
        self.loaded
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
    /// What the exercise of its footer and its drawer found, once the page
    /// settled; nothing when it was not said within the wait.
    pub footer: Option<Exercised>,
}

/// What a probe's exercise of a window's footer and drawer found
/// (`EXERCISE`), in the real webview: the design system's scripts, the
/// page's own, the window's calls into it and the document's layout, each
/// doing what it is for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "fourteen facts a probe observes, each independent of the others: there is no \
              state they move through, so no machine to write"
)]
pub struct Exercised {
    /// The design system's script enhanced the prompt: it is marked so, and
    /// its block is drawn.
    pub enhanced: bool,
    /// A pointer moving onto the info line's chevron — a touch's — said the
    /// command the chevron runs as what the system's own pointer is over.
    pub hovered: bool,
    /// Typing into the prompt said what was typed, as a draft.
    pub drafted: bool,
    /// Activating the info line's chevron said `info.expand`.
    pub acted: bool,
    /// The footer brought to new markup showed what the markup changed and
    /// kept the focus on the chevron and what the design system's script
    /// added to the prompt.
    pub kept: bool,
    /// A draft written into the prompt reached its text area.
    pub written: bool,
    /// The design system's scripts took the drawer drawn into the page and
    /// its tabs rail: the drawer is marked enhanced, the strip a tab list.
    pub rails: bool,
    /// The drawer, holding more than fits, stood below the footer and
    /// inside the window, its content scrolling: its cap held.
    pub capped: bool,
    /// Pressing a tab of the tabs rail said the page it names.
    pub tabbed: bool,
    /// Pressing the controls rail's collapse said its command.
    pub collapsed: bool,
    /// The prompt pointed at the active option of the list the drawer drew
    /// once it named that list, and once both were drawn closed no element
    /// pointed at an option that is gone.
    pub pointed: bool,
    /// The two loads the page asked of an address off the protocol — a
    /// fetch and an image — were refused by the document's policy, and
    /// nothing of its own was.
    pub refused: bool,
    /// The keyboard taking the info line's chevron was said as on the info
    /// line, and given up to nothing as on the workspace area.
    pub placed: bool,
    /// A Leader holding Super, its keys reported as `WebKitGTK` reports
    /// them — Super's own key going down and up, and no Meta flag on any key
    /// — was said going down, a key pressed with it held, and going up.
    pub held: bool,
}

/// The text a probe's exercise types into the prompt.
pub const PROBED_DRAFT: &str = "probed draft";

/// The message the footer a probe's exercise draws says.
const PROBED_MESSAGE: &str = "probed message";

/// The draft a probe's exercise writes into the prompt.
const PROBED_WRITTEN: &str = "probed written";

/// The page whose tab a probe's exercise presses: the leader menu's, the
/// one not in focus in the drawer it draws ([`probed_drawer`]).
const PROBED_TAB: u64 = 3;

/// A probe's exercise of the footer and the drawer, in the page once it has
/// settled, its `{draw}` the call that brings the footer to markup with a
/// message on the info line, expanded, its prompt naming the command list,
/// its `{drawer}` the one that brings the drawer to the command list over two
/// pages, and its `{closed}` the two that bring both back to no list: two
/// loads asked of an address off the protocol, which the document's policy
/// refuses; whether the design
/// system's script enhanced the prompt; a touch's pointer moving onto the
/// info line's chevron, which the page says as what the system's own pointer
/// is over; typing into the prompt, and activating the
/// info line's chevron with the focus on it; the draw, after which the
/// chevron still has the focus and says the line is expanded, the message is
/// shown, the prompt still holds what was typed and is still enhanced; a
/// draft written; the drawer drawn, after which the design system's scripts
/// took it and its tabs rail, and it stands below the footer inside the
/// window with its content scrolling, and every tab names a tab panel; the
/// prompt pointing at the list's active option; a tab and collapse pressed; a
/// Leader holding Super pressed as `WebKitGTK` reports its keys (`{keys}`);
/// both drawn closed, after which nothing points at an option that is gone;
/// and the keyboard given up to nothing. It says what it found as
/// [`Said::Exercised`] once the policy has refused both loads, or a second
/// has passed, after the draft, the action, the tab, the control, the keys
/// and the keyboard's places the page's own script said.
const EXERCISE: &str = "(() => {
  const refused = window.noctuaRefused || [];
  fetch('http://127.0.0.1:9/probed').catch(() => null);
  const image = new Image();
  image.src = 'http://127.0.0.1:9/probed.png';
  const footer = document.querySelector('footer');
  const prompt = footer.querySelector('[data-nd-prompt]');
  const area = footer.querySelector('.nd-prompt-input');
  const toggle = footer.querySelector('.nd-footer-line-toggle');
  const enhanced = () => prompt.getAttribute('data-nd-enhanced') === 'true'
    && footer.querySelector('.nd-prompt-cursor') !== null;
  const wasEnhanced = enhanced();
  toggle.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, pointerType: 'touch' }));
  area.value = '{typed}';
  area.dispatchEvent(new Event('input', { bubbles: true }));
  toggle.focus();
  toggle.click();
  {draw}
  const kept = document.activeElement === toggle
    && toggle.getAttribute('aria-expanded') === 'true'
    && footer.textContent.includes('{message}')
    && area.value === '{typed}'
    && enhanced();
  window.noctua.draft('{written}');
  const written = area.value === '{written}';
  {drawer}
  const region = document.getElementById('noctua-drawer');
  const drawer = region.querySelector('[data-nd-drawer]');
  const content = drawer ? drawer.querySelector('.nd-drawer-panel > :not(.nd-drawer-rail)') : null;
  const strip = region.querySelector('[data-nd-tabs]');
  const rails = drawer !== null && drawer.getAttribute('data-nd-enhanced') === 'drawer'
    && strip !== null && strip.getAttribute('role') === 'tablist'
    && Array.from(strip.querySelectorAll('.nd-tab')).every((tab) => {
      const panel = document.getElementById(tab.getAttribute('aria-controls'));
      return panel !== null && panel.getAttribute('role') === 'tabpanel';
    });
  const listed = document.getElementById(area.getAttribute('aria-controls') || '');
  const option = listed ? listed.querySelector('[data-nd-active=true]') : null;
  const pointedOpen = option !== null && option.id !== ''
    && area.getAttribute('aria-activedescendant') === option.id;
  const whole = document.getElementById('noctua').getBoundingClientRect();
  const lines = footer.getBoundingClientRect();
  const box = drawer ? drawer.getBoundingClientRect() : null;
  const capped = box !== null && content !== null && content.scrollHeight > content.clientHeight
    && lines.top >= whole.top - 0.5 && lines.bottom <= box.top + 0.5
    && box.bottom <= whole.bottom + 0.5;
  const other = strip ? strip.querySelector('.nd-tab[data-nd-value=\"{tab}\"]') : null;
  if (other) other.click();
  const collapse = region.querySelector('[data-nd-action=\"drawer.collapse\"]');
  if (collapse) collapse.click();
  window.noctua.keys({keys});
  const key = (type, code, alt) => window.dispatchEvent(new KeyboardEvent(type,
    { code, key: code, altKey: alt, metaKey: false, bubbles: true, cancelable: true }));
  key('keydown', 'AltLeft', true);
  key('keydown', 'MetaLeft', true);
  key('keydown', 'Space', true);
  key('keydown', 'KeyQ', true);
  key('keyup', 'KeyQ', true);
  key('keyup', 'MetaLeft', true);
  key('keyup', 'AltLeft', false);
  window.noctua.keys(null);
  {closed}
  const pointed = pointedOpen && Array.from(document.querySelectorAll('[aria-activedescendant]'))
    .every((holder) => document.getElementById(holder.getAttribute('aria-activedescendant')) !== null);
  if (document.activeElement) document.activeElement.blur();
  const off = (address) => address.startsWith('http://127.0.0.1:9');
  const bit = (held) => (held ? '1' : '0');
  const finish = (left) => {
    if (left > 0 && refused.filter(off).length < 2) {
      setTimeout(() => finish(left - 1), 25);
      return;
    }
    const turned = refused.filter(off).length === 2 && refused.every(off);
    window.ipc.postMessage('exercised ' + bit(wasEnhanced) + ' ' + bit(kept) + ' ' + bit(written)
      + ' ' + bit(rails) + ' ' + bit(capped) + ' ' + bit(pointed) + ' ' + bit(turned));
  };
  setTimeout(() => finish(40), 0);
})();";

/// The drawer a probe's exercise draws: the command list over the leader
/// menu's page, every command of the catalog's in it — more than any window
/// holds — with its controls rail and a tabs rail of the two pages.
fn probed_drawer(locale: nc_catalog::Locale) -> crate::drawer::View {
    use crate::mirror::{Asking, Drawer, Kind, Page, Place};
    let page = |id: u64, kind: Kind| Page {
        id,
        kind,
        topic: None,
        trail: Vec::new(),
        scroll: 0,
    };
    let shell = Shell {
        windows: Vec::new(),
        pages: [
            page(PROBED_TAB, Kind::Leader),
            page(PROBED_TAB + 1, Kind::Commands),
        ]
        .into_iter()
        .map(|page| (page.id, page))
        .collect(),
        drawer: Drawer {
            pages: vec![PROBED_TAB, PROBED_TAB + 1],
            focused: Some(PROBED_TAB + 1),
            collapsed: false,
            toward: Place::Workspace,
            flipped: false,
            share: 33,
        },
        sidebar: false,
        footer: true,
        expanded: false,
        message: None,
        draft: ";".to_owned(),
        asking: Some(Asking::Commands),
        seats: std::collections::BTreeMap::new(),
        next: PROBED_TAB + 2,
    };
    // The command list draws no key, so the drawer is drawn with none bound.
    let channel = nc_channel::Channel::Production;
    let keymap = Keymap::new(
        nc_schema::keys::LeaderPart {
            leader: channel.leader(),
            chord: channel.chord_modifier(),
        },
        nc_schema::keys::Tables::default(),
    );
    crate::drawer::view(
        Some(&shell),
        ";",
        None,
        (&keymap, &Commands::from_catalog()),
        locale,
    )
}

/// The exercise of the footer and the drawer a window opened for `context`
/// is put through (`EXERCISE`).
fn exercise(context: Context) -> String {
    let drawer = probed_drawer(context.locale);
    let mut view = crate::footer::view(None, context);
    view.info.expanded = true;
    view.info.rows = vec![crate::footer::Row::Message(PROBED_MESSAGE.to_owned())];
    let closed = crate::footer::View {
        controls: None,
        ..view.clone()
    };
    view.controls = drawer.controls();
    EXERCISE
        .replace("{typed}", PROBED_DRAFT)
        .replace("{message}", PROBED_MESSAGE)
        .replace("{written}", PROBED_WRITTEN)
        .replace("{tab}", &PROBED_TAB.to_string())
        .replace("{keys}", &probed_keys().to_canonical())
        .replace(
            "{draw}",
            &crate::footer::draw(&crate::footer::markup(&view)),
        )
        .replace(
            "{drawer}",
            &crate::footer::draw(&crate::drawer::markup(&drawer)),
        )
        .replace(
            "{closed}",
            &[
                crate::footer::draw(&crate::footer::markup(&closed)),
                crate::footer::draw(&crate::drawer::markup(&crate::drawer::View::hidden())),
            ]
            .concat(),
        )
}

/// The keys a probe's exercise tells the page: a Leader holding Super — the
/// development channel's default, Alt+Super+Space — whose keys it then
/// presses as `WebKitGTK` reports them, Super's own key going down and up and
/// no Meta flag on any key; nothing where the grammar refused that chord.
fn probed_keys() -> nc_canon::Value {
    use nc_schema::keys::{Chord, Leader, LeaderPart, Modifier, Modifiers, Tables};
    Leader::of(Chord::new(
        Modifiers::of(&[Modifier::Alt, Modifier::Super]),
        Key::Space,
    ))
    .map_or(nc_canon::Value::Null, |leader| {
        Keymap::new(
            LeaderPart {
                leader,
                chord: Modifier::Shift,
            },
            Tables::default(),
        )
        .told()
    })
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
    /// What the page's own script said while the footer and the drawer were
    /// exercised: a draft, an action, a tab, a control, the keys held and the
    /// keyboard's places.
    exercised: Exercised,
    /// How far the keys of a Leader holding Super were said, in order: going
    /// down, a key held with it, going up.
    held: u8,
    /// How far the keyboard's places were said, in order: the info line,
    /// then the workspace area.
    placed: u8,
    /// What the window was opened for.
    context: Context,
    /// When the probe stops waiting for it.
    deadline: Instant,
}

impl Watched {
    /// A window opened in `index`-th of [`PROBED`]'s modes, asked to show at
    /// once, watched until `within` has passed.
    fn open(
        target: &EventLoopWindowTarget<Arrived>,
        proxy: EventLoopProxy<Arrived>,
        (index, profile): (usize, &std::path::Path),
        within: Duration,
    ) -> Result<Self, Unopened> {
        let mode = PROBED.get(index).copied().unwrap_or(Mode::Light);
        let context = Context {
            locale: nc_catalog::Locale::En,
            channel: nc_channel::Channel::Production,
        };
        let opened = open(target, proxy, (context, profile), mode, (index, true))?;
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
            exercised: Exercised::default(),
            held: 0,
            placed: 0,
            context,
            deadline: Instant::now() + within,
        })
    }

    /// Takes in what its document said; whether the window has been watched
    /// to the end.
    fn heard(&mut self, said: &Said) -> bool {
        match *said {
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
                self.opened.send(vec![exercise(self.context)]);
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
                    return self.seen.footer.is_some();
                }
            }
            Said::Draft(ref text) => self.exercised.drafted = text == PROBED_DRAFT,
            Said::Action(action) => self.exercised.acted = action == Action::ExpandInfo,
            Said::Choose(List::Tabs, ref value) => {
                self.exercised.tabbed = *value == PROBED_TAB.to_string();
            }
            Said::Control(control) => self.exercised.collapsed = control == Control::Collapse,
            // Only the one hover the exercise makes counts, and a pointer
            // that leaves later takes nothing back.
            Said::Hover(ref target) => {
                self.exercised.hovered |= target.as_deref() == Some(Action::ExpandInfo.command());
            }
            Said::Exercised([enhanced, kept, written, rails, capped, pointed, refused]) => {
                self.seen.footer = Some(Exercised {
                    enhanced,
                    kept,
                    written,
                    rails,
                    capped,
                    pointed,
                    refused,
                    held: self.held == 3,
                    placed: self.placed == 2,
                    ..self.exercised
                });
                return self.seen.least.is_some();
            }
            // Each step counts only after the one before it.
            Said::LeaderDown => self.held = u8::from(self.held == 0),
            Said::Held(Key::KeyQ, false) if self.held == 1 => self.held = 2,
            Said::LeaderUp if self.held == 2 => self.held = 3,
            Said::Focus(Region::Info) if self.placed == 0 => self.placed = 1,
            Said::Focus(Region::Workspace) if self.placed == 1 => self.placed = 2,
            Said::LeaderUp
            | Said::Held(..)
            | Said::InMenu(_)
            | Said::Choose(..)
            | Said::Send(_)
            | Said::Active(..)
            | Said::Focus(_)
            | Said::Escape
            | Said::Seen(_) => {}
        }
        false
    }
}

/// How long a probe tries to remove the folder its webviews kept their own
/// in before it leaves that folder to the system.
///
/// `WebView2`'s browser process holds files in that folder until it ends, a
/// moment after the last webview using it closed. This bounds the wait for
/// that end, so that a process that never ends cannot hold the probe.
const LETTING_GO: Duration = Duration::from_secs(20);

/// The folder a probe's webviews keep their own in, where a system's webview
/// keeps anything: a scratch made new and private under the system's
/// temporary one, `nc-desktop-probe-` and a nonce, and removed with
/// everything in it however the probe ends.
struct Scratch(std::path::PathBuf);

impl Scratch {
    /// A folder no other has named, made private.
    fn new() -> std::io::Result<Self> {
        let mut nonce = [0_u8; 8];
        nc_platform::random(&mut nonce)?;
        let folder = std::env::temp_dir().join(format!(
            "nc-desktop-probe-{:016x}",
            u64::from_le_bytes(nonce)
        ));
        nc_platform::create_private_directory(&folder)?;
        Ok(Self(folder))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // **Tried again until the webviews' own processes let go of it.** On
        // Windows this is WebView2's user data folder, which its browser
        // process holds until it ends, after the probe's last webview closed.
        // After a single try, a Windows runner found the folder still there.
        // A folder still not removed once `LETTING_GO` has passed is left to
        // the system's temporary folder to clear, and is no reason to fail
        // what the probe found.
        let deadline = Instant::now() + LETTING_GO;
        while std::fs::remove_dir_all(&self.0).is_err()
            && self.0.exists()
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Opens a window as the interface does in each of the light and the dark,
/// one after the other, each asked to show from the moment it is opened,
/// watched for `within` at most and closed: whether this system can host the
/// interface, asked with no daemon. **What its webviews keep of their own is
/// kept in a folder the probe makes and removes**, a scratch of its own.
///
/// # Errors
///
/// [`Unopened`], as [`open`] refuses, and [`Unopened::Profile`] where that
/// folder could not be made.
pub fn probe(within: Duration) -> Result<Probed, Unopened> {
    let profile = Scratch::new().map_err(Unopened::Profile)?;
    let mut event_loop = EventLoopBuilder::<Arrived>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut watched = Some(Watched::open(
        &event_loop,
        proxy.clone(),
        (0, &profile.0),
        within,
    )?);
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
            Event::UserEvent(Arrived::Said { window, said }) if window == at => {
                current.heard(&said)
            }
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
            match Watched::open(target, proxy.clone(), (at, &profile.0), within) {
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
    drop(profile);
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
    /// The command a control of the footer runs.
    Run(Action),
    /// A command a key or a list runs, with its arguments: a refusal is said
    /// on the info line in `locale`, and once it ran, the prompt is cleared
    /// where `clear`.
    Dispatch {
        /// The command.
        command: String,
        /// Its arguments.
        arguments: nc_canon::Value,
        /// Whether the prompt is cleared once it ran.
        clear: bool,
        /// The locale a refusal is said in.
        locale: nc_catalog::Locale,
    },
    /// A note of what this window said at the rate it draws.
    Note(nc_ipc::Note),
    /// The registry, in each locale: the commands the list shows.
    Registry,
}

impl Ask {
    /// The command it dispatches; nothing for a note or the registry, which
    /// dispatch nothing.
    #[must_use]
    pub fn command(&self) -> Option<&str> {
        match self {
            Self::Close(_) => Some("window.close"),
            Self::Run(action) => Some(action.command()),
            Self::Dispatch { command, .. } => Some(command),
            Self::Note(_) | Self::Registry => None,
        }
    }

    /// The arguments it dispatches its command with; nothing for a note or
    /// the registry.
    #[must_use]
    pub fn arguments(&self) -> Option<nc_canon::Value> {
        match self {
            Self::Close(window) => Some(nc_canon::Value::object([(
                "window",
                nc_canon::Value::Integer(i64::try_from(*window).unwrap_or(i64::MAX)),
            )])),
            Self::Run(action) => Some(action.arguments()),
            Self::Dispatch { arguments, .. } => Some(arguments.clone()),
            Self::Note(_) | Self::Registry => None,
        }
    }

    /// `command` run with no argument, its refusal said in `locale`.
    #[must_use]
    pub fn bare(command: &str, locale: nc_catalog::Locale) -> Self {
        Self::Dispatch {
            command: command.to_owned(),
            arguments: nc_canon::Value::object(Vec::<(&str, nc_canon::Value)>::new()),
            clear: false,
            locale,
        }
    }
}

/// What the info line says of `refusal`, in `locale`: its code, and its
/// message in that locale — the catalog's sentence, which is what a person
/// reads (`NOCTUA.md §15.1`).
#[must_use]
pub fn refused_said(refusal: &nc_ipc::Refusal, locale: nc_catalog::Locale) -> String {
    let code = refusal.code;
    format!(
        "[{}] {}",
        code.code(),
        nc_catalog::message(code.message_key(), locale).unwrap_or(refusal.sentence.as_str())
    )
}

/// `info.say` with `text`, as the window runs it.
fn say_on_the_info_line(text: &str) -> nc_canon::Value {
    nc_canon::Value::object([("text", nc_canon::Value::text(text))])
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
/// one request at a time, until the loop has ended; whether a note was
/// taken reaches the loop through `proxy`.
fn asking(
    found: Found,
    proxy: EventLoopProxy<Arrived>,
) -> std::io::Result<std::sync::mpsc::Sender<Ask>> {
    let runtime = runtime()?;
    let (sender, asks) = std::sync::mpsc::channel::<Ask>();
    std::thread::Builder::new()
        .name("noctua-asking".to_owned())
        .spawn(move || {
            let mut asker = Asker::new(found);
            while let Ok(ask) = asks.recv() {
                match ask {
                    Ask::Note(note) => {
                        let taken = runtime.block_on(asker.note(note)).is_ok();
                        // A loop that has ended has no use for the answer.
                        let _ = proxy.send_event(Arrived::Told(taken));
                    }
                    Ask::Registry => {
                        let listings: Option<Vec<_>> = nc_catalog::Locale::ALL
                            .iter()
                            .map(|locale| runtime.block_on(asker.registry(*locale)).ok())
                            .collect();
                        if let Some(listings) = listings {
                            let _ = proxy
                                .send_event(Arrived::Registry(Commands::from_registry(&listings)));
                        }
                    }
                    Ask::Dispatch {
                        command,
                        arguments,
                        clear,
                        locale,
                    } => match runtime.block_on(asker.dispatch(&command, arguments)) {
                        Ok(Ok(_)) if clear => {
                            let _ = runtime.block_on(asker.dispatch(
                                "prompt.clear",
                                nc_canon::Value::object(Vec::<(&str, nc_canon::Value)>::new()),
                            ));
                        }
                        Ok(Err(refusal)) => {
                            let said = refused_said(&refusal, locale);
                            let _ = runtime
                                .block_on(asker.dispatch("info.say", say_on_the_info_line(&said)));
                        }
                        _ => {}
                    },
                    other => {
                        if let (Some(command), Some(args)) = (other.command(), other.arguments()) {
                            // What the daemon answers reaches the window
                            // through the state it then holds.
                            let _ = runtime.block_on(asker.dispatch(command, args));
                        }
                    }
                }
            }
        })?;
    Ok(sender)
}

/// Sends the daemon the note of what this window said and has not told it,
/// where there is one and none is in flight.
fn tell(mirror: &mut Mirror, asks: &std::sync::mpsc::Sender<Ask>) {
    // A note past its bound took its facts back, and says nothing.
    if let Some(Ok(note)) = mirror.to_tell()
        && asks.send(Ask::Note(note)).is_err()
    {
        mirror.told(false);
    }
}

/// Whether the desktop of an install asks the system for the Leader, where
/// `listener` is the process that does on this system: the desktop, and never
/// for an install a test `relocated`, which asks the system for nothing
/// (§12.8).
#[must_use]
pub const fn asks(relocated: bool, listener: nc_ipc::leader::Listener) -> bool {
    matches!(listener, nc_ipc::leader::Listener::Desktop) && !relocated
}

/// Whether the keys in force are read again: the daemon's count of the writes
/// of the keybinding configuration, `said`, is another number than the one
/// they were `read` at — larger or smaller, since the count turns to zero as
/// a revision does and a daemon started again begins at none.
#[must_use]
pub const fn reads_the_keys_again(read: u64, said: u64) -> bool {
    read != said
}

/// What the info line says where a compositor's binding delivers the Leader:
/// the code a session that registers no global key is told by, its message
/// in `locale`, and the binding to make — the Leader, as a Linux keyboard
/// labels it, running `cli leader`.
#[must_use]
pub fn bound_said(
    leader: nc_schema::keys::Leader,
    cli: &std::path::Path,
    locale: nc_catalog::Locale,
) -> String {
    format!(
        "{} {}: {} leader",
        refused_said(
            &nc_ipc::leader::refusal(&nc_platform::leader::Refused::ByBinding),
            locale
        ),
        leader.written_for(nc_schema::keys::Keyboard::Linux),
        cli.display()
    )
}

/// Asks this system for the Leader of `keymap`, for the install `found`,
/// where its desktop is the listener: what is held for as long as the window
/// runs, and what the info line is to say where nothing is — the refusal,
/// or the binding a compositor delivers it by. A press, and a registration
/// lost, reach the loop through `proxy`.
fn listened(
    found: &Found,
    keymap: &Keymap,
    proxy: &EventLoopProxy<Arrived>,
    locale: nc_catalog::Locale,
) -> (Option<nc_platform::leader::Held>, Option<String>) {
    if !asks(
        found.install.namespace.is_some(),
        nc_ipc::leader::listener(),
    ) {
        return (None, None);
    }
    let variable = |name: &str| std::env::var_os(name);
    let leader = keymap.part().leader;
    let (pressed, lost) = (proxy.clone(), proxy.clone());
    let listening = nc_ipc::leader::listen(nc_platform::leader::delivery(&variable), || {
        nc_platform::leader::register(
            leader,
            &variable,
            move || {
                // A loop that has ended has no use for a press.
                let _ = pressed.send_event(Arrived::Pressed);
            },
            move |why| {
                let said = refused_said(&nc_ipc::leader::refusal(&why), locale);
                let _ = lost.send_event(Arrived::Unheard(said));
            },
        )
    });
    match listening {
        Ok(nc_ipc::leader::Listening::Held(held)) => (Some(held), None),
        Ok(nc_ipc::leader::Listening::ByBinding) => (
            None,
            Some(bound_said(
                leader,
                &found.layout.install.join(&found.layout.cli),
                locale,
            )),
        ),
        Err(refusal) => (None, Some(refused_said(&refusal, locale))),
    }
}

/// The commands that open a page, which close the leader menu themselves
/// (§12.9): every other entry the window closes the menu for before it runs,
/// so a page's flip is never spent by the window.
pub const OPENS_A_PAGE: [&str; 6] = [
    "page.open",
    "page.settings",
    "page.help",
    "page.dev",
    "page.shortcuts",
    "page.actions",
];

/// What the window asks to run the leader menu's entry `command`, the menu
/// open as `menu` where it is: the menu closed first — unless the command
/// opens a page, which closes it and spends the menu's flip — then the
/// command, with no argument.
#[must_use]
pub fn entry_asks(command: &str, menu: Option<u64>, locale: nc_catalog::Locale) -> Vec<Ask> {
    let mut asks = Vec::new();
    if !OPENS_A_PAGE.contains(&command)
        && let Some(menu) = menu
    {
        asks.push(Ask::Dispatch {
            command: "page.close".to_owned(),
            arguments: nc_canon::Value::object([
                (
                    "page",
                    nc_canon::Value::Integer(i64::try_from(menu).unwrap_or(i64::MAX)),
                ),
                ("in", nc_canon::Value::text("drawer")),
            ]),
            clear: false,
            locale,
        });
    }
    asks.push(Ask::bare(command, locale));
    asks
}

/// What the window asks once `command` is chosen from a list, the prompt
/// asking for something where `asking`: the command run at once where it
/// takes no parameter — the prompt cleared once it ran, where it asked — and
/// otherwise the prompt kept on it, asking for its parameters; and whether
/// the prompt is then emptied and given the keyboard.
#[must_use]
pub fn chosen_asks(
    command: &crate::commands::Command,
    asking: bool,
    locale: nc_catalog::Locale,
) -> (Ask, bool) {
    if command.parameters.is_empty() {
        return (
            Ask::Dispatch {
                command: command.name.clone(),
                arguments: nc_canon::Value::object(Vec::<(&str, nc_canon::Value)>::new()),
                clear: asking,
                locale,
            },
            false,
        );
    }
    (
        Ask::Dispatch {
            command: "prompt.ask".to_owned(),
            arguments: nc_canon::Value::object([(
                "command",
                nc_canon::Value::text(command.name.as_str()),
            )]),
            clear: false,
            locale,
        },
        true,
    )
}

/// What choosing `value` from the prompt's list comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// A command of the command list, chosen.
    Command(String),
    /// The prompt's draft completed with a parameter or a value offered.
    Draft(String),
}

/// What choosing `value` from the prompt's list comes to, the prompt asking
/// for `asking` and holding `draft`: a command the list holds, while it asks
/// for the command list; the draft completed by an offer the list makes, while
/// it asks for a command's parameters; nothing for any other value.
#[must_use]
pub fn picked(
    asking: Option<&crate::mirror::Asking>,
    draft: &str,
    value: &str,
    commands: &Commands,
) -> Option<Picked> {
    match asking? {
        crate::mirror::Asking::Commands => commands
            .named(value)
            .map(|command| Picked::Command(command.name.clone())),
        crate::mirror::Asking::Parameters { command, .. } => {
            let command = commands.named(command)?;
            crate::commands::offered(draft, command)
                .into_iter()
                .find(|offer| offer.value() == value)
                .map(|offer| Picked::Draft(crate::commands::completed(draft, &offer)))
        }
    }
}

/// What the window asks once the prompt is sent with no option chosen,
/// asking for `asking` and holding `draft`: the command it asks the
/// parameters of, run with what the draft gives it and the prompt cleared
/// once it ran; nothing while it asks for anything else.
#[must_use]
pub fn sent_asks(
    asking: Option<&crate::mirror::Asking>,
    draft: &str,
    commands: &Commands,
    locale: nc_catalog::Locale,
) -> Option<Ask> {
    let Some(crate::mirror::Asking::Parameters { command, .. }) = asking else {
        return None;
    };
    let command = commands.named(command)?;
    Some(Ask::Dispatch {
        command: command.name.clone(),
        arguments: crate::commands::arguments(draft, command),
        clear: true,
        locale,
    })
}

/// What Esc runs: `prompt.clear` while the prompt asks for something, which
/// closes its list, and otherwise `drawer.back`, which closes what is focused
/// in the drawer or goes back one sub-page there (§12.8, §12.11).
#[must_use]
pub const fn escaped(asking: bool) -> &'static str {
    if asking {
        "prompt.clear"
    } else {
        "drawer.back"
    }
}

/// Whether `value`, an option the page says the keyboard moved to, is a
/// target a seat's selection may name: one to 64 ASCII letters, digits or
/// `-_.:/`, as `shell.hover` takes one.
#[must_use]
pub fn is_target(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte))
}

/// What the window says once the page said the system's own pointer is over
/// `target`, or over nothing that names one: that seat's pointer fact, where
/// it differs from what `mirror` holds of it — as this window said it, or as
/// the state holds it — and nothing where it is the same, so a pointer that
/// stays over one target is said once.
#[must_use]
pub fn hover_said(mirror: &Mirror, target: Option<String>) -> Option<Fact> {
    let differs =
        mirror.hovered(crate::seat::SYSTEMS_OWN, crate::mirror::By::Pointer) != target.as_deref();
    differs.then_some(Fact::Hover {
        seat: crate::seat::SYSTEMS_OWN,
        by: crate::mirror::By::Pointer,
        target,
    })
}

/// What the window says once the page said the system's own keyboard moved
/// to the option `value` of a list: that seat's keyboard fact, its selection
/// on `value`, where `value` is a target `shell.hover` takes; nothing for any
/// other value.
#[must_use]
pub fn selection_said(value: String) -> Option<Fact> {
    is_target(&value).then_some(Fact::Hover {
        seat: crate::seat::SYSTEMS_OWN,
        by: crate::mirror::By::Keyboard,
        target: Some(value),
    })
}

/// `value`, a size or a place the state holds, as the system's pixels a
/// window is asked in: the state holds each to a bound far inside `i32`.
fn pixels(value: i64) -> f64 {
    f64::from(i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX }))
}

/// `value`, a coordinate the system said in logical pixels, rounded to the
/// nearest whole one: what a test's install prints of where a window is.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a coordinate on a screen, rounded first, and a value past `i64` saturates, \
              which `as` does for a float"
)]
fn rounded(value: f64) -> i64 {
    value.round() as i64
}

/// What the event loop keeps beside the window: the state mirrored, the keys
/// in force, the commands the list shows, the Leader as heard, where the
/// keyboard was last put, the size and the place last applied, and, for a
/// test's install, where what the window shows is said.
struct Running<'said> {
    /// What the window knows of itself.
    context: Context,
    /// The interface's state, as this window holds it.
    mirror: Mirror,
    /// Whether the window is shown.
    showing: Showing,
    /// The keys in force.
    keymap: Keymap,
    /// The commands the list shows.
    commands: Commands,
    /// The Leader, as the window hears it.
    leader: crate::keys::Leader,
    /// Whether the window has the keyboard.
    focused: bool,
    /// Where the window last put seat 1's keyboard.
    placed: Option<Region>,
    /// What the info line is to say of the Leader, where it is not heard.
    unheard: Option<String>,
    /// The install whose keys these are, read again when a setting wrote
    /// them.
    found: Found,
    /// What a press of the Leader, and a registration lost, reach the loop
    /// through.
    proxy: EventLoopProxy<Arrived>,
    /// The Leader this window asked the system for, held on this thread, the
    /// event loop's, until the loop has ended or a setting changed it.
    held: Option<nc_platform::leader::Held>,
    /// The count of writes of the keybinding configuration the keys in force
    /// were read at (`Mirror::keys`).
    keys_read: u64,
    /// The asking thread.
    asks: std::sync::mpsc::Sender<Ask>,
    /// The size the state asked last that the window applied.
    sized: Option<(i64, i64)>,
    /// The place the state asked last that the window applied.
    moved: Option<(i64, i64)>,
    /// Where a line saying what the window and its page show goes, for a
    /// test's install alone ([`witnessed_line`]).
    witness: Option<&'said mut dyn FnMut(&str)>,
}

impl Running<'_> {
    /// Sends `ask` to the asking thread, which outlives the loop.
    fn ask(&self, ask: Ask) {
        let _ = self.asks.send(ask);
    }

    /// `command` with `arguments`, run as the person; once it ran, the prompt
    /// is cleared where `clear`.
    fn run(&self, command: &str, arguments: nc_canon::Value, clear: bool) {
        self.ask(Ask::Dispatch {
            command: command.to_owned(),
            arguments,
            clear,
            locale: self.context.locale,
        });
    }

    /// Takes `fact`, which this window says, and tells the daemon.
    fn said(&mut self, fact: Fact) {
        self.mirror.said(fact);
        tell(&mut self.mirror, &self.asks);
    }

    /// The window `mirror` shows: its identifier.
    fn window(&self) -> Option<u64> {
        self.mirror
            .shell()
            .and_then(this_window)
            .map(|window| window.id)
    }

    /// The leader menu's page, while it is open in focus in the drawer.
    fn menu(&self) -> Option<u64> {
        let shell = self.mirror.shell()?;
        let id = shell.drawer.focused.filter(|_| !shell.drawer.collapsed)?;
        (shell.pages.get(&id)?.kind == crate::mirror::Kind::Leader).then_some(id)
    }

    /// The window sized and placed as the state asks, where it asks what the
    /// window has not applied yet.
    fn arranged(&mut self, opened: &mut Opened) {
        let Some(window) = self.mirror.shell().and_then(this_window) else {
            return;
        };
        let (size, place) = (window.size, window.place);
        if let Some((width, height)) = asked_anew(size, self.sized) {
            opened.size_to(LogicalSize::new(pixels(width), pixels(height)));
            self.sized = Some((width, height));
        }
        if let Some((x, y)) = asked_anew(place, self.moved) {
            opened.place_at(LogicalPosition::new(pixels(x), pixels(y)));
            self.moved = Some((x, y));
        }
    }

    /// The page asked what it shows, for a test's install, once its document
    /// has loaded.
    fn witness(&self, opened: &Opened) {
        if self.witness.is_some() && self.showing.is_loaded() {
            opened.send(vec!["window.noctua.witness();".to_owned()]);
        }
    }

    /// What the page said it shows, `seen`, printed with what the system says
    /// of the window and the digest of what the page holds, for a test's
    /// install.
    fn witnessed(&mut self, opened: &Opened, seen: &Witnessed) {
        if let Some(witness) = self.witness.as_mut() {
            let scale_factor = opened.window.scale_factor();
            let at = opened.window.outer_position().ok().map(|corner| {
                let corner = corner.to_logical::<f64>(scale_factor);
                (rounded(corner.x), rounded(corner.y))
            });
            let window = Windowed {
                shown: opened.window.is_visible(),
                decorated: opened.window.is_decorated(),
                at,
            };
            witness(&witnessed_line(window, opened.drawing.digest(), seen));
        }
    }

    /// The footer, the drawer and the sidebar drawn from the state and what
    /// this window said, every seat but the system's own told to the page,
    /// and seat 1's keyboard put where the state says.
    fn redraw(&mut self, opened: &mut Opened) {
        let draft = self.mirror.draft().unwrap_or_default().to_owned();
        let selected = self
            .mirror
            .hovered(1, crate::mirror::By::Keyboard)
            .map(str::to_owned);
        let window = self.window();
        let shell = self.mirror.shell();
        let drawer = crate::drawer::view(
            shell,
            &draft,
            selected.as_deref(),
            (&self.keymap, &self.commands),
            self.context.locale,
        );
        let sidebar = crate::sidebar::view(shell, window, selected.as_deref(), self.context.locale);
        let footer = shell.map(|shell| {
            let mut footer = crate::footer::view(Some(shell), self.context);
            footer.controls = drawer.controls();
            (crate::footer::markup(&footer), shell.draft.as_str())
        });
        let mut scripts = opened.drawing.parts(
            footer,
            crate::drawer::markup(&drawer),
            crate::sidebar::markup(&sidebar),
        );
        let told = match (shell, window) {
            (Some(shell), Some(window)) => crate::seat::script(&crate::seat::told(shell, window)),
            _ => crate::seat::NONE.to_owned(),
        };
        let redrawn = !scripts.is_empty();
        scripts.extend(opened.drawing.seats(told, redrawn));
        opened.send(scripts);
        self.keyboard(opened);
    }

    /// Seat 1's keyboard put where the state says, once the document has
    /// loaded and been drawn — a place put before then would be lost with
    /// the page that could not take it — and only when the state moved it
    /// from where the window last put it or the page last said it went: on a
    /// part's own element, or given up to nothing for the workspace area,
    /// which holds nothing a keyboard is on yet. A part that cannot take it —
    /// not drawn, hidden or inert — the page answers with where the keyboard
    /// is, which this window then says as the seat's focus, so the state
    /// follows the page ([`crate::page::SCRIPT`]).
    fn keyboard(&mut self, opened: &Opened) {
        if !self.showing.is_loaded() {
            return;
        }
        let region = self
            .window()
            .and_then(|window| self.mirror.focus(1).filter(|focus| focus.window == window))
            .map(|focus| focus.region);
        if region != self.placed {
            self.placed = region;
            if let Some(region) = region {
                opened.send(vec![format!("window.noctua.focus('{}');", region.word())]);
            }
        }
    }

    /// Runs what a press came to.
    fn act(&self, act: Option<Act>) {
        match act {
            Some(Act::Tap) => self.run(nc_ipc::leader::TAP, nc_ipc::leader::tap(), false),
            Some(Act::Run(command)) => self.ask(Ask::bare(&command, self.context.locale)),
            None => {}
        }
    }

    /// Runs the leader menu's entry `command` ([`entry_asks`]).
    fn entry(&self, command: &str) {
        for ask in entry_asks(command, self.menu(), self.context.locale) {
            self.ask(ask);
        }
    }

    /// The prompt holds `text`, which this window wrote into it.
    fn written(&mut self, opened: &mut Opened, text: &str) {
        let (said, scripts) = opened.drawing.written(text);
        opened.send(scripts);
        self.said(Fact::Draft(said));
        self.redraw(opened);
    }

    /// The command `name` chosen from a list: run at once where it takes no
    /// parameter, and otherwise the prompt kept on it, asking for them.
    fn chose(&mut self, opened: &mut Opened, name: &str) {
        let Some(command) = self.commands.named(name) else {
            return;
        };
        let asking = self
            .mirror
            .shell()
            .is_some_and(|shell| shell.asking.is_some());
        let (ask, parameters) = chosen_asks(command, asking, self.context.locale);
        self.ask(ask);
        if !parameters {
            return;
        }
        self.written(opened, "");
        if let Some(window) = self.window() {
            self.said(Fact::Focus {
                seat: 1,
                on: crate::mirror::Focus {
                    window,
                    region: Region::Prompt,
                },
            });
        }
        opened.send(vec!["window.noctua.focus('prompt');".to_owned()]);
        self.placed = Some(Region::Prompt);
    }

    /// The prompt's list chose `value`: a command of the command list, or a
    /// parameter or a value offered for the command the prompt asks about.
    fn picked(&mut self, opened: &mut Opened, value: &str) {
        let asking = self.mirror.shell().and_then(|shell| shell.asking.clone());
        let draft = self.mirror.draft().unwrap_or_default().to_owned();
        match picked(asking.as_ref(), &draft, value, &self.commands) {
            Some(Picked::Command(name)) => self.chose(opened, &name),
            Some(Picked::Draft(completed)) => self.written(opened, &completed),
            None => {}
        }
    }

    /// The prompt was sent with no option chosen: the command it asks the
    /// parameters of run with what the prompt holds, and the prompt cleared
    /// once it ran.
    fn sent(&self) {
        let asking = self.mirror.shell().and_then(|shell| shell.asking.as_ref());
        let draft = self.mirror.draft().unwrap_or_default();
        if let Some(ask) = sent_asks(asking, draft, &self.commands, self.context.locale) {
            self.ask(ask);
        }
    }

    /// Takes what the page said of the keys, the lists and the focus.
    fn heard(&mut self, opened: &mut Opened, said: Said) {
        match said {
            Said::LeaderDown => self.leader.down(),
            Said::LeaderUp => {
                let act = self.leader.up();
                self.act(act);
            }
            Said::Held(key, with_chord) => {
                let act = self.leader.key(key, with_chord, &self.keymap);
                self.act(act);
            }
            Said::InMenu(key) => {
                if self.menu().is_some()
                    && let Some(Act::Run(command)) = crate::keys::in_menu(key, &self.keymap)
                {
                    self.entry(&command);
                }
            }
            Said::Choose(List::Menu, command) => {
                if self.keymap.is_entry(&command) {
                    self.entry(&command);
                }
            }
            Said::Choose(List::Actions, command) => {
                if crate::drawer::ACTIONS.contains(&command.as_str()) {
                    self.chose(opened, &command);
                }
            }
            Said::Choose(List::Commands, value) | Said::Send(Some(value)) => {
                self.picked(opened, &value);
            }
            Said::Choose(List::Tabs, value) => {
                if let Some(page) = self
                    .mirror
                    .shell()
                    .and_then(|shell| crate::drawer::tab(shell, &value))
                {
                    let (command, arguments) =
                        crate::drawer::focused(page, crate::mirror::Place::Drawer, None);
                    self.run(&command, arguments, false);
                }
            }
            Said::Choose(List::Sidebar, value) => {
                if let Some((page, window)) = self.window().and_then(|window| {
                    let shell = self.mirror.shell()?;
                    crate::sidebar::chosen(shell, window, &value).map(|page| (page, window))
                }) {
                    let (command, arguments) = crate::sidebar::chosen_run(page, window);
                    self.run(&command, arguments, false);
                }
            }
            Said::Control(control) => {
                if let Some(shell) = self.mirror.shell() {
                    let (command, arguments) = control.run(shell);
                    self.run(&command, arguments, false);
                }
            }
            Said::Send(None) => self.sent(),
            Said::Active(_, value) => {
                if let Some(fact) = selection_said(value) {
                    self.said(fact);
                }
            }
            Said::Focus(region) => {
                self.placed = Some(region);
                if let Some(window) = self.window() {
                    let on = crate::mirror::Focus { window, region };
                    if self.mirror.focus(1) != Some(on) {
                        self.said(Fact::Focus { seat: 1, on });
                    }
                }
            }
            Said::Escape => {
                let asking = self
                    .mirror
                    .shell()
                    .is_some_and(|shell| shell.asking.is_some());
                self.ask(Ask::bare(escaped(asking), self.context.locale));
            }
            Said::Hover(target) => {
                if let Some(fact) = hover_said(&self.mirror, target) {
                    self.said(fact);
                }
            }
            Said::Choose(List::Shortcuts, _)
            | Said::Loaded
            | Said::Scale(_)
            | Said::Action(_)
            | Said::Draft(_)
            | Said::FirstFrame(_)
            | Said::Settled(_)
            | Said::Viewport(..)
            | Said::Exercised(_)
            | Said::Seen(_) => {}
        }
    }

    /// Reads the keys in force again, once a shortcut setting wrote the
    /// channel's keybinding configuration (§12.15): the page is told them,
    /// and a Leader that changed is asked of the system — **the new chord
    /// first, and the old one let go once it was asked for** — with what came
    /// of asking said on the info line as it is when the window opens.
    fn rebound(&mut self, opened: &mut Opened) {
        let keymap = Keymap::of(&nc_channel::keys::effective(&self.found));
        if keymap.part().leader != self.keymap.part().leader {
            let (held, unheard) = listened(&self.found, &keymap, &self.proxy, self.context.locale);
            drop(std::mem::replace(&mut self.held, held));
            if let Some(said) = &unheard {
                self.run("info.say", say_on_the_info_line(said), false);
            }
            self.unheard = unheard;
        }
        self.keymap = keymap;
        opened.send(vec![format!(
            "window.noctua.keys({});",
            self.keymap.told().to_canonical()
        )]);
    }

    /// Takes what reached the event loop: the page's words, the daemon's
    /// state, a press of the Leader, the registry, and the window's own
    /// events.
    fn on(&mut self, opened: &mut Opened, event: Event<'_, Arrived>) {
        match event {
            Event::UserEvent(Arrived::Said { said, .. }) => match said {
                Said::Loaded => {
                    self.showing.loaded();
                    let mut scripts = opened.drawing.loaded();
                    scripts.push(format!(
                        "window.noctua.keys({});",
                        self.keymap.told().to_canonical()
                    ));
                    opened.send(scripts);
                    self.keyboard(opened);
                    apply(&opened.window, self.showing);
                    self.witness(opened);
                }
                Said::Scale(thousandths) => {
                    opened.fit(thousandths);
                    self.witness(opened);
                }
                Said::Seen(seen) => self.witnessed(opened, &seen),
                Said::Action(action) => self.ask(Ask::Run(action)),
                Said::Draft(text) => {
                    let (draft, scripts) = opened.drawing.typed(&text);
                    opened.send(scripts);
                    self.said(Fact::Draft(draft));
                    self.redraw(opened);
                }
                Said::FirstFrame(_)
                | Said::Settled(_)
                | Said::Viewport(..)
                | Said::Exercised(_) => {}
                other => self.heard(opened, other),
            },
            Event::UserEvent(Arrived::Told(taken)) => {
                self.mirror.told(taken);
                tell(&mut self.mirror, &self.asks);
            }
            Event::UserEvent(Arrived::Heard(Heard::Reached)) => {
                self.mirror.reconnected();
                self.ask(Ask::Registry);
                if let Some(said) = self.unheard.clone() {
                    self.run("info.say", say_on_the_info_line(&said), false);
                }
            }
            Event::UserEvent(Arrived::Heard(Heard::Changed(changed))) => {
                // A state this build does not read leaves the one held drawn.
                if self.mirror.watched(changed).is_ok() {
                    if reads_the_keys_again(self.keys_read, self.mirror.keys()) {
                        self.keys_read = self.mirror.keys();
                        self.rebound(opened);
                    }
                    self.redraw(opened);
                    self.arranged(opened);
                    if let Some(shown) = shown(&self.mirror) {
                        self.showing.want(shown);
                        apply(&opened.window, self.showing);
                    }
                    self.witness(opened);
                }
            }
            Event::UserEvent(Arrived::Pressed) => {
                let focused = self.focused && self.showing.shows();
                let act = self.leader.pressed(focused);
                self.act(act);
                if self.leader.is_held() {
                    opened.send(vec!["window.noctua.held();".to_owned()]);
                }
            }
            Event::UserEvent(Arrived::Unheard(said)) => {
                self.run("info.say", say_on_the_info_line(&said), false);
                self.unheard = Some(said);
            }
            Event::UserEvent(Arrived::Registry(commands)) => {
                self.commands = commands;
                self.redraw(opened);
                self.witness(opened);
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(focused),
                ..
            } => {
                self.focused = focused;
                if !focused {
                    self.leader.blurred();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Moved(_),
                ..
            } => self.witness(opened),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if let Some(window) = self.window() {
                    self.ask(Ask::Close(window));
                }
            }
            _ => {}
        }
    }
}

/// Runs the interface for the daemon of `found`'s install: a window opened
/// hidden in the theme the system prefers, shown and hidden as the daemon's
/// state says once its document has loaded, its footer and its drawer drawn
/// from the state, the Leader asked of the system where this window is its
/// listener, and every key, list and control run as the command it names,
/// until the event loop ends — which nothing in this build asks it to.
///
/// **A test's install is told what the window shows** — an install relocated
/// under a namespace, which a real install never is: whenever the state, the
/// page's scale, the registry or the window's place moved, the page is asked
/// what it shows, and
/// [`witnessed_line`] of it is handed to `said`, which the binary prints.
/// Nothing is handed to it for any other install.
///
/// # Errors
///
/// [`Unopened`], as [`open`] refuses, and for a thread not started.
pub fn run(found: Found, said: &mut dyn FnMut(&str)) -> Result<(), Unopened> {
    let mut event_loop = EventLoopBuilder::<Arrived>::with_user_event().build();
    let context = Context {
        locale: nc_catalog::Locale::En,
        channel: found.install.channel,
    };
    let keymap = Keymap::of(&nc_channel::keys::effective(&found));
    let mut opened = open(
        &event_loop,
        event_loop.create_proxy(),
        (context, &found.layout.webview),
        Mode::DEFAULT,
        (0, false),
    )?;
    // **Held on this thread**, the event loop's, as a registration must be
    // (`nc_platform::leader`), and given back when the loop has ended.
    let (held, unheard) = listened(&found, &keymap, &event_loop.create_proxy(), context.locale);
    let witness = found.install.namespace.is_some().then_some(said);
    following(found.clone(), event_loop.create_proxy()).map_err(Unopened::Thread)?;
    let asks = asking(found.clone(), event_loop.create_proxy()).map_err(Unopened::Thread)?;
    let mut running = Running {
        context,
        mirror: Mirror::new(),
        showing: Showing::default(),
        keymap,
        commands: Commands::from_catalog(),
        leader: crate::keys::Leader::default(),
        focused: false,
        placed: None,
        unheard,
        found,
        proxy: event_loop.create_proxy(),
        held,
        keys_read: 0,
        asks,
        sized: None,
        moved: None,
        witness,
    };
    running.ask(Ask::Registry);
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::Wait;
        running.on(&mut opened, event);
    });
    drop(opened);
    drop(running.held.take());
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use nc_canon::Value;
    use nc_ipc::client::Changed;
    use tao::dpi::LogicalSize;

    use nc_schema::keys::Key;

    use super::{
        Ask, Said, Showing, Windowed, Witnessed, asked_anew, at_least, fitted, hover_said, least,
        said, shown, this_window, within, witnessed_line,
    };
    use crate::drawer::List;
    use crate::footer::Action;
    use crate::mirror::{Fact, Mirror, Region};

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
    pub(crate) fn this_window_is_the_states_first_shown_as_it_says() {
        let mut mirror = Mirror::new();
        assert_eq!(shown(&mirror), None);
        for (windows, first) in [(&[true, false][..], true), (&[false, true], false)] {
            mirror
                .watched(Changed {
                    revision: 1,
                    state: Some(crate::mirror::tests::as_watched(state(windows))),
                })
                .expect("the state is read");
            assert_eq!(shown(&mirror), Some(first), "{windows:?}");
            let shell = mirror.shell().expect("a state is held");
            assert_eq!(this_window(shell).map(|window| window.id), Some(1));
        }
    }

    /// The keys in force are read again when the daemon's count of the
    /// writes of the keybinding configuration is another number than the one
    /// they were read at, whichever way it moved, and not while it is the
    /// same.
    #[test]
    fn the_keys_are_read_again_when_the_count_of_their_writes_is_another() {
        assert!(!super::reads_the_keys_again(0, 0));
        assert!(!super::reads_the_keys_again(4, 4));
        assert!(super::reads_the_keys_again(0, 1));
        assert!(super::reads_the_keys_again(4, 5));
        assert!(super::reads_the_keys_again(4, 0), "a daemon started again");
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
        assert!(!showing.is_loaded());
        showing.loaded();
        assert!(showing.is_loaded());
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

    /// Closing the window is `window.close`, naming it; a control's action
    /// is its command, with no argument; and a note dispatches nothing.
    #[test]
    pub(crate) fn a_close_is_window_close_naming_the_window() {
        let close = Ask::Close(3);
        assert_eq!(close.command(), Some("window.close"));
        assert_eq!(
            close.arguments(),
            Some(Value::object([("window", Value::Integer(3))]))
        );
        for action in Action::ALL {
            let run = Ask::Run(action);
            assert_eq!(run.command(), Some(action.command()));
            assert_eq!(
                run.arguments(),
                Some(Value::object(Vec::<(&str, Value)>::new()))
            );
        }
        let note = nc_ipc::Note::new(Value::object([(
            "draft",
            Value::object([("text", Value::text("a"))]),
        )]))
        .expect("a note");
        let note = Ask::Note(note);
        assert_eq!((note.command(), note.arguments()), (None, None));
    }

    /// The webview's channel is read for the closed set of messages, each
    /// written one way, and nothing else is one.
    #[test]
    pub(crate) fn only_the_closed_set_is_read_from_the_webview() {
        assert_eq!(said(crate::page::LOADED), Some(Said::Loaded));
        assert_eq!(
            said("first-frame 1 2 3 255"),
            Some(Said::FirstFrame([1, 2, 3, 255]))
        );
        assert_eq!(said("settled 0 0 0 0"), Some(Said::Settled([0, 0, 0, 0])));
        assert_eq!(said("viewport 320 240"), Some(Said::Viewport(320, 240)));
        assert_eq!(said("scale 1042"), Some(Said::Scale(1042)));
        assert_eq!(said("scale 0"), Some(Said::Scale(0)));
        for action in Action::ALL {
            assert_eq!(
                said(&format!("action {}", action.command())),
                Some(Said::Action(action))
            );
        }
        for control in crate::drawer::Control::ALL {
            assert_eq!(
                said(&format!("action {}", control.word())),
                Some(Said::Control(control))
            );
        }
        for (message, text) in [
            ("draft ", ""),
            ("draft a", "a"),
            ("draft  two  spaces ", " two  spaces "),
            ("draft line\nbreak", "line\nbreak"),
            ("draft action sidebar.show", "action sidebar.show"),
            ("draft loaded", "loaded"),
        ] {
            assert_eq!(
                said(message),
                Some(Said::Draft(text.to_owned())),
                "{message:?}"
            );
        }
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
            "draft",
            "Draft a",
            " draft a",
            "drafta",
            "action",
            "action ",
            "action window.close",
            "action sidebar.show ",
            "action  sidebar.show",
            "action sidebar.show drawer.expand",
            "Action sidebar.show",
            "action drawer.expand ",
            "action drawer.back",
            "action page.focus",
        ] {
            assert_eq!(said(other), None, "{other:?}");
        }
    }

    /// What a probe's exercise found is read as its seven bits, in their
    /// order, each `1` or `0`, and nothing else is.
    #[test]
    fn an_exercise_is_read_as_its_seven_bits() {
        assert_eq!(
            said("exercised 1 0 1 0 1 0 1"),
            Some(Said::Exercised([
                true, false, true, false, true, false, true
            ]))
        );
        assert_eq!(
            said("exercised 0 1 0 1 0 1 0"),
            Some(Said::Exercised([
                false, true, false, true, false, true, false
            ]))
        );
        for other in [
            "exercised",
            "exercised 1 1",
            "exercised 1 1 1",
            "exercised 1 1 1 1",
            "exercised 1 1 1 1 1",
            "exercised 1 1 1 1 1 1",
            "exercised 1 1 1 1 1 1 1 1",
            "exercised 1 1 2 1 1 1 1",
            "exercised true 1 1 1 1 1 1",
            "exercised 1  1 1 1 1 1 1",
        ] {
            assert_eq!(said(other), None, "{other:?}");
        }
    }

    /// What the system's own pointer is over is read as a target of the
    /// shape `shell.hover` takes, or as nothing, and what a page says it shows
    /// in its one shape — every word of the closed set it is, the parts in
    /// their order, each seat's state a number, a state and a target, and no
    /// more of them than the seats the kernel allows can be in.
    #[test]
    pub(crate) fn the_pointer_and_the_witness_are_read_in_one_shape() {
        assert_eq!(said("hover"), Some(Said::Hover(None)));
        assert_eq!(
            said("hover info.expand"),
            Some(Said::Hover(Some("info.expand".to_owned())))
        );
        assert_eq!(said("hover 4"), Some(Said::Hover(Some("4".to_owned()))));
        let seen = said(
            "seen 320 240 1 0 0 1 drawer footer,drawer,sidebar 2:hover:info.expand \
             2:focus:prompt 12:drag:a:b",
        );
        assert_eq!(
            seen,
            Some(Said::Seen(Witnessed {
                page: (320, 240),
                fits: true,
                lines: false,
                reachable: false,
                styled: true,
                keyboard: "drawer".to_owned(),
                shows: vec![
                    "footer".to_owned(),
                    "drawer".to_owned(),
                    "sidebar".to_owned()
                ],
                seats: vec![
                    "2:hover:info.expand".to_owned(),
                    "2:focus:prompt".to_owned(),
                    "12:drag:a:b".to_owned(),
                ],
            }))
        );
        assert_eq!(
            said("seen 800 600 0 1 1 0 - -"),
            Some(Said::Seen(Witnessed {
                page: (800, 600),
                fits: false,
                lines: true,
                reachable: true,
                styled: false,
                keyboard: "-".to_owned(),
                shows: Vec::new(),
                seats: Vec::new(),
            }))
        );
        let most = usize::try_from(nc_catalog::KERNEL.shell_seats_max).expect("small") * 4;
        let seats = |count: usize| vec!["2:hover:x"; count].join(" ");
        assert!(said(&format!("seen 1 1 1 1 1 1 - footer {}", seats(most))).is_some());
        for other in [
            "hover ".to_owned(),
            "hover a b".to_owned(),
            "hover a#b".to_owned(),
            format!("hover {}", "a".repeat(65)),
            "Hover a".to_owned(),
            "seen".to_owned(),
            "seen 320 240 1 1 1 1 prompt".to_owned(),
            "seen 320 240 1 1 1 prompt footer".to_owned(),
            "seen 320 240 1 2 1 1 prompt footer".to_owned(),
            "seen 320 240 1 1 1 2 prompt footer".to_owned(),
            "seen 320 240 1 1 1 1 window footer".to_owned(),
            "seen 320 240 1 1 1 1 prompt drawer,footer".to_owned(),
            "seen 320 240 1 1 1 1 prompt footer,footer".to_owned(),
            "seen 320 240 1 1 1 1 prompt footer,page".to_owned(),
            "seen 320 240 1 1 1 1 prompt ".to_owned(),
            "seen 320 240 1 1 1 1 prompt footer 2:press:x".to_owned(),
            "seen 320 240 1 1 1 1 prompt footer two:hover:x".to_owned(),
            "seen 320 240 1 1 1 1 prompt footer 2:hover".to_owned(),
            "seen 320 240 1 1 1 1 prompt footer 2:hover:a,b".to_owned(),
            "seen -1 240 1 1 1 1 prompt footer".to_owned(),
            format!("seen 1 1 1 1 1 1 - footer {}", seats(most + 1)),
        ] {
            assert_eq!(said(&other), None, "{other:?}");
        }
    }

    /// What a test's install prints of its window is one line of `name=value`
    /// words, a list joined by commas and none written `-`.
    #[test]
    fn a_witnessed_line_names_each_part() {
        let seen = Witnessed {
            page: (320, 240),
            fits: true,
            lines: true,
            reachable: true,
            styled: false,
            keyboard: "prompt".to_owned(),
            shows: vec!["footer".to_owned(), "drawer".to_owned()],
            seats: vec![
                "2:hover:info.expand".to_owned(),
                "2:focus:prompt".to_owned(),
            ],
        };
        let window = Windowed {
            shown: true,
            decorated: false,
            at: Some((-4, 60)),
        };
        assert_eq!(
            witnessed_line(window, 0xab, &seen),
            "witnessed shown=1 decorated=0 at=-4,60 drawn=00000000000000ab page=320x240 fits=1 \
             lines=1 reachable=1 styled=0 keyboard=prompt shows=footer,drawer \
             seats=2:hover:info.expand,2:focus:prompt"
        );
        let none = Witnessed {
            fits: false,
            lines: false,
            reachable: false,
            styled: true,
            shows: Vec::new(),
            seats: Vec::new(),
            ..seen
        };
        let window = Windowed {
            shown: false,
            decorated: true,
            at: None,
        };
        assert_eq!(
            witnessed_line(window, u64::MAX, &none),
            "witnessed shown=0 decorated=1 at=- drawn=ffffffffffffffff page=320x240 fits=0 \
             lines=0 reachable=0 styled=1 keyboard=prompt shows=- seats=-"
        );
        assert_eq!(super::rounded(59.6), 60);
        assert_eq!(super::rounded(-4.4), -4);
    }

    /// The system's own pointer is said as seat 1's hover when it moves onto
    /// another target than the one the window holds — as it said it, or as
    /// the state holds it — and not again while it stays.
    #[test]
    pub(crate) fn the_systems_own_pointer_is_said_once_per_target() {
        let mut mirror = Mirror::new();
        let fact = |target: Option<&str>| Fact::Hover {
            seat: crate::seat::SYSTEMS_OWN,
            by: crate::mirror::By::Pointer,
            target: target.map(str::to_owned),
        };
        assert_eq!(
            hover_said(&mirror, None),
            None,
            "nothing held, nothing over"
        );
        assert_eq!(
            hover_said(&mirror, Some("info.expand".to_owned())),
            Some(fact(Some("info.expand")))
        );
        mirror.said(fact(Some("info.expand")));
        assert_eq!(hover_said(&mirror, Some("info.expand".to_owned())), None);
        assert_eq!(hover_said(&mirror, None), Some(fact(None)));
        let mut held = Mirror::new();
        let mut state = crate::mirror::tests::a_state_with_windows(Vec::new());
        if let Value::Object(fields) = &mut state {
            fields.insert(
                "seats".to_owned(),
                Value::Array(vec![Value::object([
                    ("seat", Value::Integer(1)),
                    ("latest", Value::text("pointer")),
                    ("hover", Value::text("drawer.expand")),
                ])]),
            );
        }
        held.watched(Changed {
            revision: 1,
            state: Some(crate::mirror::tests::as_watched(state)),
        })
        .expect("read");
        assert_eq!(hover_said(&held, Some("drawer.expand".to_owned())), None);
        assert_eq!(
            hover_said(&held, Some("info.expand".to_owned())),
            Some(fact(Some("info.expand")))
        );
    }

    /// The system's own keyboard moving to an option of a list is said as
    /// seat 1's selection on that option's value, for a value of the shape
    /// `shell.hover` takes and for no other.
    #[test]
    pub(crate) fn the_systems_own_selection_is_said_as_its_keyboards() {
        assert_eq!(
            super::selection_said("kernel.find".to_owned()),
            Some(Fact::Hover {
                seat: crate::seat::SYSTEMS_OWN,
                by: crate::mirror::By::Keyboard,
                target: Some("kernel.find".to_owned()),
            })
        );
        for refused in ["", "a b", "a#b", &"a".repeat(65)] {
            assert_eq!(
                super::selection_said(refused.to_owned()),
                None,
                "{refused:?}"
            );
        }
    }

    /// A size or a place the state asks is applied once: when it is
    /// something the window has not applied, and never when the state asks
    /// nothing.
    #[test]
    pub(crate) fn a_size_or_a_place_is_applied_once_per_change() {
        assert_eq!(asked_anew::<(i64, i64)>(None, None), None);
        assert_eq!(asked_anew(Some((320, 240)), None), Some((320, 240)));
        assert_eq!(asked_anew(Some((320, 240)), Some((320, 240))), None);
        assert_eq!(
            asked_anew(Some((800, 600)), Some((320, 240))),
            Some((800, 600))
        );
        assert_eq!(asked_anew(None, Some((320, 240))), None);
        assert_eq!(super::pixels(320), 320.0);
        assert_eq!(super::pixels(-32_768), -32_768.0);
        assert_eq!(super::pixels(i64::MAX), f64::from(i32::MAX));
        assert_eq!(super::pixels(i64::MIN), f64::from(i32::MIN));
    }

    /// The keys, the lists and the focus the page says are read in one
    /// spelling each — a key of the vocabulary by its code, a list of the
    /// four, a value of an option whole and within its bound — and nothing
    /// else is one.
    #[test]
    fn the_keys_the_lists_and_the_focus_are_read_in_one_spelling() {
        for other in [
            "leader",
            "leader down ",
            "Leader down",
            "leader left",
            "key held KeyQ",
            "key held KeyQ 2",
            "key held keyq 0",
            "key held AltLeft 0",
            "key held Escape 0",
            "key menu",
            "key menu Numpad1",
            "key menu KeyS 1",
            "choose",
            "choose leader",
            "choose leader ",
            "choose menu page.settings",
            "choose leader page.settings\n",
            "active commands",
            "active shortcut x",
            "send ",
            "send\n",
            "focus",
            "focus window",
            "focus Info",
            "focus prompt ",
            "escape ",
            "Escape",
        ] {
            assert_eq!(said(other), None, "{other:?}");
        }
        let long = format!("send {}", "a".repeat(super::VALUE_BYTES_MAX + 1));
        assert_eq!(said(&long), None, "a value past its bound");
        for (message, read) in [
            ("leader down", Said::LeaderDown),
            ("leader up", Said::LeaderUp),
            ("key held KeyQ 0", Said::Held(Key::KeyQ, false)),
            ("key held ArrowLeft 1", Said::Held(Key::ArrowLeft, true)),
            ("key menu Slash", Said::InMenu(Key::Slash)),
            (
                "choose leader page.settings",
                Said::Choose(List::Menu, "page.settings".to_owned()),
            ),
            (
                "choose commands value:two words",
                Said::Choose(List::Commands, "value:two words".to_owned()),
            ),
            (
                "active actions window.hide",
                Said::Active(List::Actions, "window.hide".to_owned()),
            ),
            ("send", Said::Send(None)),
            (
                "send parameter:kind",
                Said::Send(Some("parameter:kind".to_owned())),
            ),
            ("focus prompt", Said::Focus(Region::Prompt)),
            ("focus drawer", Said::Focus(Region::Drawer)),
            ("focus sidebar", Said::Focus(Region::Sidebar)),
            ("focus info", Said::Focus(Region::Info)),
            ("focus modeline", Said::Focus(Region::Modeline)),
            ("focus workspace", Said::Focus(Region::Workspace)),
            ("choose pages 4", Said::Choose(List::Tabs, "4".to_owned())),
            (
                "choose sidebar 3",
                Said::Choose(List::Sidebar, "3".to_owned()),
            ),
            ("escape", Said::Escape),
        ] {
            assert_eq!(said(message), Some(read), "{message:?}");
        }
    }

    /// A refusal is said by its code and its message in the window's
    /// locale; where a compositor's binding delivers the Leader, by the code
    /// of a session that registers no key, and the binding to make.
    #[test]
    fn a_leader_not_heard_is_said_by_its_code_and_message() {
        let refusal = nc_ipc::leader::refusal(&nc_platform::leader::Refused::Taken);
        for locale in nc_catalog::Locale::ALL {
            let said = super::refused_said(&refusal, *locale);
            let message = nc_catalog::message(
                nc_catalog::ErrorCode::ChannelLeaderTaken.message_key(),
                *locale,
            )
            .expect("a message");
            assert_eq!(
                said,
                format!(
                    "[{}] {message}",
                    nc_catalog::ErrorCode::ChannelLeaderTaken.code()
                )
            );
        }
        let leader = nc_schema::keys::Leader::of(
            nc_schema::keys::Chord::parse("Alt+Super+Space").expect("a chord"),
        )
        .expect("a Leader");
        let bound = super::bound_said(
            leader,
            std::path::Path::new("/home/a/bin/noctua-dev-0123abcd"),
            nc_catalog::Locale::En,
        );
        assert!(
            bound.starts_with(&format!(
                "[{}] ",
                nc_catalog::ErrorCode::ChannelLeaderNoSession.code()
            )),
            "{bound}"
        );
        assert!(
            bound.ends_with(" Alt+Super+Space: /home/a/bin/noctua-dev-0123abcd leader"),
            "{bound}"
        );
        for said in [
            super::refused_said(&refusal, nc_catalog::Locale::PtBR),
            bound,
        ] {
            assert!(
                !said.chars().any(nc_schema::unseen) && said.len() <= 4096,
                "`info.say` takes it: {said}"
            );
        }
    }

    /// The desktop asks the system for the Leader where it is the listener,
    /// and never for a test's install.
    #[test]
    fn the_desktop_asks_for_the_leader_where_it_listens_and_never_for_a_test() {
        use nc_ipc::leader::Listener;
        assert!(super::asks(false, Listener::Desktop));
        assert!(!super::asks(false, Listener::Daemon));
        assert!(!super::asks(true, Listener::Desktop));
        assert!(!super::asks(true, Listener::Daemon));
    }

    /// A menu entry that opens no page closes the open menu first, and one
    /// that opens a page leaves the menu, and its flip, to the kernel; every
    /// command that opens a page among the catalog's entries is one the
    /// window knows opens one.
    #[test]
    pub(crate) fn an_entry_closes_the_menu_first_unless_it_opens_a_page() {
        let en = nc_catalog::Locale::En;
        let bare = |command: &str| Ask::bare(command, en);
        assert_eq!(
            super::entry_asks("page.close", Some(7), en),
            [
                Ask::Dispatch {
                    command: "page.close".to_owned(),
                    arguments: Value::object([
                        ("page", Value::Integer(7)),
                        ("in", Value::text("drawer")),
                    ]),
                    clear: false,
                    locale: en,
                },
                bare("page.close"),
            ]
        );
        assert_eq!(
            super::entry_asks("sidebar.cycle", None, en),
            [bare("sidebar.cycle")]
        );
        for opens in super::OPENS_A_PAGE {
            assert_eq!(
                super::entry_asks(opens, Some(7), en),
                [bare(opens)],
                "{opens}"
            );
            assert!(nc_catalog::command(opens).is_some(), "{opens}");
        }
        for (command, bound) in nc_channel::keys::catalog_bindings() {
            let in_menu = bound
                .iter()
                .any(|binding| binding.table == nc_schema::keys::Table::Menu);
            if in_menu && command.starts_with("page.") && command != "page.close" {
                assert!(super::OPENS_A_PAGE.contains(&command.as_str()), "{command}");
            }
        }
        let run = bare("window.hide");
        assert_eq!(run.command(), Some("window.hide"));
        assert_eq!(
            run.arguments(),
            Some(Value::object(Vec::<(&str, Value)>::new()))
        );
        assert_eq!(
            (Ask::Registry.command(), Ask::Registry.arguments()),
            (None, None)
        );
    }

    /// A command chosen runs at once where it takes no parameter — clearing
    /// the prompt where it asked — and otherwise keeps the prompt on it.
    #[test]
    pub(crate) fn a_command_chosen_runs_or_asks_for_its_parameters() {
        let en = nc_catalog::Locale::En;
        let commands = crate::commands::Commands::from_catalog();
        let hide = commands.named("window.hide").expect("listed");
        let (ask, asks) = super::chosen_asks(hide, true, en);
        assert!(asks, "an optional parameter is a parameter");
        assert_eq!(ask.command(), Some("prompt.ask"));
        assert_eq!(
            ask.arguments(),
            Some(Value::object([("command", Value::text("window.hide"))]))
        );
        let shown = commands.named("sidebar.show").expect("listed");
        for asking in [true, false] {
            let (ask, asks) = super::chosen_asks(shown, asking, en);
            assert!(!asks);
            assert_eq!(
                ask,
                Ask::Dispatch {
                    command: "sidebar.show".to_owned(),
                    arguments: Value::object(Vec::<(&str, Value)>::new()),
                    clear: asking,
                    locale: en,
                }
            );
        }
    }

    /// The prompt's list chooses a command while it asks for the list, and
    /// completes the draft with what it offers while it asks for a command's
    /// parameters; a value it does not offer comes to nothing. Sent with no
    /// option, the command runs with what the draft gives it, the prompt
    /// cleared once it ran. Esc clears the prompt while it asks, and goes
    /// back in the drawer otherwise.
    #[test]
    pub(crate) fn the_prompts_list_chooses_completes_and_runs() {
        use crate::mirror::Asking;
        let en = nc_catalog::Locale::En;
        let commands = crate::commands::Commands::from_catalog();
        let parameters = Asking::Parameters {
            command: "page.open".to_owned(),
            parameter: None,
        };
        assert_eq!(
            super::picked(Some(&Asking::Commands), ";x", "page.help", &commands),
            Some(super::Picked::Command("page.help".to_owned()))
        );
        assert_eq!(
            super::picked(Some(&Asking::Commands), ";x", "no.such", &commands),
            None
        );
        assert_eq!(
            super::picked(Some(&parameters), "kind=he", "value:help", &commands),
            Some(super::Picked::Draft("kind=help ".to_owned()))
        );
        assert_eq!(
            super::picked(Some(&parameters), "", "parameter:kind", &commands),
            Some(super::Picked::Draft("kind=".to_owned()))
        );
        assert_eq!(
            super::picked(Some(&parameters), "kind=he", "value:dev", &commands),
            None,
            "a value not offered"
        );
        assert_eq!(super::picked(None, "", "page.help", &commands), None);
        assert_eq!(
            super::sent_asks(Some(&parameters), "kind=help", &commands, en),
            Some(Ask::Dispatch {
                command: "page.open".to_owned(),
                arguments: Value::object([("kind", Value::text("help"))]),
                clear: true,
                locale: en,
            })
        );
        assert_eq!(
            super::sent_asks(Some(&Asking::Commands), ";x", &commands, en),
            None
        );
        assert_eq!(super::sent_asks(None, "x", &commands, en), None);
        assert_eq!(super::escaped(true), "prompt.clear");
        assert_eq!(super::escaped(false), "drawer.back");
        for target in ["page.help", "parameter:kind", "value:help"] {
            assert!(super::is_target(target), "{target}");
        }
        for other in ["", "value:two words", &"a".repeat(65), "é"] {
            assert!(!super::is_target(other), "{other}");
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
