//! `noctua-daemon` itself: the invocation it answers, the refusal outside an
//! install, and what `--initialize` leaves in a profile.
//!
//! **The binary this build made, run as a process.** The manifest is read
//! beside the canonicalized executable, so the one way to be read beside one
//! is to be an executable copied where an installer would put it, with a
//! manifest written beside it under a scratch root and a namespace of its own.

use std::path::{Path, PathBuf};

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// A scratch directory for one test, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-daemon-binary-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch directory");
        Self(root)
    }
}

/// The binary at `executable`, run with `arguments` and its output read.
#[expect(
    clippy::disallowed_methods,
    reason = "the daemon's binary run as a process with its output captured, never inherited: \
              the one way to hold what it answers to what it prints"
)]
fn run(executable: &Path, arguments: &[&std::ffi::OsStr]) -> (Option<i32>, String) {
    let output = std::process::Command::new(executable)
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the binary runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A world file as an installer writes one: the one every suite's install
/// copies in.
fn a_world() -> String {
    nc_daemon::testing::a_world()
}

/// An invocation the binary does not answer is refused, with exit code 2 and
/// the one it does answer named.
#[test]
fn an_invocation_the_binary_does_not_answer_is_refused() {
    let (code, said) = run(Path::new(BINARY), &["--serve".as_ref()]);
    assert_eq!(code, Some(2), "{said}");
    assert!(said.contains("--initialize"), "{said}");
}

/// The binary outside an install refuses with a channel's code, and names no
/// channel it was not given.
#[test]
fn outside_an_install_the_binary_refuses_with_a_channels_code() {
    let (code, said) = run(Path::new(BINARY), &["--initialize".as_ref()]);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("[NC-CHAN-"), "{said}");
}

/// `--initialize` beside a manifest makes the profile and its device and
/// copies the world in; run again, it keeps the seed it drew.
#[test]
fn initialize_beside_a_manifest_makes_the_device_once_and_copies_the_world() {
    let scratch = Scratch::new("initialize");
    for base in ["data", "config", "cache", "state"] {
        nc_platform::create_private_directory(&scratch.0.join(base)).expect("a base");
    }
    let install = nc_channel::Install {
        channel: nc_channel::Channel::Staging,
        bases: nc_channel::Bases::new(
            scratch.0.join("data"),
            scratch.0.join("config"),
            scratch.0.join("cache"),
            scratch.0.join("state"),
        )
        .expect("plain bases"),
        namespace: Some(nc_channel::Namespace::parse("t2").expect("a namespace")),
        log_endpoint: None,
    };
    let user = nc_platform::user_in(&scratch.0).expect("the scratch is private");
    let layout = nc_channel::layout(&install, &user.to_string());
    nc_platform::create_private_directory(&layout.state).expect("the state directory");
    nc_platform::create_private_directory(&layout.install).expect("the install directory");
    let copied = layout
        .install
        .join(Path::new(BINARY).file_name().expect("a binary's name"));
    std::fs::copy(BINARY, &copied).expect("the binary is copied where an installer puts it");
    {
        use std::io::Write as _;
        let mut manifest = nc_platform::create_private(&layout.manifest).expect("a manifest");
        manifest
            .write_all(install.to_canonical().as_bytes())
            .expect("written");
    }
    let world = scratch.0.join("world.json");
    std::fs::write(&world, a_world()).expect("a world to copy");

    let (code, said) = run(
        &copied,
        &[
            "--initialize".as_ref(),
            "--world".as_ref(),
            world.as_os_str(),
        ],
    );
    assert_eq!(code, Some(0), "{said}");
    let seed = std::fs::read(&layout.device_key).expect("the seed was drawn");
    assert_eq!(seed.len(), nc_daemon::device::SEED_BYTES);
    assert!(layout.device.is_file(), "no device record was written");
    assert_eq!(
        std::fs::read_to_string(&layout.world).expect("the world was copied"),
        a_world()
    );

    let (code, said) = run(&copied, &["--initialize".as_ref()]);
    assert_eq!(code, Some(0), "{said}");
    assert_eq!(
        std::fs::read(&layout.device_key).expect("the seed reads"),
        seed,
        "a second `--initialize` drew the seed again"
    );
}
