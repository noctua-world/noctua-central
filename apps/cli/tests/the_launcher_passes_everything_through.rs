//! A launcher passes every argument, standard input, output and error, and the
//! exit code through to the CLI its pointer names (`NOCTUA.md §17.1`).
//!
//! **On every system**, though only Windows installs a launcher: the built
//! `noctua` is copied into a scratch bin directory under `noctua-dev`'s name,
//! with no install beside it and a private pointer naming Python, which writes
//! back what it was handed and ends with a code of its own. A copy with no
//! pointer is a CLI with no install.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A scratch directory for this suite, removed however a test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = nc_platform::testing::make_private(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-launcher-suite-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch directory");
        Self(root)
    }
}

/// The built CLI copied into `bin` under `name`, runnable, with no install
/// beside it.
fn launcher_in(bin: &Path, name: &str) -> PathBuf {
    let launcher = bin.join(format!("{name}{}", nc_platform::EXECUTABLE_SUFFIX));
    nc_platform::create_private_executable(&launcher)
        .and_then(|mut file| {
            file.write_all(&std::fs::read(env!("CARGO_BIN_EXE_noctua"))?)?;
            file.sync_all()
        })
        .expect("the CLI copied");
    launcher
}

/// Python's own executable, as it names itself: an absolute path.
#[expect(
    clippy::disallowed_methods,
    reason = "Python asked where it is, its output read back and never printed"
)]
fn python() -> PathBuf {
    let said = Command::new(nc_platform::testing::PYTHON)
        .args(["-c", "import sys; sys.stdout.write(sys.executable)"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .expect("Python runs");
    PathBuf::from(String::from_utf8(said.stdout).expect("a path"))
}

/// What `launcher` wrote and ended with, run with `arguments` and handed the
/// bytes `input`.
///
/// **Asked again**, for a bounded time, while a file just written is busy in
/// a sibling's child.
#[expect(
    clippy::disallowed_methods,
    reason = "the launcher under test is run with its standard input, output and error piped \
              and read back, never printed"
)]
fn ran(launcher: &Path, arguments: &[&str], input: &[u8]) -> std::process::Output {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let started = Command::new(launcher)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        match started {
            Ok(mut child) => {
                child
                    .stdin
                    .take()
                    .expect("its input")
                    .write_all(input)
                    .expect("handed over");
                return child.wait_with_output().expect("it ended");
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("`{}` did not start: {error}", launcher.display()),
        }
    }
}

/// Every argument, standard input, output and error, and the exit code are
/// the named CLI's.
///
/// **Every argument**: one with a space, one not ASCII, one empty, and one
/// the CLI would read as its own option.
#[test]
fn a_launcher_passes_every_argument_its_streams_and_the_exit_code_through() {
    let scratch = Scratch::new("through");
    let launcher = launcher_in(&scratch.0, "noctua-dev-tw1");
    let pointer = nc_channel::checkouts::pointer_beside(&launcher);
    nc_platform::create_private(&pointer)
        .and_then(|mut file| {
            file.write_all(
                nc_channel::checkouts::Pointer { cli: python() }
                    .to_canonical()
                    .as_bytes(),
            )
        })
        .expect("a pointer");
    let script = "import sys; read = sys.stdin.buffer.read(); \
                  sys.stdout.write(ascii(sys.argv[1:]) + '|' + ascii(read)); \
                  sys.stderr.write('said on its error'); sys.exit(7)";
    let output = ran(
        &launcher,
        &["-c", script, "a b", "ç", "", "--json"],
        b"handed in",
    );
    assert_eq!(output.status.code(), Some(7), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "['a b', '\\xe7', '', '--json']|b'handed in'"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "said on its error");
}

/// A copy of the CLI with no pointer beside it is a CLI with no install: it
/// runs nothing and ends in that code.
#[test]
fn a_copy_with_no_pointer_is_a_cli_with_no_install() {
    let scratch = Scratch::new("bare");
    let launcher = launcher_in(&scratch.0, "noctua-dev-tw2");
    let output = ran(&launcher, &["help"], b"");
    assert_eq!(output.status.code(), Some(12), "{output:?}");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.contains(nc_catalog::ErrorCode::ChannelNoManifest.code()),
        "{said}"
    );
    // PROBE, taken out once read: which tool puts a path that is not ASCII
    // on Windows' clipboard whole, read back through PowerShell in UTF-8.
    if nc_platform::clipboard::SYSTEM == nc_platform::clipboard::System::Windows {
        let path = "C:\\Users\\Zoë\\naïve — 日本\\noctua-dev.exe";
        let read = || {
            let back = ran(
                Path::new("powershell.exe"),
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-Clipboard -Raw",
                ],
                b"",
            );
            (
                back.status.code(),
                String::from_utf8_lossy(&back.stdout).into_owned(),
                String::from_utf8_lossy(&back.stderr).into_owned(),
            )
        };
        let clip = ran(
            Path::new("clip.exe"),
            &[],
            &nc_platform::clipboard::encoded(path, nc_platform::clipboard::Input::Utf16WithMark),
        );
        let after_clip = read();
        let shell = ran(
            Path::new("powershell.exe"),
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$i = [Console]::OpenStandardInput(); $m = New-Object IO.MemoryStream; $i.CopyTo($m); Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString($m.ToArray()))",
            ],
            path.as_bytes(),
        );
        let after_shell = read();
        let plain = ran(Path::new("clip.exe"), &[], path.as_bytes());
        let after_plain = read();
        panic!(
            "PROBE wanted {path:?}\nclip.exe UTF-16 with mark: {:?} -> {after_clip:?}\nSet-Clipboard from UTF-8: {:?} {:?} -> {after_shell:?}\nclip.exe UTF-8: {:?} -> {after_plain:?}",
            clip.status.code(),
            shell.status.code(),
            String::from_utf8_lossy(&shell.stderr),
            plain.status.code()
        );
    }
}
