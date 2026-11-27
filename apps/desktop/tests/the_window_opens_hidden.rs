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
//! Windows. **It is handed a home and a temporary directory of its own**,
//! under a scratch root the suite removes, so it holds that the probe writes
//! nothing in a person's own directories — its webviews keep nothing of their
//! own — and removes the folder it made for what a system's webview keeps
//! even so. It spawns a process, so no mutation command runs it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A scratch root of the suite's own, removed however it leaves.
struct Scratch(PathBuf);

impl Scratch {
    /// A private directory under the temporary one, named for this process.
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nc-prb-{}", std::process::id()));
        let _ = nc_platform::testing::make_private(&root);
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch root");
        Self(root)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = nc_platform::testing::make_private(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The variables the probe is handed a home by, each a directory of it.
const HOME: [&str; 5] = [
    "HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_STATE_HOME",
];

/// The variables the system's temporary directory is read from.
const TEMPORARY: [&str; 3] = ["TMPDIR", "TMP", "TEMP"];

/// Every path under `root`, but those in `made`.
fn written(root: &Path, made: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut left = vec![root.to_path_buf()];
    while let Some(directory) = left.pop() {
        for entry in std::fs::read_dir(&directory)
            .expect("a scratch directory is read")
            .map_while(Result::ok)
        {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                left.push(path.clone());
            }
            if !made.contains(&path) {
                found.push(path);
            }
        }
    }
    found
}

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
    let scratch = Scratch::new();
    let home = scratch.0.join("home");
    let temporary = scratch.0.join("tmp");
    let mut made = vec![home.clone(), temporary.clone()];
    nc_platform::create_private_directory(&home).expect("a home");
    nc_platform::create_private_directory(&temporary).expect("a temporary directory");
    for variable in HOME {
        let directory = if variable == "HOME" {
            home.clone()
        } else {
            let directory = home.join(variable.to_ascii_lowercase());
            nc_platform::create_private_directory(&directory).expect("a base directory");
            made.push(directory.clone());
            directory
        };
        probe.env(variable, directory);
    }
    for variable in TEMPORARY {
        probe.env(variable, &temporary);
    }
    probe.env("MESA_SHADER_CACHE_DISABLE", "true");
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
    assert_eq!(
        written(&home, &made),
        Vec::<PathBuf>::new(),
        "the probe wrote in the home it was handed"
    );
    let kept: Vec<PathBuf> = written(&temporary, &made)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("nc-desktop-probe-"))
        })
        .collect();
    assert_eq!(
        kept,
        Vec::<PathBuf>::new(),
        "the probe left the folder its webviews kept their own in"
    );
}
