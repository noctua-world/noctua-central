//! `noctua mcp <agent>` writes nothing on standard output but the protocol's
//! answers, one a line, and says everything else on standard error
//! (`NOCTUA.md §20.3`).
//!
//! **The shipped binary, through its own doors**: the built `noctua` copied
//! into a scratch directory, beside a manifest relocated under a namespace of
//! this process's — so no name it derives is a real install's — or beside
//! none, and run with its standard input, output and error piped. No daemon
//! runs: the session opens only at `initialize`, so what is answered before
//! needs none, and `initialize` finds nothing listening. It spawns a process,
//! so no mutation command runs it.

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
    /// A private directory under the temporary one, named for this process
    /// and `tag`.
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nc-mcp-suite-{}-{tag}", std::process::id()));
        let _ = nc_platform::testing::make_private(&root);
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch directory");
        Self(root)
    }
}

/// The built CLI copied into `directory`, runnable.
fn copied_into(directory: &Path) -> PathBuf {
    let cli = directory.join(format!("noctua{}", nc_platform::EXECUTABLE_SUFFIX));
    nc_platform::create_private_executable(&cli)
        .and_then(|mut file| {
            file.write_all(&std::fs::read(env!("CARGO_BIN_EXE_noctua"))?)?;
            file.sync_all()
        })
        .expect("the CLI copied");
    cli
}

/// The built CLI copied into `scratch` beside a manifest of its own.
///
/// **A staging install relocated under a namespace** named for this process
/// and `tag`, its runtime directory made and nothing listening there.
fn installed_in(scratch: &Scratch, tag: u8) -> PathBuf {
    let base = |name: &str| {
        let base = scratch.0.join(name);
        nc_platform::create_private_directory(&base).expect("a base");
        base
    };
    let namespace = format!("m{:x}x{tag}", std::process::id() % 0xff_ffff);
    let install = nc_channel::Install {
        channel: nc_channel::Channel::Staging,
        bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
            .expect("plain absolute bases"),
        namespace: Some(nc_channel::Namespace::parse(&namespace).expect("a namespace")),
        log_endpoint: None,
    };
    nc_platform::create_private(&scratch.0.join(nc_channel::MANIFEST))
        .and_then(|mut file| file.write_all(install.to_canonical().as_bytes()))
        .expect("a manifest");
    let cli = copied_into(&scratch.0);
    let found = nc_channel::found::beside(&cli).expect("the install is read");
    for directory in [&found.layout.state, &found.layout.runtime] {
        nc_platform::create_private_directory(directory).expect("a directory");
    }
    cli
}

/// An agent's identifier, as a minter gives one.
fn agent() -> String {
    let flake = nc_ids::MachineFlake::from_value(9).expect("a flake");
    nc_ids::Minter::new(flake, nc_ids::SystemClock)
        .mint()
        .expect("minted")
        .to_crockford()
}

/// What `cli` wrote and ended with, run with `arguments` and handed the
/// bytes of `input`, its standard input then closed.
///
/// **Asked again**, for a bounded time, while a file just written is busy in
/// a sibling's child.
#[expect(
    clippy::disallowed_methods,
    reason = "the CLI under test is run with its standard input, output and error piped and read \
              back, never printed"
)]
fn ran(cli: &Path, arguments: &[&str], input: &[u8]) -> std::process::Output {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let started = Command::new(cli)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        match started {
            Ok(mut child) => {
                let mut handed = child.stdin.take().expect("its input");
                // A run that refuses its line ends without reading what it
                // was handed, and its input may be closed under the write.
                let _ = handed.write_all(input);
                drop(handed);
                return child.wait_with_output().expect("it ended");
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("`{}` did not start: {error}", cli.display()),
        }
    }
}

/// Every line of `written`, each read as one JSON value: holding no line
/// feed of its own, and the last ended by one.
fn frames(written: &[u8]) -> Vec<serde_json::Value> {
    let text = String::from_utf8(written.to_vec()).expect("UTF-8");
    assert!(text.is_empty() || text.ends_with('\n'), "{text}");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("one JSON value a line"))
        .collect()
}

/// The binary answers each request on one line, and ends when its input
/// ends.
///
/// **The real door and the real framing**: a ping written with a carriage
/// return before its line feed, and server/discover, are answered on two
/// lines — the ping's empty result, and the method this server does not have
/// — and closing its input ends it in 0 with nothing on standard error.
#[test]
fn the_binary_answers_each_request_on_one_line_and_ends_when_its_input_ends() {
    let scratch = Scratch::new("lines");
    let cli = installed_in(&scratch, 1);
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n\
                 {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"server/discover\"}\n";
    let output = ran(&cli, &["mcp", &agent()], input.as_bytes());
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let frames = frames(&output.stdout);
    assert_eq!(frames.len(), 2, "{frames:?}");
    assert_eq!(
        frames[0],
        serde_json::json!({"jsonrpc":"2.0","id":1,"result":{}})
    );
    assert_eq!(frames[1]["id"], 2);
    assert_eq!(frames[1]["error"]["code"], -32_601);
}

/// With no daemon, `initialize` is answered with the reason, and the binary
/// ends in nine.
///
/// **One `-32603` line** naming the endpoint's code, the same said on
/// standard error, and exit 9.
#[test]
fn with_no_daemon_initialize_is_answered_with_the_reason_and_the_binary_ends_in_nine() {
    let scratch = Scratch::new("down");
    let cli = installed_in(&scratch, 2);
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":\
                 {\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\
                 \"clientInfo\":{\"name\":\"a client\",\"version\":\"1\"}}}\n";
    let output = ran(&cli, &["mcp", &agent()], input.as_bytes());
    assert_eq!(output.status.code(), Some(9), "{output:?}");
    let code = nc_catalog::ErrorCode::IpcUnreachable.code();
    let frames = frames(&output.stdout);
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(frames[0]["id"], 1);
    assert_eq!(frames[0]["error"]["code"], -32_603);
    assert_eq!(frames[0]["error"]["data"]["code"], code);
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(said.starts_with(&format!("error[{code}]")), "{said}");
}

/// A line the CLI refuses is said on standard error alone.
///
/// **Beside a manifest**: `--json mcp`, a word that is no agent's identifier
/// and a second word each end in exit 2, the CLI's code on standard error and
/// nothing on standard output, whatever the options asked.
#[test]
fn a_line_refused_by_the_cli_is_said_on_standard_error_alone() {
    let scratch = Scratch::new("refused");
    let cli = installed_in(&scratch, 3);
    let agent = agent();
    let code = nc_catalog::ErrorCode::CliCommandLineUnreadable.code();
    for line in [
        &["--json", "mcp", &agent][..],
        &["--json", "mcp"],
        &["mcp", "extra"],
        &["mcp", &agent, "extra"],
    ] {
        let output = ran(&cli, line, b"");
        assert_eq!(output.status.code(), Some(2), "{line:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{line:?}: {output:?}");
        let said = String::from_utf8_lossy(&output.stderr);
        assert!(
            said.starts_with(&format!("error[{code}]")),
            "{line:?}: {said}"
        );
    }
}

/// With no install, nothing is written on standard output, whatever the
/// options ask.
///
/// **A copy with no manifest beside it**, and no pointer: `mcp` and
/// `--json mcp` each end in 12 with the manifest's code on standard error,
/// while `--json help` beside them is one document on standard output.
#[test]
fn with_no_install_nothing_is_written_on_standard_output_whatever_the_options_ask() {
    let scratch = Scratch::new("bare");
    let cli = copied_into(&scratch.0);
    let agent = agent();
    let code = nc_catalog::ErrorCode::ChannelNoManifest.code();
    for line in [
        &["mcp", &agent][..],
        &["--json", "mcp", &agent],
        &["--json", "mcp"],
    ] {
        let output = ran(&cli, line, b"");
        assert_eq!(output.status.code(), Some(12), "{line:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{line:?}: {output:?}");
        let said = String::from_utf8_lossy(&output.stderr);
        assert!(said.contains(code), "{line:?}: {said}");
    }
    let output = ran(&cli, &["--json", "help"], b"");
    assert_eq!(output.status.code(), Some(12), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).starts_with("{\"error\":"),
        "{output:?}"
    );
}
