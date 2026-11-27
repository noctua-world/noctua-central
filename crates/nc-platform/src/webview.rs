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
//! own. **A webview keeps nothing of its own** (`Kept`): its store is
//! ephemeral on every system, and Windows alone, whose `WebView2` keeps a
//! user data folder even then, is handed the folder to keep it in — on Linux
//! no store with a folder is built at all, since `WebKitGTK` given one still
//! makes its cache and its HSTS store under the user's own directories, by the
//! executable's name. Every other decision about the window and its webview
//! is the desktop's, made the same on every system.
//!
//! # What this answers (`§2.4`)
//!
//! A window whose GTK box is gone — a webview built nowhere — refused by name
//! rather than built into a window it is not in. What the webview loads, and
//! what it may ask of the process, are the desktop's to decide.
//!
//! **No test is written here**, since a test in a module the feature hides is
//! a test hidden from every run that leaves the feature off: the desktop's own
//! suites build a window and its webview through this, on each system, in a
//! process of their own — `apps/desktop/tests/the_window_opens_hidden.rs`
//! loads the document from `origin` through `into` and `Kept`, so an address
//! another system's spelling answers nothing and fails the probe, and both it
//! and `apps/desktop/tests/the_window_follows_its_daemon.rs` hand the window a
//! home of their own and hold that nothing was written in it.

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

/// What a webview keeps of its own: built to keep nothing — its store
/// ephemeral on every system, `WebKitGTK`'s ephemeral context, `WKWebView`'s
/// store kept in memory, `WebView2` in private — and, where the system's
/// webview keeps a folder even so, Windows' `WebView2`, the folder it was
/// handed.
#[derive(Debug)]
pub struct Kept(Option<wry::WebContext>);

impl Kept {
    /// What a webview keeps, `folder` the one it keeps a folder in where its
    /// system's webview keeps one at all: on Windows alone. Elsewhere no
    /// store with a folder is built, since `WebKitGTK` given one still writes
    /// under the user's own directories.
    #[must_use]
    pub fn in_folder(folder: &std::path::Path) -> Self {
        Self(cfg!(windows).then(|| wry::WebContext::new(Some(folder.to_path_buf()))))
    }

    /// A builder of a webview that keeps nothing of its own: in private, and
    /// handed the folder where one is kept.
    pub fn builder(&mut self) -> wry::WebViewBuilder<'_> {
        match self.0.as_mut() {
            Some(context) => wry::WebViewBuilder::new_with_web_context(context),
            None => wry::WebViewBuilder::new(),
        }
        .with_incognito(true)
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
