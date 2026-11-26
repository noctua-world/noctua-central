//! The shipped binary opens its window hidden and undecorated, keeps it
//! hidden until its document has loaded though it is asked to show at once,
//! paints the theme it was opened in from its first frame, never makes its
//! page smaller than the least, keeps its drawer below the footer inside the
//! window however much it holds, and closes the window (`NOCTUA.md §12.2`,
//! §12.10, §12.11, §12.19, §9.6).
//!
//! **The built `noctua-desktop`, asked `--probe`**, which needs no install
//! and no daemon: a window opened through `tao` in the light and then one in
//! the dark, each webview built through `nc_platform::webview` — `WebKitGTK`
//! on Linux, `WKWebView` on macOS, `WebView2` on Windows — loading the
//! interface's document through its custom protocol, and what the document
//! and the probe's observer say read from its message channel: the page's
//! background at its first frame and once settled, each painted into a pixel
//! and read back, the page's size, and what its footer and its drawer did
//! when exercised. **On a display the test is handed**
//! (`nc_platform::testing::Display`): on Linux an `Xvfb` of its own, so no
//! window reaches a person's session, and the session's own on macOS and
//! Windows. It spawns a process, so no mutation command runs it.

use std::process::Command;

/// The probe opens a window in each mode, each hidden and undecorated, kept
/// hidden until its document loaded, painting at its first frame what its
/// settled page paints — the dark one the design system's dark surface, the
/// two differing — and keeping its page the least size, and closes each,
/// answering exit code 0.
#[test]
fn the_window_opens_hidden_paints_its_theme_first_and_closes() {
    #[expect(
        clippy::disallowed_methods,
        reason = "the binary under test, its standard streams captured, so nothing it writes \
                  reaches the gate's output"
    )]
    let mut probe = Command::new(env!("CARGO_BIN_EXE_noctua-desktop"));
    let display = nc_platform::testing::Display::for_a_window().expect("a display");
    let ran = display
        .handed_to(&mut probe)
        .arg("--probe")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the binary ran");
    drop(display);
    let said = String::from_utf8_lossy(&ran.stderr);
    assert!(
        ran.status.success(),
        "the probe failed ({}): {said}",
        ran.status
    );
    assert!(
        said.contains(
            "noctua-desktop: a window opened in the light and one in the dark, each hidden and \
             undecorated and kept hidden until its document loaded, each first frame painting \
             what its settled page paints and the dark one the dark surface, each page the \
             least size when asked for less, each footer and drawer doing what they are for, \
             and each window was closed"
        ),
        "{said}"
    );
    assert!(
        ran.stdout.is_empty(),
        "the probe writes nothing on standard output"
    );
}
