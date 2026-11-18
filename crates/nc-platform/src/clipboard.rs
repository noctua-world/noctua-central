//! The system's clipboard, reached through the system's own tool: which tool,
//! chosen for a system and an environment by one pure function, and a text
//! handed to it on its standard input, never as an argument.
//!
//! | System | The tools, in the order they are tried |
//! |---|---|
//! | Linux, and every Unix but macOS | `wl-copy` when the environment names a Wayland display, then `xclip -selection clipboard`, then `xsel --clipboard --input` |
//! | macOS | `pbcopy`, told the text is UTF-8 through `LC_ALL`, since it reads the encoding from the locale |
//! | Windows | PowerShell's `Set-Clipboard`, handed the text as bytes it decodes as UTF-8 itself |
//!
//! **Windows' tool was measured, not assumed**, on the hosted Windows runner,
//! with a path that is not ASCII read back through `Get-Clipboard`: `clip.exe`
//! handed UTF-16 with its byte-order mark kept the mark as a character at the
//! head of the path, and handed UTF-8 it read the console's code page, which
//! was right there and is not on a machine set to another; `Set-Clipboard`,
//! handed bytes it decodes as UTF-8 itself, put the path whole.
//!
//! **Which tools a system has is [`tools`]', a pure function of the system and
//! the environment**, so the Linux job tests every system's choice. A tool that
//! is missing, or that fails, is passed over for the next; a text none of them
//! took is said to be uncopied, with why for each, and nothing else is undone.
//!
//! **The text is the tool's standard input**, so it is never a word another
//! program reads off the command line, and the tool's own output goes nowhere:
//! `xclip` and `wl-copy` leave a process behind that serves the clipboard, and
//! one holding a pipe of this process's would keep a reader of it waiting.
//!
//! # What this does not answer
//!
//! A tool is found on `PATH`, as the system's shell finds it, so a program of
//! the same name earlier on `PATH` is the one handed the text: the order of a
//! person's search path is theirs to keep.

use std::ffi::OsString;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A system, as its clipboard is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    /// Linux, and every Unix but macOS: Wayland's tool, or X's.
    Linux,
    /// Apple's macOS.
    MacOs,
    /// Windows.
    Windows,
}

/// The system this was built for.
pub const SYSTEM: System = if cfg!(target_os = "macos") {
    System::MacOs
} else if cfg!(windows) {
    System::Windows
} else {
    System::Linux
};

/// The variable that names a Wayland display, the one this reads.
pub const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";

/// What PowerShell runs to put its standard input on Windows' clipboard: the
/// bytes read whole and decoded as UTF-8 here, so no code page of the
/// console's reads them.
pub const SET_CLIPBOARD: &str = "$i = [Console]::OpenStandardInput(); $m = New-Object IO.MemoryStream; \
     $i.CopyTo($m); Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString($m.ToArray()))";

/// A program that puts its standard input on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    /// Its name, found on `PATH`.
    pub program: String,
    /// The arguments it takes, the text never among them.
    pub arguments: Vec<String>,
    /// What it is started with in its environment, beside what it inherits.
    pub environment: Vec<(String, String)>,
}

impl Tool {
    /// `program` with `arguments`, inheriting the environment as it is.
    fn of(program: &str, arguments: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            arguments: arguments.iter().map(|&word| word.to_owned()).collect(),
            environment: Vec::new(),
        }
    }
}

/// The tools `system` puts a text on its clipboard with, in the order they
/// are tried, in the environment `variable` reads.
///
/// **A Wayland display is named by [`WAYLAND_DISPLAY`] set and not empty**,
/// and only then is `wl-copy` tried first; X's two tools follow on every Unix
/// but macOS, since an X server is reached by its own variable, which the tool
/// reads.
#[must_use]
pub fn tools(system: System, variable: &dyn Fn(&str) -> Option<OsString>) -> Vec<Tool> {
    match system {
        System::Linux => {
            let wayland = variable(WAYLAND_DISPLAY).is_some_and(|display| !display.is_empty());
            let mut tools = Vec::new();
            if wayland {
                tools.push(Tool::of("wl-copy", &[]));
            }
            tools.push(Tool::of("xclip", &["-selection", "clipboard"]));
            tools.push(Tool::of("xsel", &["--clipboard", "--input"]));
            tools
        }
        System::MacOs => vec![Tool {
            environment: vec![("LC_ALL".to_owned(), "en_US.UTF-8".to_owned())],
            ..Tool::of("pbcopy", &[])
        }],
        System::Windows => vec![Tool::of(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", SET_CLIPBOARD],
        )],
    }
}

/// How long a tool may take to read what it is handed and end.
///
/// Far past what each takes, and a bound on a run that a tool which never
/// ends would otherwise hold.
pub const WITHIN: Duration = Duration::from_secs(10);

/// Why a text was not copied: each tool tried, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uncopied(Vec<(String, String)>);

impl Uncopied {
    /// Each tool tried, by its name, beside what became of it.
    #[must_use]
    pub fn tried(&self) -> &[(String, String)] {
        &self.0
    }
}

impl core::fmt::Display for Uncopied {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0.is_empty() {
            return write!(formatter, "this system names no tool to copy with");
        }
        for (at, (program, what)) in self.0.iter().enumerate() {
            if at > 0 {
                write!(formatter, "; ")?;
            }
            write!(formatter, "`{program}` {what}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Uncopied {}

/// `text` put on the clipboard by the first of `tools` that takes it: its
/// name, or why none did.
///
/// # Errors
///
/// [`Uncopied`], naming each tool and why it did not take the text: not on
/// `PATH`, failed to start, did not read it, ended badly, or ran past
/// [`WITHIN`] and was ended.
pub fn copy(text: &str, tools: &[Tool]) -> Result<String, Uncopied> {
    let mut tried = Vec::new();
    for tool in tools {
        match handed(text, tool, WITHIN) {
            Ok(()) => return Ok(tool.program.clone()),
            Err(what) => tried.push((tool.program.clone(), what)),
        }
    }
    Err(Uncopied(tried))
}

/// `text` handed to `tool` on its standard input, and the tool waited for,
/// for `within` at most: nothing, or what became of it.
#[expect(
    clippy::disallowed_methods,
    reason = "the system's clipboard tool, handed the text on its standard input, its own \
              standard output and error null, so it prints nothing"
)]
fn handed(text: &str, tool: &Tool, within: Duration) -> Result<(), String> {
    let mut child = Command::new(&tool.program)
        .args(&tool.arguments)
        .envs(
            tool.environment
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "is not on `PATH`".to_owned(),
            _ => format!("did not start: {error}"),
        })?;
    // **Closed once written**, so the tool reads to its end.
    let written = child
        .stdin
        .take()
        .map(|mut input| input.write_all(text.as_bytes()));
    let deadline = Instant::now() + within;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                drop(child.kill());
                drop(child.wait());
                return Err(format!(
                    "did not end within {} seconds, and was ended",
                    within.as_secs()
                ));
            }
            Err(error) => return Err(format!("could not be waited for: {error}")),
        }
    };
    match written {
        Some(Ok(())) => {}
        Some(Err(error)) => return Err(format!("did not read the text: {error}")),
        None => return Err("was started with no standard input".to_owned()),
    }
    if status.success() {
        Ok(())
    } else {
        Err(format!("ended {status}"))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{System, Tool};

    fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, OsString)> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect();
        move |name: &str| {
            pairs
                .iter()
                .find(|(variable, _)| variable == name)
                .map(|(_, value)| value.clone())
        }
    }

    fn programs(tools: &[Tool]) -> Vec<&str> {
        tools.iter().map(|tool| tool.program.as_str()).collect()
    }

    /// Each system's tools are chosen by the system and the environment alone.
    ///
    /// On Linux `wl-copy` first only where a Wayland display is named, then X's
    /// two tools; `pbcopy` on macOS, told the text is UTF-8; and on Windows
    /// `Set-Clipboard`, decoding the bytes as UTF-8 itself. No tool is handed
    /// the text as an argument.
    #[test]
    fn each_systems_tools_are_chosen_by_the_system_and_the_environment() {
        let wayland = super::tools(
            System::Linux,
            &environment(&[("WAYLAND_DISPLAY", "wayland-0")]),
        );
        assert_eq!(programs(&wayland), ["wl-copy", "xclip", "xsel"]);
        assert_eq!(wayland[1].arguments, ["-selection", "clipboard"]);
        assert_eq!(wayland[2].arguments, ["--clipboard", "--input"]);
        for unnamed in [environment(&[]), environment(&[("WAYLAND_DISPLAY", "")])] {
            assert_eq!(
                programs(&super::tools(System::Linux, &unnamed)),
                ["xclip", "xsel"],
                "wl-copy was tried with no Wayland display named"
            );
        }
        let mac = super::tools(System::MacOs, &environment(&[("WAYLAND_DISPLAY", "w")]));
        assert_eq!(programs(&mac), ["pbcopy"]);
        assert_eq!(
            mac[0].environment,
            [("LC_ALL".to_owned(), "en_US.UTF-8".to_owned())]
        );
        let windows = super::tools(System::Windows, &environment(&[]));
        assert_eq!(programs(&windows), ["powershell.exe"]);
        let script = windows[0].arguments.last().expect("a script");
        assert!(
            script.contains("OpenStandardInput()")
                && script.contains("Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString("),
            "Windows' tool does not decode its input as UTF-8 itself: {script}"
        );
        assert_eq!(
            windows[0].arguments[..3],
            ["-NoProfile", "-NonInteractive", "-Command"]
        );
        for system in [System::Linux, System::MacOs, System::Windows] {
            for tool in super::tools(system, &environment(&[("WAYLAND_DISPLAY", "w")])) {
                assert!(
                    tool.arguments
                        .iter()
                        .all(|word| !word.contains("noctua-dev")),
                    "{tool:?}"
                );
            }
        }
    }

    /// The text is the tool's standard input and never an argument, and a
    /// tool that is missing or fails is passed over for the next.
    ///
    /// Python stands in for a tool: it writes what it reads, and the
    /// arguments it was given, to a file of the test's.
    #[test]
    fn the_text_is_the_tools_input_and_a_tool_that_fails_is_passed_over() {
        let scratch = Scratch::new("copied");
        let kept = scratch.0.join("kept");
        let python = |script: &str| Tool {
            program: crate::testing::PYTHON.to_owned(),
            arguments: vec!["-c".to_owned(), script.to_owned()],
            environment: Vec::new(),
        };
        let writes = format!(
            "import sys; open({:?}, 'wb').write(sys.stdin.buffer.read() + repr(sys.argv[1:]).encode())",
            kept.to_string_lossy()
        );
        let text = "/home/zoë/naïve — 日本/noctua-dev";
        let tools = [
            Tool::of("nc-no-such-clipboard-tool", &[]),
            python("import sys; sys.stdin.buffer.read(); sys.exit(3)"),
            python(&writes),
        ];
        let copied = super::copy(text, &tools).expect("the third took it");
        assert_eq!(copied, crate::testing::PYTHON);
        let read = std::fs::read(&kept).expect("the tool wrote what it read");
        assert_eq!(
            read,
            format!("{text}[]").as_bytes(),
            "the text was not the input alone"
        );
        let refused = super::copy(text, &tools[..2]).expect_err("neither took it");
        assert_eq!(refused.tried().len(), 2);
        let said = refused.to_string();
        assert!(said.contains("is not on `PATH`"), "{said}");
        assert!(said.contains("ended"), "{said}");
        assert_eq!(
            super::copy(text, &[]).expect_err("no tool").to_string(),
            "this system names no tool to copy with"
        );
    }

    /// A scratch directory, removed however the test leaves.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A private directory of its own.
        fn new(tag: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-clipboard-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            crate::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }
    }
}
