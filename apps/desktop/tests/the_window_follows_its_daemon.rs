//! The shipped window over a real daemon: it opens hidden and undecorated
//! with the footer alone, shows, hides, resizes and moves as the daemon's
//! state says, draws a second seat as the design system draws the first,
//! holds every page inside the smallest window with every control a touch
//! can reach, and, killed, comes back drawing exactly what it drew, at the
//! size and the place it had, from the state the daemon kept (`NOCTUA.md
//! §12.1`, §12.2, §12.18, §12.19; acceptance criteria 2, 4 and 10).
//!
//! **A real daemon and the real binary.** The daemon is a child of this
//! suite's own binary told to serve a scratch install (`nc_daemon::testing`),
//! so no stale `noctua-daemon` is what runs; the built `noctua-desktop` is
//! linked beside that install's manifest, as an installer puts it, and
//! started on a display the test is handed (`nc_platform::testing::Display`:
//! on Linux an `Xvfb` of its own, the session's own on macOS and Windows).
//! The suite changes the interface's state only as the CLI does, by commands
//! through the one client the window itself asks with
//! (`nc_desktop::client::Asker`).
//!
//! **What the window shows is read from what it says**: the install is a
//! test's — relocated under a namespace — so the window prints, each time the
//! state, the page's scale, the registry or its place moved, one line of what
//! the system says of the window — shown, decorated, where its corner is —
//! and of what its page says it shows: its size in CSS pixels, whether
//! everything drawn stands inside it either way, whether every line of the
//! footer stands whole inside it, whether every control is at
//! least `--noctua-target-min` each way and every box that scrolls what it
//! holds a stop of the keyboard or holding one, whether each seat's attribute
//! changes how its element is drawn, where the system's own keyboard is,
//! which parts draw anything, every seat's state by the element it is on, and
//! a digest of the markup the page holds (`nc_desktop::host::witnessed_line`).
//! A real install never prints it.
//!
//! **The window is handed a home of its own** (`Home`): `HOME` and every
//! XDG base directory under the install's scratch root, so the suite holds
//! that the window writes nothing in a person's own directories — its
//! webview keeps nothing of its own, and the folder a system's webview keeps
//! even so is the install's (`nc_platform::webview::Kept`) — and never
//! touches the person's real ones. The GL driver's shader cache, which is the
//! driver's and no webview's, is turned off for it.
//!
//! **Nothing here is not applicable on any system**: each step is the
//! window's own, through `tao` and the system's webview, on the display a
//! runner has. Moving and resizing by the system's own gestures, and a touch
//! screen's own tap, are not driven — no runner has the hand that makes them
//! — and are on the list kept by hand at the Stage's close.
//!
//! It spawns a process, so no mutation command runs it.

use std::collections::BTreeMap;
use std::io::BufRead as _;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;

use nc_canon::Value;
use nc_daemon::testing::{Installed, PATIENCE};

/// What tells a child of this binary to serve the install it names.
const SERVES: &str = "NC_DESKTOP_WINDOW_SERVES";

/// The one test, by the name the harness filters it with.
const NAME: &str = "the_window_shows_hides_and_comes_back_from_its_daemon_with_a_second_seat";

/// One line the window printed of what it shows, by its words.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Witnessed(BTreeMap<String, String>);

impl Witnessed {
    /// The line `line` is, where it is one the window prints of itself.
    fn read(line: &str) -> Option<Self> {
        let rest = line.strip_prefix("noctua-desktop: witnessed ")?;
        Some(Self(
            rest.split(' ')
                .filter_map(|word| word.split_once('='))
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
        ))
    }

    /// The value of `name`, or nothing where the line holds none.
    fn get(&self, name: &str) -> &str {
        self.0.get(name).map_or("", String::as_str)
    }

    /// Every seat's state on the page.
    fn seats(&self) -> Vec<&str> {
        match self.get("seats") {
            "-" | "" => Vec::new(),
            seats => seats.split(',').collect(),
        }
    }
}

/// The home a window is handed, under the install's scratch root: `HOME` and
/// each XDG base directory, made empty before the window starts.
struct Home {
    /// Where it is.
    root: std::path::PathBuf,
}

impl Home {
    /// The variables a window is handed, each a directory of this home.
    const VARIABLES: [&'static str; 5] = [
        "HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
    ];

    /// A home under `scratch`, each of its directories made.
    fn under(scratch: &Path) -> Self {
        let root = scratch.join("home");
        nc_platform::create_private_directory(&root).expect("a home");
        let home = Self { root };
        for variable in Self::VARIABLES.into_iter().skip(1) {
            nc_platform::create_private_directory(&home.directory(variable))
                .expect("a base directory");
        }
        home
    }

    /// The directory `variable` names.
    fn directory(&self, variable: &str) -> std::path::PathBuf {
        if variable == "HOME" {
            self.root.clone()
        } else {
            self.root.join(variable.to_ascii_lowercase())
        }
    }

    /// `command`, handed this home, and the GL driver's shader cache off.
    fn handed_to<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        for variable in Self::VARIABLES {
            command.env(variable, self.directory(variable));
        }
        command.env("MESA_SHADER_CACHE_DISABLE", "true")
    }

    /// Everything written in this home since it was made: every path in it
    /// but its base directories, empty.
    fn written(&self) -> Vec<std::path::PathBuf> {
        let made: Vec<std::path::PathBuf> = Self::VARIABLES
            .into_iter()
            .map(|variable| self.directory(variable))
            .collect();
        let mut found = Vec::new();
        let mut left = vec![self.root.clone()];
        while let Some(directory) = left.pop() {
            for entry in std::fs::read_dir(&directory)
                .expect("the home is read")
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
}

/// The desktop's binary running for the install, what it prints read line
/// by line on a thread of its own; killed and waited for however the suite
/// leaves.
struct Desktop {
    /// The binary, running.
    child: Child,
    /// Each line it prints on its standard error, as it prints it.
    lines: mpsc::Receiver<String>,
    /// Every line it has printed so far, for a failure to show.
    heard: Vec<String>,
}

impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Desktop {
    /// `binary` started on `display`, handed `home`, its standard error
    /// piped.
    fn start(binary: &Path, display: &nc_platform::testing::Display, home: &Home) -> Self {
        nc_platform::process::keep_standard_handles_from_children()
            .expect("this process's standard handles are kept from its children");
        #[expect(
            clippy::disallowed_methods,
            reason = "the binary under test, its standard error read by this suite, so nothing \
                      it writes reaches the gate's output"
        )]
        let mut command = Command::new(binary);
        home.handed_to(display.handed_to(&mut command))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("the desktop starts");
        let error = child.stderr.take().expect("its standard error");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(error).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            heard: Vec::new(),
        }
    }

    /// The first line it prints from now on of which `wanted` holds, within
    /// [`PATIENCE`]; the suite fails naming `what`, with everything it
    /// printed, otherwise.
    fn until(&mut self, what: &str, wanted: impl Fn(&Witnessed) -> bool) -> Witnessed {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let seen = Witnessed::read(&line);
                    self.heard.push(line);
                    if let Some(seen) = seen
                        && wanted(&seen)
                    {
                        return seen;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                    panic!(
                        "the window never showed {what} within {PATIENCE:?}; it printed:\n{}",
                        self.heard.join("\n")
                    )
                }
            }
        }
    }
}

/// An empty object: a command's arguments where it takes none.
fn none() -> Value {
    Value::object(Vec::<(&str, Value)>::new())
}

/// Where the daemon's state, as `shell.read` answers it in `read`, puts the
/// system's own seat's keyboard: the part's word, or `-` for nowhere.
fn keyboard_of(read: &Value) -> String {
    /// The field `name` of `value`, where it is an object.
    fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
        match value {
            Value::Object(fields) => fields.get(name),
            _ => None,
        }
    }
    let seats = field(read, "shell").and_then(|shell| field(shell, "seats"));
    let Some(Value::Array(seats)) = seats else {
        return "-".to_owned();
    };
    seats
        .iter()
        .find(|seat| matches!(field(seat, "seat"), Some(Value::Integer(1))))
        .and_then(|seat| field(seat, "focus"))
        .and_then(|focus| match field(focus, "region") {
            Some(Value::Text(region)) => Some(region.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "-".to_owned())
}

/// Whether the page `seen` holds has been laid out in a window shown: a
/// webview a hidden window holds may have no size at all, which `WebKitGTK`
/// gives it until the window is mapped, and says its size once it has one.
fn laid_out(seen: &Witnessed) -> bool {
    seen.get("shown") == "1"
        && !seen.get("page").starts_with("0x")
        && !seen.get("page").ends_with("x0")
}

/// The window opens hidden and undecorated with nothing but the footer,
/// shows when the Leader is tapped with the keyboard on its prompt, opens
/// the leader menu in its drawer at the next tap, gives the keyboard back to
/// the prompt line when Esc's command closes the menu and keeps it in the
/// drawer where the drawer still shows a page, answers a part it cannot take
/// the keyboard on with where its keyboard is, the state following the page
/// each time, draws a second seat's
/// hover and keyboard where the state puts them and as the design system
/// draws a state, keeps everything inside the smallest window with every
/// control at least the least target, moves where the state puts it, puts
/// the keyboard on the sidebar; killed, it leaves the daemon's state as it
/// was, and started again it draws exactly what it drew, at its size and its
/// place; and it hides when the state says so.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one window's life, in order: each step stands on the state the one before left, \
              and splitting it would start a daemon and a webview again for each"
)]
fn the_window_shows_hides_and_comes_back_from_its_daemon_with_a_second_seat() {
    if let Some(executable) = std::env::var_os(SERVES) {
        nc_daemon::run::serve(Path::new(&executable)).expect("the daemon ran");
        return;
    }
    let this = std::env::current_exe().expect("this test binary");
    let installed = Installed::new(&this);
    let built = Path::new(env!("CARGO_BIN_EXE_noctua-desktop"));
    let desktop = installed
        .layout
        .install
        .join(built.file_name().expect("a binary's name"));
    // Linked where the system allows it, and copied where it does not, as
    // the daemon's own binary is.
    if std::fs::hard_link(built, &desktop).is_err() {
        std::fs::copy(built, &desktop).expect("the desktop is copied beside the manifest");
    }
    let mut daemon = installed.spawn_as(
        &this,
        &["--exact", NAME, "--nocapture", "--test-threads=1"],
        &[(SERVES, installed.executable.as_os_str())],
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    drop(runtime.block_on(daemon.connected()));
    let found = nc_channel::found::beside(&installed.executable).expect("the install");
    let mut asker = nc_desktop::client::Asker::new(found);
    let mut run = |command: &str, args: Value| -> Value {
        runtime
            .block_on(asker.dispatch(command, args))
            .expect("the daemon answered")
            .unwrap_or_else(|refusal| panic!("{command} was refused: {refusal:?}"))
    };
    let display = nc_platform::testing::Display::for_a_window().expect("a display");
    let home = Home::under(installed.root());
    let mut window = Desktop::start(&desktop, &display, &home);

    // **The first run**: hidden and undecorated — what its page draws is
    // read once it is shown, since a webview a hidden window holds may not be
    // laid out at all.
    let first = window.until("its first run, hidden", |seen| seen.get("shown") == "0");
    assert_eq!(first.get("decorated"), "0", "{first:?}");

    // **Shown by the Leader**, the keyboard on the prompt, the footer alone
    // and nothing outside the window.
    run("leader.tap", none());
    let shown = window.until("itself shown with the keyboard on its prompt", |seen| {
        seen.get("shown") == "1" && seen.get("keyboard") == "prompt" && laid_out(seen)
    });
    assert_eq!(
        (
            shown.get("decorated"),
            shown.get("shows"),
            shown.get("fits"),
            shown.get("lines"),
            shown.get("reachable"),
            shown.seats(),
        ),
        ("0", "footer", "1", "1", "1", Vec::<&str>::new()),
        "{shown:?}"
    );

    // **A page in the workspace area**, which the sidebar lists: the
    // workspace area draws no page of its own yet, so the footer is still all
    // the window draws.
    run(
        "page.open",
        Value::object([
            ("kind", Value::text("home")),
            ("in", Value::text("workspace")),
        ]),
    );
    let opened = window.until("a page opened in its workspace area", |seen| {
        laid_out(seen) && seen.get("drawn") != shown.get("drawn")
    });
    assert_eq!(
        (opened.get("shows"), opened.get("keyboard")),
        ("footer", "prompt"),
        "{opened:?}"
    );

    // **The keyboard given up to the workspace area**, where a click on
    // nothing in it gives it: the window gives its keyboard up to nothing,
    // and a tap then puts it back on the prompt line rather than open the
    // leader menu (§12.8, the first rule).
    run(
        "shell.focus",
        Value::object([("region", Value::text("workspace"))]),
    );
    window.until("its keyboard on nothing", |seen| {
        seen.get("keyboard") == "-" && seen.get("shows") == "footer"
    });
    run("leader.tap", none());
    window.until(
        "its keyboard back on the prompt, and no leader menu",
        |seen| seen.get("keyboard") == "prompt" && seen.get("shows") == "footer",
    );

    // **The leader menu**, in the drawer, the keyboard on it.
    run("leader.tap", none());
    window.until("the leader menu in its drawer", |seen| {
        seen.get("shows") == "footer,drawer" && seen.get("keyboard") == "drawer"
    });

    // **Closed by Esc's command**, the drawer's one page: the drawer then
    // shows nothing, so the keyboard goes back to the prompt line, in the
    // state as on the page — never left on the list the draw took away.
    run("drawer.back", none());
    let closed = window.until("the leader menu closed", |seen| {
        seen.get("shows") == "footer"
    });
    assert_eq!(closed.get("keyboard"), "prompt", "{closed:?}");
    assert_eq!(keyboard_of(&run("shell.read", none())), "prompt");

    // **A part the page cannot take the keyboard on**, named by the state:
    // the sidebar, hidden. The page answers with where its keyboard is, and
    // the state follows it back to the prompt line.
    run(
        "shell.focus",
        Value::object([("region", Value::text("sidebar"))]),
    );
    let deadline = Instant::now() + PATIENCE;
    while keyboard_of(&run("shell.read", none())) != "prompt" {
        assert!(
            Instant::now() < deadline,
            "the state kept the keyboard on a hidden sidebar"
        );
        window.until("a line once the state moved", |_| true);
    }

    // **The leader menu beside a page in the drawer**, closed by Esc's
    // command: the drawer still shows the page, so the keyboard stays in the
    // drawer — the menu's list taken away, on the drawer's own element — in
    // the state as on the page. The window moved after it, so the line read
    // is one drawn past the menu's closing.
    run(
        "page.open",
        Value::object([("kind", Value::text("help")), ("in", Value::text("drawer"))]),
    );
    run("leader.tap", none());
    window.until("the leader menu beside a page in its drawer", |seen| {
        seen.get("shows") == "footer,drawer" && seen.get("keyboard") == "drawer"
    });
    run("drawer.back", none());
    run(
        "window.move",
        Value::object([("x", Value::Integer(40)), ("y", Value::Integer(60))]),
    );
    let beside = window.until("the page left in its drawer", |seen| {
        seen.get("at") == "40,60"
    });
    assert_eq!(
        (beside.get("shows"), beside.get("keyboard")),
        ("footer,drawer", "drawer"),
        "{beside:?}"
    );
    assert_eq!(keyboard_of(&run("shell.read", none())), "drawer");
    run("drawer.back", none());
    let emptied = window.until("its drawer emptied", |seen| seen.get("shows") == "footer");
    assert_eq!(emptied.get("keyboard"), "prompt", "{emptied:?}");
    run("leader.tap", none());
    window.until("the leader menu in its drawer again", |seen| {
        seen.get("shows") == "footer,drawer" && seen.get("keyboard") == "drawer"
    });

    // **A second seat**: its pointer over the info line's chevron and its
    // keyboard on the prompt, where the system's own seat is not, each drawn
    // by the design system's attribute and changing how its element is drawn.
    run(
        "shell.hover",
        Value::object([
            ("seat", Value::Integer(2)),
            ("by", Value::text("pointer")),
            ("target", Value::text("info.expand")),
        ]),
    );
    run(
        "shell.focus",
        Value::object([
            ("seat", Value::Integer(2)),
            ("region", Value::text("prompt")),
        ]),
    );
    let seated = window.until("a second seat's hover and keyboard", |seen| {
        seen.seats() == ["2:hover:info.expand", "2:focus:prompt"]
    });
    assert_eq!(
        (seated.get("styled"), seated.get("keyboard")),
        ("1", "drawer"),
        "a second seat is drawn as the first's states are, and moves nothing of the first's: \
         {seated:?}"
    );

    // **The smallest window**: everything inside it, every control a target.
    run(
        "window.resize",
        Value::object([
            (
                "width",
                Value::Integer(i64::from(nc_catalog::KERNEL.shell_window_min_width)),
            ),
            (
                "height",
                Value::Integer(i64::from(nc_catalog::KERNEL.shell_window_min_height)),
            ),
        ]),
    );
    let least = format!(
        "{}x{}",
        nc_catalog::KERNEL.shell_window_min_width,
        nc_catalog::KERNEL.shell_window_min_height
    );
    let small = window.until("its page at the least size", |seen| {
        seen.get("page") == least
    });
    assert_eq!(
        (
            small.get("fits"),
            small.get("lines"),
            small.get("reachable"),
            small.get("styled")
        ),
        ("1", "1", "1", "1"),
        "{small:?}"
    );

    // **The info line expanded in the smallest window**, over the drawer
    // holding the leader menu: a message and every seat's row, more than the
    // window holds, and the footer's lines still inside it, the info line
    // scrolling what it cannot show. Expanded, its chevron runs
    // `info.collapse`, so the second seat's pointer is over nothing.
    run(
        "info.say",
        Value::object([(
            "text",
            Value::text(
                "A message long enough to wrap onto several rows of the info line in the \
                 smallest window the interface is ever drawn in, so that expanded, the line \
                 holds more than the window has room for",
            ),
        )]),
    );
    run(
        "shell.hover",
        Value::object([
            ("seat", Value::Integer(1)),
            ("by", Value::text("pointer")),
            ("target", Value::text("sidebar.cycle")),
        ]),
    );
    run("info.expand", none());
    let expanded = window.until("its info line expanded at the least size", |seen| {
        seen.get("page") == least && seen.seats() == ["2:focus:prompt"]
    });
    assert_eq!(
        (
            expanded.get("fits"),
            expanded.get("lines"),
            expanded.get("reachable"),
            expanded.get("keyboard"),
        ),
        ("1", "1", "1", "drawer"),
        "the footer's lines stay inside the smallest window: {expanded:?}"
    );
    run("info.collapse", none());
    run("info.clear", none());
    window.until("its info line one line again", |seen| {
        seen.get("page") == least && seen.seats() == ["2:hover:info.expand", "2:focus:prompt"]
    });

    // **Moved by command**: its corner where the state puts it.
    run(
        "window.move",
        Value::object([("x", Value::Integer(80)), ("y", Value::Integer(120))]),
    );
    window.until("its corner where the state put it", |seen| {
        seen.get("at") == "80,120" && seen.get("page") == least
    });

    // **The sidebar**, shown and then given the keyboard, inside the
    // smallest window as well.
    run("sidebar.cycle", none());
    run("sidebar.cycle", none());
    let kept = window.until("the sidebar with the keyboard on it", |seen| {
        seen.get("shows") == "footer,drawer,sidebar"
            && seen.get("keyboard") == "sidebar"
            && seen.get("page") == least
    });
    assert_eq!(
        (
            kept.get("shown"),
            kept.get("fits"),
            kept.get("lines"),
            kept.get("reachable"),
            kept.get("styled"),
            kept.seats(),
        ),
        (
            "1",
            "1",
            "1",
            "1",
            "1",
            vec!["2:hover:info.expand", "2:focus:prompt"]
        ),
        "{kept:?}"
    );

    // **Killed**: the daemon keeps everything the window showed, changed in
    // nothing.
    let before = run("shell.read", none());
    drop(window);
    let after = run("shell.read", none());
    assert_eq!(
        before, after,
        "killing the window changed the daemon's state"
    );

    // **Started again**: it draws exactly what it drew, from the daemon.
    let mut window = Desktop::start(&desktop, &display, &home);
    window.until("exactly what it drew before it was killed", |seen| {
        *seen == kept
    });

    // **Hidden by the state**.
    run("window.hide", none());
    let hidden = window.until("itself hidden", |seen| seen.get("shown") == "0");
    assert_eq!(hidden.get("decorated"), "0", "{hidden:?}");
    drop(window);
    drop(display);

    // **Nothing written in the home it was handed**: its webview keeps
    // nothing of its own.
    assert_eq!(
        home.written(),
        Vec::<std::path::PathBuf>::new(),
        "the window wrote in the home it was handed"
    );
}
