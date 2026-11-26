//! The shipped binary opens its window hidden and undecorated, its webview
//! loads the interface's document with the window still hidden, and the
//! window closes (`NOCTUA.md §12.2`, §12.10).
//!
//! **The built `noctua-desktop`, asked `--probe`**, which needs no install
//! and no daemon: a window opened through `tao`, its webview built through
//! `nc_platform::webview` — `WebKitGTK` on Linux, `WKWebView` on macOS,
//! `WebView2` on Windows — and the document's one word read from its message
//! channel. **On a display the test is handed** (`nc_platform::testing::Display`):
//! on Linux an `Xvfb` of its own, so no window reaches a person's session,
//! and the session's own on macOS and Windows. It spawns a process, so no
//! mutation command runs it.

use std::process::Command;

/// The probe opens a window hidden and undecorated, loads its document with
/// the window still hidden, and closes it, answering exit code 0.
#[test]
fn the_window_opens_hidden_loads_its_document_and_closes() {
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
            "noctua-desktop: a window opened hidden and undecorated, its document loaded \
             with the window still hidden, and the window was closed"
        ),
        "{said}"
    );
    assert!(
        ran.stdout.is_empty(),
        "the probe writes nothing on standard output"
    );
}
