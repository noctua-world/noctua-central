//! The one call that puts a webview into a window, and the one address a
//! custom protocol's pages are at, which each system makes its own way
//! (`NOCTUA.md §2.7`, §4): behind the `webview` feature, which only the
//! desktop interface turns on.
//!
//! **Linux builds it through GTK**, into the box `tao` keeps in every window,
//! because `wry`'s other path takes the window's raw handle, which is an X
//! window's alone — a Wayland session would have no webview at all. **macOS
//! and Windows build it on the window's own handle**, which is the only path
//! `wry` has there. **A custom protocol's page is at `<scheme>://localhost/`
//! on Linux and macOS, and at `http://<scheme>.localhost/` on Windows**
//! (`origin`), where `WebView2` serves a custom scheme under a host of its
//! own. Every other decision about the window and its webview is the
//! desktop's, made the same on every system.
//!
//! # What this answers (`§2.4`)
//!
//! A window whose GTK box is gone — a webview built nowhere — refused by name
//! rather than built into a window it is not in. What the webview loads, and
//! what it may ask of the process, are the desktop's to decide.
//!
//! **No test is written here**, since a test in a module the feature hides is
//! a test hidden from every run that leaves the feature off: the desktop's own
//! suite builds a window and its webview through this, on each system
//! (`apps/desktop/tests/the_window_opens_hidden.rs`).

#![cfg(feature = "webview")]

use core::fmt;

/// Why a webview was not put into a window.
#[derive(Debug)]
pub enum Unbuilt {
    /// The window holds no box a webview could be put in: on Linux, `tao`
    /// keeps one in every window it builds, so this is a window not built by
    /// it.
    NoBox,
    /// The system's webview refused, saying why.
    Refused(wry::Error),
}

impl fmt::Display for Unbuilt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBox => formatter.write_str(
                "the window holds no box a webview could be put in, so no webview was built",
            ),
            Self::Refused(error) => {
                write!(formatter, "the system's webview was not built: {error}")
            }
        }
    }
}

impl core::error::Error for Unbuilt {}

/// The webview `builder` describes, built into `window` the way this system
/// builds one: into the window's GTK box on Linux, on its handle elsewhere.
///
/// # Errors
///
/// [`Unbuilt::NoBox`] for a window with no box to put it in, and
/// [`Unbuilt::Refused`] with what the system's webview said.
pub fn into<'a>(
    builder: wry::WebViewBuilder<'a>,
    window: &'a tao::window::Window,
) -> Result<wry::WebView, Unbuilt> {
    #[cfg(target_os = "linux")]
    {
        use tao::platform::unix::WindowExtUnix as _;
        use wry::WebViewBuilderExtUnix as _;
        let container = window.default_vbox().ok_or(Unbuilt::NoBox)?;
        builder.build_gtk(container).map_err(Unbuilt::Refused)
    }
    #[cfg(not(target_os = "linux"))]
    {
        builder.build(window).map_err(Unbuilt::Refused)
    }
}

/// The address of the root of the pages a custom protocol named `scheme`
/// serves, as this system's webview loads them, ending in `/`:
/// `<scheme>://localhost/` on Linux and macOS, and `http://<scheme>.localhost/`
/// on Windows.
#[must_use]
pub fn origin(scheme: &str) -> String {
    if cfg!(windows) {
        format!("http://{scheme}.localhost/")
    } else {
        format!("{scheme}://localhost/")
    }
}
