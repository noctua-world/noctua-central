//! A program registered with this system's service manager, so that it is
//! started and, when it dies, started again (`NOCTUA.md §17.3`): a systemd
//! user unit on Linux, a launch agent on macOS, a scheduled task on Windows.
//!
//! # What each manager is told, as the hosted runners measured it
//!
//! | Manager | The restorer | What it restores |
//! |---|---|---|
//! | systemd | `Restart=on-failure` after `RestartSec` | a death by `SIGKILL` or a non-zero exit; `SIGTERM` and exit 0 are clean stops, left down |
//! | launchd | `RunAtLoad`, `KeepAlive` with `SuccessfulExit` false, `ThrottleInterval` | a death by any signal, `SIGTERM` among them, or a non-zero exit, at most once per throttle; exit 0 left down |
//! | Task Scheduler | a `RegistrationTrigger` and a `LogonTrigger`, each repeating every minute without end, `IgnoreNew` | whatever stopped, at the next repetition — a clean exit as well as a crash |
//!
//! **Windows restores by repetition, never by how a program ended**
//! (`NOCTUA.md §2.7`). Its `RestartOnFailure` restarts a task that failed to
//! start and not a program that died — measured, a task whose program exited 1
//! was never run again — so the restorer is a repetition every minute, refused
//! while an instance runs (`IgnoreNew`), from the moment the task is
//! registered and from each logon. Nothing there tells a crash from a clean
//! exit, and [`Manager::restarts_a_clean_exit`] says so: a program that must
//! stay down after a clean exit honours its own mark each time it is started,
//! as the daemon reads its `stopped` mark when it is started `--restored`.
//!
//! # One body, every system
//!
//! The generators — [`systemd_unit`], [`launchd_plist`] and [`task_xml`] — the
//! commands each manager is asked and the readers of what it answers are
//! compiled on every system, so the Linux job tests and mutates all three of
//! each. Which manager this system has is [`MANAGER`], fixed when the crate is
//! built for it and never asked of the machine, and a [`Registrar`] is the
//! one that runs its tool.
//!
//! # What this answers (`§2.4`)
//!
//! A manager starting something other than what it was asked to. Every value
//! a definition carries is escaped for the format that reads it — systemd's
//! specifiers, variables, quotes and backslashes; XML's five characters in a
//! plist and in a task, whose arguments are first quoted as Windows splits a
//! command line — and one no escape can carry is refused: a control
//! character anywhere, a `%` in what a task runs, since Task Scheduler
//! expands `%NAME%` there and writes no other form of one, and a `$`, `\`,
//! `"` or `'` in a unit's program, which systemd does not read back as
//! itself there. The program is an
//! absolute path, and so is every tool asked: `systemctl` from the absolute
//! entries of `PATH` alone, `/bin/launchctl`, and `schtasks.exe` under
//! `%SystemRoot%`, so a relative entry of `PATH` never chooses what runs. A
//! name is letters, digits, `.`, `_` and `-`, opening with a letter or a
//! digit, so no name is read by a tool as one of its options. A definition
//! file is created new and private (`create_private`), never through a link,
//! and a name the manager already holds is refused before one is written, so a
//! registration never replaces a service, whoever made it, and one that fails
//! takes back only what it made. No credential is stored: a task runs with the
//! logged-on user's own token, at least privilege.
//!
//! # What it does not
//!
//! A process running as the same user, which can rewrite or remove any
//! definition this writes, register a program of its own under any name, or
//! stop the manager's restorer; and whoever the system lets past every access
//! list. On Windows, registering a task as a user who is not an administrator
//! is not measured: the hosted runner is elevated.

use std::ffi::{OsStr, OsString};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Failed, User};

/// The most characters a name holds, its `.service` or its `\` included.
const NAME_MAX: usize = 200;

/// The longest a restorer is told to wait before it starts a program again.
const RESTART_AFTER_MAX: Duration = Duration::from_secs(3_600);

/// How long [`Registrar::unregister`] waits for its manager to forget a
/// service it was told to remove.
///
/// **A bound, not a pause**: launchd lets a program it boots out exit before
/// the label leaves its domain, so the answer is asked again until it holds or
/// this passes.
const FORGOTTEN_WITHIN: Duration = Duration::from_secs(20);

/// How often a wait on a manager's answer asks again.
const ASKED_EVERY: Duration = Duration::from_millis(100);

/// What Task Scheduler repeats a task every: the restorer's interval.
const REPEATED_EVERY: &str = "PT1M";

/// Which service manager registers a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    /// On Linux, systemd's user manager: a unit in a directory it reads.
    Systemd,
    /// On macOS, launchd: an agent in the user's graphical domain.
    Launchd,
    /// On Windows, Task Scheduler: a task in the root folder.
    TaskScheduler,
}

/// The manager this system registers a program with, fixed when this crate is
/// built for the system and never asked of the machine.
///
/// **A Unix other than macOS is taken to run systemd**, as Linux does: Linux is
/// the system the gate runs on, and on another one a `systemctl` that is not
/// there fails [`Registrar::for_user`], naming it.
pub const MANAGER: Manager = if cfg!(windows) {
    Manager::TaskScheduler
} else if cfg!(target_os = "macos") {
    Manager::Launchd
} else {
    Manager::Systemd
};

impl Manager {
    /// Every manager this module writes for.
    pub const ALL: [Self; 3] = [Self::Systemd, Self::Launchd, Self::TaskScheduler];

    /// Whether this manager's restorer starts a program again after it exits
    /// 0: true for Task Scheduler alone, whose restorer is a repetition that
    /// starts whatever is not running (see the module's header).
    #[must_use]
    pub const fn restarts_a_clean_exit(self) -> bool {
        matches!(self, Self::TaskScheduler)
    }

    /// Whether a `%` in the program or an argument reaches the program as
    /// written.
    ///
    /// False for Task Scheduler, which expands `%NAME%` in what a task runs and
    /// has no way to write a `%` it leaves alone, so [`task_xml`] refuses one.
    #[must_use]
    pub const fn carries_a_percent_sign(self) -> bool {
        !matches!(self, Self::TaskScheduler)
    }

    /// Whether a load this manager refused can leave the service loaded.
    ///
    /// True for systemd alone, whose `start` loads a unit before it fails to
    /// start it and keeps it failed; launchd's bootstrap and Task Scheduler's
    /// create refuse whole, so what a failed one takes back is its file, never
    /// a service the manager holds under that name, which is someone else's.
    const fn a_refused_load_leaves_it_loaded(self) -> bool {
        matches!(self, Self::Systemd)
    }

    /// Where this manager reads, at each login, a program registered to be
    /// started then, for a user whose configuration base directory is
    /// `config`.
    ///
    /// For systemd, its user units in the `user` folder of the `systemd`
    /// folder in that base, as `$XDG_CONFIG_HOME` names the base; for launchd,
    /// its agents in the `LaunchAgents` folder beside it, the base being the
    /// Library's `Preferences`; nothing for Task Scheduler, which keeps its own
    /// copy of a task and starts it at each logon by the trigger it carries.
    #[must_use]
    pub fn loaded_at_login(self, config: &Path) -> Option<PathBuf> {
        match self {
            Self::Systemd => Some(config.join("systemd").join("user")),
            Self::Launchd => config.parent().map(|library| library.join("LaunchAgents")),
            Self::TaskScheduler => None,
        }
    }

    /// Whether `name` is one this manager registers a service under.
    ///
    /// Letters, digits, `.`, `_` and `-`, opening with a letter or a digit —
    /// and for systemd ending in `.service`, for Task Scheduler opening with
    /// the `\` of the root folder — so that no name is read as an option, a
    /// path or a pattern by the tool it is handed to.
    #[must_use]
    pub fn takes(self, name: &str) -> bool {
        let stem = match self {
            Self::Systemd => name.strip_suffix(".service"),
            Self::Launchd => Some(name),
            Self::TaskScheduler => name.strip_prefix('\\'),
        };
        name.chars().count() <= NAME_MAX && stem.is_some_and(is_a_plain_name)
    }

    /// Whether `prefix` is one [`Registrar::registered_under`] can look for:
    /// the start of a name this manager takes.
    fn takes_as_a_prefix(self, prefix: &str) -> bool {
        let stem = match self {
            Self::TaskScheduler => prefix.strip_prefix('\\'),
            Self::Systemd | Self::Launchd => Some(prefix),
        };
        stem.is_some_and(is_a_plain_name)
    }

    /// The name a service's definition file has, in the directory it is
    /// registered from.
    fn file_for(self, name: &str) -> String {
        match self {
            Self::Systemd => name.to_owned(),
            Self::Launchd => format!("{name}.plist"),
            Self::TaskScheduler => format!("{}.xml", name.trim_start_matches('\\')),
        }
    }

    /// What a name this manager takes is, as a refusal says it.
    const fn the_rule_for_a_name(self) -> &'static str {
        match self {
            Self::Systemd => {
                "letters, digits, `.`, `_` and `-`, opening with a letter or a digit and ending \
                 in `.service`"
            }
            Self::Launchd => "letters, digits, `.`, `_` and `-`, opening with a letter or a digit",
            Self::TaskScheduler => {
                "a `\\` and then letters, digits, `.`, `_` and `-`, opening with a letter or a \
                 digit"
            }
        }
    }
}

impl core::fmt::Display for Manager {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Systemd => "systemd",
            Self::Launchd => "launchd",
            Self::TaskScheduler => "Task Scheduler",
        })
    }
}

/// Whether `text` is a name's stem: letters, digits, `.`, `_` and `-`,
/// opening with a letter or a digit.
fn is_a_plain_name(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && text.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

/// A program to register: what it is called, how it is started and how soon
/// it is started again.
///
/// **Judged when it is made**, so every generator writes only what some
/// format can carry; what one format cannot — a `%` in what a task runs — its
/// generator refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// The name its manager knows it by.
    name: String,
    /// What a person reading the manager's list is told it is.
    description: String,
    /// The program, an absolute path written in UTF-8.
    program: String,
    /// What the program is handed, in order.
    arguments: Vec<String>,
    /// How many seconds a restorer waits before starting it again.
    restart_after: u64,
    /// The user it runs as, as this system names one.
    user: String,
}

impl Service {
    /// A service named `name` that runs `program` with `arguments` as `user`,
    /// started again `restart_after` it dies.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a name no manager takes, a program that is not
    /// an absolute path written in UTF-8, a description that is empty, opens
    /// or ends with a space, ends with a `\` or holds a control character, an
    /// argument holding a control character, or a delay that is not a whole
    /// number of seconds from one second to an hour.
    pub fn new(
        name: &str,
        description: &str,
        program: &Path,
        arguments: &[String],
        restart_after: Duration,
        user: &User,
    ) -> Result<Self, Error> {
        Self::described(
            name,
            description,
            program,
            arguments,
            restart_after,
            user.to_string(),
        )
    }

    /// What [`Service::new`] makes, for a user given as its system writes
    /// one, which the tests hand a fixed text so that every generator's
    /// output is written out whole.
    fn described(
        name: &str,
        description: &str,
        program: &Path,
        arguments: &[String],
        restart_after: Duration,
        user: String,
    ) -> Result<Self, Error> {
        let described = |why: String| Err(Error::Described(why));
        if !Manager::ALL.iter().any(|manager| manager.takes(name)) {
            return described(format!(
                "`{}` is not a name any service manager here takes: write it in letters, \
                 digits, `.`, `_` and `-`, opening with a letter or a digit",
                shown(name)
            ));
        }
        let Some(written) = program.to_str() else {
            return described(format!(
                "`{}` is not written in UTF-8, which every definition is: install the program \
                 under a path that is",
                program.display()
            ));
        };
        if !program.is_absolute() {
            return described(format!(
                "`{}` is not an absolute path, and a manager would look for it wherever it \
                 starts from: name the program from the root",
                shown(written)
            ));
        }
        for word in core::iter::once(written)
            .chain(core::iter::once(description))
            .chain(arguments.iter().map(String::as_str))
        {
            if word.chars().any(char::is_control) {
                return described(format!(
                    "`{}` holds a control character, which no definition carries as written: \
                     leave it out",
                    shown(word)
                ));
            }
        }
        if description.is_empty()
            || description.starts_with(char::is_whitespace)
            || description.ends_with(char::is_whitespace)
            || description.ends_with('\\')
        {
            return described(format!(
                "the description `{}` is empty, opens or ends with a space, or ends with a `\\`, \
                 which a unit file would read as a line carried on: write it as a sentence",
                shown(description)
            ));
        }
        if restart_after.subsec_nanos() != 0
            || restart_after < Duration::from_secs(1)
            || restart_after > RESTART_AFTER_MAX
        {
            return described(format!(
                "a restorer cannot be told to wait {restart_after:?}: launchd counts its \
                 throttle in whole seconds, so give a whole number of seconds from one to an \
                 hour"
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            description: description.to_owned(),
            program: written.to_owned(),
            arguments: arguments.to_vec(),
            restart_after: restart_after.as_secs(),
            user,
        })
    }

    /// The name its manager knows it by.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The program and then its arguments: every word a definition hands
    /// the system to start.
    fn words(&self) -> impl Iterator<Item = &str> {
        core::iter::once(self.program.as_str()).chain(self.arguments.iter().map(String::as_str))
    }
}

/// At most 64 characters of `text`, for a refusal to quote.
fn shown(text: &str) -> String {
    let mut out: String = text.chars().take(64).collect();
    if text.chars().nth(64).is_some() {
        out.push('…');
    }
    out.escape_debug().to_string()
}

/// The systemd user unit that runs `service` and restarts it on failure.
///
/// `Restart=on-failure` restarts a death by a signal — but `SIGTERM`, `SIGINT`,
/// `SIGHUP` and `SIGPIPE`, which systemd reads as a clean stop — and a
/// non-zero exit; `Type=exec` makes a start that cannot execute the program a
/// failed start rather than a unit reported running. Every word of
/// `ExecStart` is quoted as one word, and the standard streams go nowhere, as
/// they do under the other two managers.
#[must_use]
pub fn systemd_unit(service: &Service) -> String {
    let words: Vec<String> = service.words().map(systemd_word).collect();
    format!(
        "[Unit]\n\
         Description={}\n\
         \n\
         [Service]\n\
         Type=exec\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec={}\n\
         StandardOutput=null\n\
         StandardError=null\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        service.description.replace('%', "%%"),
        words.join(" "),
        service.restart_after,
    )
}

/// One word of an `ExecStart` line, quoted so systemd reads it back as it is.
///
/// **Four characters are systemd's**: `%` opens a specifier and `$` a
/// variable, each written twice to stand for itself — measured, `%%` in a unit
/// reads back as `%` — and inside the double quotes a `\` opens an escape and
/// a `"` closes the word, each written after a `\`. The quotes make one word of
/// what holds a space. **An argument reads back as itself, and the program's
/// `%` does**; a program holding `$`, `\`, `"` or `'` is refused before this
/// writes it ([`systemd_program`]), since systemd reads no variable in the
/// program and refuses a path holding the other three.
fn systemd_word(word: &str) -> String {
    let mut out = String::with_capacity(word.len() + 2);
    out.push('"');
    for character in word.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '%' => out.push_str("%%"),
            '$' => out.push_str("$$"),
            _ => out.push(character),
        }
    }
    out.push('"');
    out
}

/// The launch agent's property list that runs `service` and keeps it alive.
///
/// `KeepAlive` with `SuccessfulExit` false respawns a program that died by any
/// signal or exited non-zero, and leaves one that exited 0; `RunAtLoad` starts
/// it as it is bootstrapped; `ThrottleInterval` is the least time between two
/// starts. The program and its arguments are `ProgramArguments`, one string
/// each, so no shell ever splits them.
#[must_use]
pub fn launchd_plist(service: &Service) -> String {
    let arguments: Vec<String> = service
        .words()
        .map(|word| format!("\t\t<string>{}</string>\n", xml_text(word)))
        .collect();
    let arguments = arguments.concat();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{label}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         {arguments}\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>KeepAlive</key>\n\
         \t<dict>\n\
         \t\t<key>SuccessfulExit</key>\n\
         \t\t<false/>\n\
         \t</dict>\n\
         \t<key>ThrottleInterval</key>\n\
         \t<integer>{throttle}</integer>\n\
         </dict>\n\
         </plist>\n",
        label = xml_text(&service.name),
        throttle = service.restart_after,
    )
}

/// Text as XML carries it: the five characters XML reads, written as their
/// entities, and every other one as it is.
fn xml_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(character),
        }
    }
    out
}

/// The scheduled task that runs `service` and starts it again, as XML text:
/// [`Registrar::register`] writes it in UTF-16 with its byte-order mark, the
/// encoding its declaration names.
///
/// **Two triggers, each repeating every minute without end**: a
/// `RegistrationTrigger`, which fires as the task is registered and then each
/// minute, and a `LogonTrigger` for the user, which does the same from each
/// logon. `IgnoreNew` refuses a repetition while an instance runs, so the
/// repetition is the restorer (see the module's header). It runs with the
/// user's interactive token at least privilege, so no password is stored, for
/// as long as it runs (`PT0S`), on battery and off it, idle or not.
///
/// # Errors
///
/// [`Error::Described`] for a program or an argument holding a `%`, which
/// Task Scheduler would expand as an environment variable's name.
pub fn task_xml(service: &Service) -> Result<String, Error> {
    if let Some(word) = service.words().find(|word| word.contains('%')) {
        return Err(Error::Described(format!(
            "`{}` holds a `%`, which Task Scheduler reads as the start of an environment \
             variable to expand and has no way to write as itself: name the program and its \
             arguments without one",
            shown(word)
        )));
    }
    let user = xml_text(&service.user);
    let repetition = format!(
        "<Repetition><Interval>{REPEATED_EVERY}</Interval><StopAtDurationEnd>false\
         </StopAtDurationEnd></Repetition>"
    );
    let arguments: Vec<String> = service
        .arguments
        .iter()
        .map(|argument| windows_argument(argument))
        .collect();
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
         <Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
         \x20 <RegistrationInfo>\n\
         \x20   <Description>{description}</Description>\n\
         \x20 </RegistrationInfo>\n\
         \x20 <Triggers>\n\
         \x20   <LogonTrigger>\n\
         \x20     <UserId>{user}</UserId>\n\
         \x20     {repetition}\n\
         \x20   </LogonTrigger>\n\
         \x20   <RegistrationTrigger>\n\
         \x20     {repetition}\n\
         \x20   </RegistrationTrigger>\n\
         \x20 </Triggers>\n\
         \x20 <Principals>\n\
         \x20   <Principal id=\"Author\">\n\
         \x20     <UserId>{user}</UserId>\n\
         \x20     <LogonType>InteractiveToken</LogonType>\n\
         \x20     <RunLevel>LeastPrivilege</RunLevel>\n\
         \x20   </Principal>\n\
         \x20 </Principals>\n\
         \x20 <Settings>\n\
         \x20   <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n\
         \x20   <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n\
         \x20   <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n\
         \x20   <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n\
         \x20   <Priority>5</Priority>\n\
         \x20   <IdleSettings>\n\
         \x20     <StopOnIdleEnd>false</StopOnIdleEnd>\n\
         \x20     <RestartOnIdle>false</RestartOnIdle>\n\
         \x20   </IdleSettings>\n\
         \x20 </Settings>\n\
         \x20 <Actions Context=\"Author\">\n\
         \x20   <Exec>\n\
         \x20     <Command>{program}</Command>\n\
         \x20     <Arguments>{arguments}</Arguments>\n\
         \x20   </Exec>\n\
         \x20 </Actions>\n\
         </Task>\n",
        description = xml_text(&service.description),
        program = xml_text(&service.program),
        arguments = xml_text(&arguments.join(" ")),
    ))
}

/// One argument of a Windows command line, quoted so the program's runtime
/// splits it back into exactly this.
///
/// **The rule the Microsoft C runtime and Rust's own reader split by**: a word
/// holding a space, a tab or a `"`, or none at all, is wrapped in quotes; a
/// `"` inside is written after a `\`, and each run of `\` before a `"` — the
/// closing one included — is doubled, while a `\` before anything else is
/// itself.
fn windows_argument(argument: &str) -> String {
    let quoted = argument.is_empty() || argument.contains([' ', '\t', '"']);
    let mut out = String::with_capacity(argument.len() + 2);
    if quoted {
        out.push('"');
    }
    let mut backslashes = 0_usize;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
        } else {
            if character == '"' {
                out.extend(core::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
        }
        out.push(character);
    }
    if quoted {
        out.extend(core::iter::repeat_n('\\', backslashes));
        out.push('"');
    }
    out
}

/// `text` in UTF-16, little-endian, after its byte-order mark: the encoding a
/// task's XML declares, which `schtasks` reads it in.
fn utf16_with_its_mark(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// Nothing, when systemd starts `service`'s program by the path it is written
/// as; otherwise the refusal a unit for it is.
///
/// **What no escape carries in the program**: measured with systemd 260's
/// `systemd-analyze verify`, a `$$` in the program stays `$$` — the program is
/// never a variable, so nothing undoes the escape — and a program holding `\`
/// is refused as a path with special characters, as one holding `"` or `'` is.
fn systemd_program(service: &Service) -> Result<(), Error> {
    if let Some(character) = service
        .program
        .chars()
        .find(|character| matches!(character, '$' | '\\' | '"' | '\''))
    {
        return Err(Error::Described(format!(
            "`{}` holds a `{character}`, which systemd does not read back as itself in a \
             unit's program: install the program under a path without `$`, `\\`, `\"` or `'`",
            shown(&service.program)
        )));
    }
    Ok(())
}

/// The definition `manager` is handed for `service`: the file's name, and the
/// bytes it holds.
fn definition(manager: Manager, service: &Service) -> Result<(String, Vec<u8>), Error> {
    if !manager.takes(&service.name) {
        return Err(Error::Described(format!(
            "`{}` is not a name {manager} registers a service under: write it as {}",
            shown(&service.name),
            manager.the_rule_for_a_name()
        )));
    }
    let bytes = match manager {
        Manager::Systemd => {
            systemd_program(service)?;
            systemd_unit(service).into_bytes()
        }
        Manager::Launchd => launchd_plist(service).into_bytes(),
        Manager::TaskScheduler => utf16_with_its_mark(&task_xml(service)?),
    };
    Ok((manager.file_for(&service.name), bytes))
}

/// Nothing, when the manager holds no service named `name`, which `held` says.
///
/// Otherwise the refusal a registration over that service is: a registration
/// never replaces one, whoever made it.
fn not_held(name: &str, held: bool) -> Result<(), Error> {
    if held {
        return Err(Error::Described(format!(
            "`{}` is already registered with this system's service manager, and a \
             registration never replaces one: unregister it first",
            shown(name)
        )));
    }
    Ok(())
}

/// What a manager says of a service: the counters it keeps itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Counters {
    /// `systemctl --user show`'s `ActiveState`, `MainPID` and `NRestarts`.
    Systemd {
        /// `active` while the program runs.
        active_state: String,
        /// The program's process, or 0 when none runs.
        main_pid: u32,
        /// How many times systemd restarted it on its own.
        restarts: u64,
    },
    /// `launchctl print`'s `state`, `pid`, `runs` and `last exit code`.
    Launchd {
        /// `running` while the program runs.
        state: String,
        /// The program's process, while one runs.
        pid: Option<u32>,
        /// How many times launchd started it.
        runs: u64,
        /// What the last instance exited with, once one has exited.
        last_exit: Option<i64>,
    },
    /// `schtasks /Query /V`'s `Status`, `Last Run Time` and `Last Result`.
    TaskScheduler {
        /// The task's state as Windows words it in its display language, kept
        /// for a person to read and never judged: in English, `Running` while an
        /// instance runs.
        status: String,
        /// When an instance was last started, or a repetition refused one, written
        /// as `schtasks` writes the time.
        last_run: String,
        /// How the last instance ended, or why the last start was refused.
        last_result: i64,
    },
}

impl Counters {
    /// Whether the program runs now, as its manager says.
    #[must_use]
    pub fn running(&self) -> bool {
        match self {
            Self::Systemd {
                active_state,
                main_pid,
                ..
            } => active_state == "active" && *main_pid != 0,
            Self::Launchd { state, pid, .. } => state == "running" && pid.is_some(),
            // **By the last result, which no display language changes**: the
            // status beside it is a word Windows translates.
            Self::TaskScheduler { last_result, .. } => {
                matches!(*last_result, TASK_RUNNING | REFUSED_WHILE_RUNNING)
            }
        }
    }

    /// The program's process, where its manager names one: systemd and
    /// launchd, never Task Scheduler.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        match self {
            Self::Systemd { main_pid, .. } => (*main_pid != 0).then_some(*main_pid),
            Self::Launchd { pid, .. } => *pid,
            Self::TaskScheduler { .. } => None,
        }
    }

    /// Whether the manager started the program again on its own since `before`
    /// was read: systemd's restarts, or launchd's runs, counted up; a task's
    /// last run moved.
    ///
    /// **A task's last run moves on a repetition `IgnoreNew` refused, too**, so
    /// the move alone says a repetition came, and [`Counters::running`] beside
    /// it says that one started the program.
    #[must_use]
    pub fn ran_since(&self, before: &Self) -> bool {
        match (self, before) {
            (Self::Systemd { restarts, .. }, Self::Systemd { restarts: was, .. }) => restarts > was,
            (Self::Launchd { runs, .. }, Self::Launchd { runs: was, .. }) => runs > was,
            (Self::TaskScheduler { last_run, .. }, Self::TaskScheduler { last_run: was, .. }) => {
                last_run != was
            }
            _ => false,
        }
    }

    /// Whether the last instance ended well — exit 0 — as its manager says;
    /// nothing where the manager keeps no word of it.
    ///
    /// **A unit that ended well is one systemd forgets**: one that exits 0 and is
    /// not restored goes inactive, and its counters with it, so it says only
    /// that an instance ended otherwise, by the unit's `failed` state. A
    /// task's last result is an end only once no instance runs.
    #[must_use]
    pub fn ended_well(&self) -> Option<bool> {
        match self {
            Self::Systemd { active_state, .. } => (active_state == "failed").then_some(false),
            Self::Launchd { last_exit, .. } => last_exit.map(|code| code == 0),
            Self::TaskScheduler { last_result, .. } => {
                (!self.running()).then_some(*last_result == 0)
            }
        }
    }

    /// What `manager` answered when its counters of the service `name` were
    /// asked, read.
    fn read(manager: Manager, answer: &str, name: &str) -> Option<Self> {
        match manager {
            Manager::Systemd => Self::from_systemctl_show(answer),
            Manager::Launchd => Self::from_launchctl_print(answer),
            Manager::TaskScheduler => Self::from_schtasks(answer, name),
        }
    }

    /// `KEY=value` lines, as `systemctl show` writes the properties asked.
    fn from_systemctl_show(answer: &str) -> Option<Self> {
        let (mut active_state, mut main_pid, mut restarts) = (None, None, None);
        for line in answer.lines() {
            match line.split_once('=') {
                Some(("ActiveState", value)) => active_state = Some(value.to_owned()),
                Some(("MainPID", value)) => main_pid = Some(value.parse().ok()?),
                Some(("NRestarts", value)) => restarts = Some(value.parse().ok()?),
                _ => {}
            }
        }
        Some(Self::Systemd {
            active_state: active_state?,
            main_pid: main_pid?,
            restarts: restarts?,
        })
    }

    /// The service's own `key = value` lines, as `launchctl print` writes them.
    ///
    /// They are one tab in; the lines a further tab in are the blocks nested
    /// inside the service, whose keys keep that tab and so are never one of
    /// these.
    fn from_launchctl_print(answer: &str) -> Option<Self> {
        let (mut state, mut pid, mut runs, mut last_exit) = (None, None, None, None);
        for line in answer.lines() {
            let Some(property) = line.strip_prefix('\t') else {
                continue;
            };
            match property.split_once(" = ") {
                Some(("state", value)) => state = Some(value.trim().to_owned()),
                Some(("pid", value)) => pid = Some(value.trim().parse().ok()?),
                Some(("runs", value)) => runs = Some(value.trim().parse().ok()?),
                // `(never exited)` until one has, which is no code.
                Some(("last exit code", value)) => last_exit = value.trim().parse().ok(),
                _ => {}
            }
        }
        Some(Self::Launchd {
            state: state?,
            pid,
            runs: runs?,
            last_exit,
        })
    }

    /// What `schtasks /Query /TN <name> /V /FO CSV` answers, read.
    ///
    /// A header, then a row for each of the task's triggers, which agree on
    /// what is read here: each column by its place ([`TASK_COLUMNS`]), and the
    /// row's own name held to `name`, so that a column that moved is not read.
    fn from_schtasks(answer: &str, name: &str) -> Option<Self> {
        let mut rows = answer
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(csv_fields);
        let header = rows.next()?;
        let row = rows.find(|row| *row != header)?;
        let [named, status, last_run, last_result] = TASK_COLUMNS.map(|column| row.get(column));
        if !named?.eq_ignore_ascii_case(name) {
            return None;
        }
        Some(Self::TaskScheduler {
            status: status?.clone(),
            last_run: last_run?.clone(),
            last_result: last_result?.trim().parse().ok()?,
        })
    }
}

/// Where `schtasks /Query /V /FO CSV` writes each column [`Counters`] reads,
/// counted from 0: the task's name, its status, its last run and its last
/// result.
///
/// **By place, never by the header's words**, which Windows writes in its
/// display language, as it does the status; the order is the tool's own.
const TASK_COLUMNS: [usize; 4] = [1, 3, 5, 6];

/// A task's last result while an instance it started runs, which the
/// headers name `SCHED_S_TASK_RUNNING`.
const TASK_RUNNING: i64 = 0x0004_1301;

/// A task's last result once a repetition came while an instance ran,
/// and `IgnoreNew` refused it.
///
/// `ERROR_REQUEST_REFUSED` as an `HRESULT`, `0x800710E0`, which `schtasks`
/// writes as a signed number. The instance's end writes its own result over
/// it, which the scratch service killed past such a refusal holds on the
/// hosted runner.
const REFUSED_WHILE_RUNNING: i64 = -2_147_020_576;

/// The fields of one line of `schtasks`' CSV: each between quotes, a `""`
/// inside one standing for a `"`, joined by commas outside them.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = line.trim_end_matches('\r').chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(core::mem::take(&mut field)),
            _ => field.push(character),
        }
    }
    fields.push(field);
    fields
}

/// The first word of each line, the unit, as each of `systemctl list-units`
/// and `list-unit-files` writes one per line with `--no-legend`.
fn first_words(answer: &str) -> Vec<String> {
    answer
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// Every label in the `services` block of `launchctl print <domain>`: one a
/// line, last on it, after the program's process and its last exit.
fn launchd_labels(answer: &str) -> Vec<String> {
    let mut labels = Vec::new();
    let mut inside = false;
    for line in answer.lines() {
        let trimmed = line.trim();
        if !inside {
            inside = trimmed == "services = {";
        } else if trimmed == "}" {
            break;
        } else if let Some(label) = trimmed.split_whitespace().last() {
            labels.push(label.to_owned());
        }
    }
    labels
}

/// Every task's path: of each row `schtasks /Query /FO CSV /NH` writes, the
/// first field.
fn task_names(answer: &str) -> Vec<String> {
    answer
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| csv_fields(line).into_iter().next())
        .collect()
}

/// Of the names a manager `listed`, those that open with `prefix`, sorted and
/// each once.
///
/// **What keeps a sweep on its own names**: launchd and Task Scheduler list
/// every service they hold, and whatever this answers a caller may go on to
/// unregister.
fn under(prefix: &str, mut listed: Vec<String>) -> Vec<String> {
    listed.retain(|name| name.starts_with(prefix));
    listed.sort();
    listed.dedup();
    listed
}

/// Why a service was not registered, started, read or removed.
#[derive(Debug)]
pub enum Error {
    /// What the service is described with cannot be handed to a manager: the
    /// sentence says what, and what to do.
    Described(String),
    /// The manager's tool is not where it is looked for.
    NoTool(String),
    /// A tool could not be run, or a definition file written or removed.
    Io(Failed),
    /// A tool ran and refused.
    Refused {
        /// What was asked, as the command line reads.
        asked: String,
        /// How it ended, and what it said.
        said: String,
    },
    /// A tool's answer is not one this reads.
    Unreadable {
        /// What was asked, as the command line reads.
        asked: String,
        /// What it answered.
        said: String,
    },
    /// A service was unregistered, and its manager still knows it.
    StillRegistered(String),
    /// A unit was unregistered — stopped, its links and its definition gone —
    /// and systemd still holds it, wanted by a unit it runs, until it reads
    /// its units again: a unit kept in the runtime directory, which is a
    /// test's and is never reloaded away.
    StillWanted(String),
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Described(why) | Self::NoTool(why) => formatter.write_str(why),
            Self::Io(failed) => write!(
                formatter,
                "{failed}: a definition is written in a directory this user can write in, and a \
                 manager's tool is started from where this looks for it"
            ),
            Self::Refused { asked, said } => write!(
                formatter,
                "`{asked}` was refused ({said}): what the manager said is why — and a user \
                 manager that is not running is started by logging in, or by lingering on Linux"
            ),
            Self::Unreadable { asked, said } => write!(
                formatter,
                "`{asked}` answered what this does not read: `{}` — the manager's own output \
                 has changed shape, and its reader in `nc_platform::service` with it",
                shown(said)
            ),
            Self::StillRegistered(name) => write!(
                formatter,
                "`{name}` was unregistered and its manager still knows it: ask the manager \
                 what holds it, and remove it by hand"
            ),
            Self::StillWanted(name) => write!(
                formatter,
                "`{name}` is stopped and its links and its definition are gone, and a unit \
                 the user's manager runs still wants it, which the manager lets go when it \
                 next reads its units: at its next start, or at `systemctl --user \
                 daemon-reload`, which nothing here asks for a unit in the runtime directory"
            ),
        }
    }
}

impl std::error::Error for Error {}

/// Whether a user's service manager outlives the user's sessions, as
/// [`Registrar::linger`] found it and left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lingering {
    /// It did already: logind keeps the user's manager from boot on.
    Already,
    /// It did not, and now does: lingering was enabled.
    Enabled,
    /// It did not, and logind refused to enable it; the text is what it said.
    Refused(String),
    /// The manager has no such thing.
    ///
    /// Both launchd and Task Scheduler start a user's programs from the user's
    /// login alone.
    NotApplicable,
}

/// What this process found around it that says where a manager's tool is.
#[derive(Debug, Default)]
struct Found {
    /// `PATH`.
    path: Option<OsString>,
    /// `XDG_RUNTIME_DIR`.
    runtime: Option<OsString>,
    /// `SystemRoot`.
    system_root: Option<OsString>,
}

/// One answer of a manager's tool.
#[derive(Debug)]
struct Answer {
    /// What was asked, as the command line reads.
    asked: String,
    /// Whether the tool said it succeeded.
    succeeded: bool,
    /// How it ended.
    status: String,
    /// What it wrote to its standard output.
    out: String,
    /// What it wrote to its standard error.
    err: String,
}

impl Answer {
    /// This answer, if the tool said it succeeded.
    fn succeeded(self) -> Result<Self, Error> {
        if self.succeeded {
            return Ok(self);
        }
        let said = if self.err.trim().is_empty() {
            self.out.trim()
        } else {
            self.err.trim()
        };
        Err(Error::Refused {
            said: format!("{}: {}", self.status, shown(said)),
            asked: self.asked,
        })
    }
}

/// This system's service manager, as one user registers with it: its tool,
/// found once, and what that tool is handed each time it is asked.
#[derive(Debug, Clone)]
pub struct Registrar {
    /// Which manager.
    manager: Manager,
    /// The user, as the manager names one: on Unix the uid, and on Windows
    /// the SID.
    user: String,
    /// The manager's tool, by an absolute path.
    tool: PathBuf,
    /// What the tool is handed beyond this process's environment.
    environment: Vec<(String, String)>,
    /// The user manager's runtime directory, on systemd.
    runtime: Option<PathBuf>,
    /// The tool logind is asked through, on systemd, where an absolute entry
    /// of `PATH` holds it.
    loginctl: Option<PathBuf>,
}

impl Registrar {
    /// This system's manager, for `user`, with its tool found where this
    /// process's environment says.
    ///
    /// **Read when it is made, and not again.** systemd's `systemctl` is the
    /// first one in an absolute entry of `PATH` — NixOS keeps none in
    /// `/usr/bin` — and a relative entry is never searched;
    /// `XDG_RUNTIME_DIR`, which `systemctl --user` finds its manager by, is
    /// handed to it as `/run/user/<uid>` where it is unset, empty or relative,
    /// as logind makes it. launchd's tool is `/bin/launchctl`; Task
    /// Scheduler's is `System32\schtasks.exe` under `SystemRoot`.
    ///
    /// # Errors
    ///
    /// [`Error::NoTool`] when no absolute entry of `PATH` holds `systemctl`,
    /// or `SystemRoot` is unset or not an absolute path.
    pub fn for_user(user: &User) -> Result<Self, Error> {
        Self::found(
            MANAGER,
            &user.to_string(),
            &Found {
                path: std::env::var_os("PATH"),
                runtime: std::env::var_os("XDG_RUNTIME_DIR"),
                system_root: std::env::var_os("SystemRoot"),
            },
        )
    }

    /// The registrar for `manager` and `user`, from what was `found`.
    fn found(manager: Manager, user: &str, found: &Found) -> Result<Self, Error> {
        let registrar = |tool: PathBuf, environment, runtime, loginctl| Self {
            manager,
            user: user.to_owned(),
            tool,
            environment,
            runtime,
            loginctl,
        };
        match manager {
            Manager::Systemd => {
                let tool = found
                    .path
                    .as_deref()
                    .and_then(|path| in_an_absolute_entry(path, "systemctl"))
                    .ok_or_else(|| {
                        Error::NoTool(
                            "no absolute entry of `PATH` holds `systemctl`, which a systemd \
                             user unit is registered through: install systemd, or put the \
                             directory holding it on `PATH` by its absolute path"
                                .to_owned(),
                        )
                    })?;
                let named = found
                    .runtime
                    .as_deref()
                    .map(PathBuf::from)
                    .filter(|runtime| runtime.is_absolute());
                let (runtime, handed) = if let Some(runtime) = named {
                    (runtime, Vec::new())
                } else {
                    let runtime = format!("/run/user/{user}");
                    (
                        PathBuf::from(&runtime),
                        vec![("XDG_RUNTIME_DIR".to_owned(), runtime)],
                    )
                };
                let loginctl = found
                    .path
                    .as_deref()
                    .and_then(|path| in_an_absolute_entry(path, "loginctl"));
                Ok(registrar(tool, handed, Some(runtime), loginctl))
            }
            Manager::Launchd => Ok(registrar(
                PathBuf::from("/bin/launchctl"),
                Vec::new(),
                None,
                None,
            )),
            Manager::TaskScheduler => {
                let root = found
                    .system_root
                    .as_deref()
                    .and_then(OsStr::to_str)
                    .filter(|root| is_an_absolute_windows_path(root))
                    .ok_or_else(|| {
                        Error::NoTool(
                            "`SystemRoot` is not set, or is not an absolute path, so where \
                             `schtasks.exe` is cannot be said: run this from an environment \
                             Windows started, which sets it as `C:\\Windows`"
                                .to_owned(),
                        )
                    })?;
                Ok(registrar(
                    PathBuf::from(format!("{root}\\System32\\schtasks.exe")),
                    Vec::new(),
                    None,
                    None,
                ))
            }
        }
    }

    /// Which manager this registers with.
    #[must_use]
    pub const fn manager(&self) -> Manager {
        self.manager
    }

    /// Where systemd reads a user unit it forgets at the next boot.
    ///
    /// That is `$XDG_RUNTIME_DIR/systemd/user`, in the user manager's runtime
    /// directory; it is `None` under the others, which keep nothing of a
    /// registration in a directory of their own.
    #[must_use]
    pub fn volatile_units(&self) -> Option<PathBuf> {
        self.runtime
            .as_ref()
            .map(|runtime| runtime.join("systemd").join("user"))
    }

    /// Registers `service`, its definition written to `directory`, and starts
    /// it: systemd is asked to, launchd starts what it bootstraps
    /// (`RunAtLoad`) and Task Scheduler what it registers (the registration
    /// trigger).
    ///
    /// **Never over a service the manager already holds**, whoever registered
    /// it: a name the manager knows is refused before anything is written, so
    /// a reinstall unregisters first.
    ///
    /// **Where the definition stays is the directory's**: a unit and a plist
    /// are read from it for as long as they are registered, and a task's XML is
    /// removed once Task Scheduler holds its own copy, or by
    /// [`Registrar::unregister`] when it cannot be then. A registration that
    /// fails takes back what it made and nothing else — the definition it
    /// wrote, and on systemd the unit its start left failed — so it leaves
    /// nothing behind; one the manager took is registered, and says so.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a service this manager takes no definition of,
    /// or one whose name it already holds, [`Error::Io`] when the definition
    /// cannot be written — a file already there included, which is never
    /// overwritten — or the tool cannot be run, and [`Error::Refused`] when the
    /// manager refuses it.
    pub fn register(&self, service: &Service, directory: &Path) -> Result<(), Error> {
        let (file, bytes) = definition(self.manager, service)?;
        not_held(&service.name, self.is_registered(&service.name)?)?;
        self.loaded(&service.name, directory, &file, &bytes, |path| {
            self.to_load(&service.name, path)
                .iter()
                .try_for_each(|arguments| self.ask(arguments).and_then(Answer::succeeded).map(drop))
        })
    }

    /// Writes `bytes` as the definition `file` in `directory`, and has `load`
    /// hand it to the manager.
    ///
    /// **Registered once the manager took it, whatever becomes of the copy**:
    /// a task's XML, which Task Scheduler no longer reads, is removed then if it
    /// can be, and otherwise by [`Registrar::unregister`], which removes the
    /// definition under that name — so a copy a scanner holds open a moment
    /// longer never reports a task the manager holds as one it refused.
    fn loaded(
        &self,
        name: &str,
        directory: &Path,
        file: &str,
        bytes: &[u8],
        load: impl FnOnce(&Path) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let path = directory.join(file);
        write_new(&path, bytes)?;
        if let Err(refused) = load(&path) {
            // **The registration's own refusal is the one reported**: what
            // taking it back says is beside the point it already failed at.
            if self.manager.a_refused_load_leaves_it_loaded() {
                drop(self.unregister(name, directory));
            } else {
                drop(remove_if_there(&path));
            }
            return Err(refused);
        }
        if self.manager == Manager::TaskScheduler {
            drop(remove_if_there(&path));
        }
        Ok(())
    }

    /// Has the manager start the service named `name` at each login as well, its
    /// definition being in `directory`.
    ///
    /// **Only systemd is told**, with `enable`: a link to the unit among
    /// those `default.target` wants, which the manager reads as it starts —
    /// at a login, or at boot for a user it lingers for — and never with the
    /// reload `enable` asks of a running manager by default, which would read
    /// again every unit the user has. A unit in the runtime directory
    /// ([`Registrar::volatile_units`]) is enabled there too (`--runtime`), so
    /// both go at the next boot. launchd starts at login every agent in the
    /// folder [`Manager::loaded_at_login`] names, and Task Scheduler a task by
    /// its logon trigger, so there it is where the definition is, or what it
    /// says.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a name this manager does not take,
    /// [`Error::Io`] when the tool cannot be run, and [`Error::Refused`] when
    /// the manager refuses.
    pub fn enable(&self, name: &str, directory: &Path) -> Result<(), Error> {
        self.takes(name)?;
        for arguments in self.to_enable(name, directory) {
            self.ask(&arguments).and_then(Answer::succeeded)?;
        }
        Ok(())
    }

    /// Makes this user's manager outlive the user's sessions — systemd's
    /// lingering, so a unit enabled with [`Registrar::enable`] starts at boot
    /// rather than at the first login — where it does not already and logind
    /// lets this user enable it; and answers which it was.
    ///
    /// **Never by a test**: lingering is the machine's, and outlives every
    /// scratch a test makes; its one caller is `cargo xtask dev --install`. A
    /// refusal is an answer and not an error, since what is refused is a
    /// policy of the machine's — a declarative system's configuration among
    /// them — which only its administrator changes.
    ///
    /// # Errors
    ///
    /// [`Error::NoTool`] when no absolute entry of `PATH` holds `loginctl`,
    /// and [`Error::Io`] when it cannot be run.
    pub fn linger(&self) -> Result<Lingering, Error> {
        if self.manager != Manager::Systemd {
            return Ok(Lingering::NotApplicable);
        }
        let loginctl = self.loginctl.as_deref().ok_or_else(|| {
            Error::NoTool(
                "no absolute entry of `PATH` holds `loginctl`, which lingering is asked of and \
                 enabled through: install systemd's logind, or put the directory holding it on \
                 `PATH` by its absolute path"
                    .to_owned(),
            )
        })?;
        let shown = self.ask_with(loginctl, &self.to_ask_lingering())?;
        lingered(&shown, || self.ask_with(loginctl, &self.to_linger()))
    }

    /// Starts the service named `name` now, through its manager.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a name this manager does not take,
    /// [`Error::Io`] when the tool cannot be run, and [`Error::Refused`] when
    /// the manager refuses — a service it does not hold among them.
    pub fn start(&self, name: &str) -> Result<(), Error> {
        self.takes(name)?;
        self.ask(&self.to_start(name))
            .and_then(Answer::succeeded)
            .map(drop)
    }

    /// Stops the service named `name`, removes it and its definition in the
    /// directory `directory`, and holds the manager to having forgotten it.
    ///
    /// **Judged by what the manager knows afterwards, not by what each step
    /// said**: a service already stopped, or a definition already gone, is
    /// part of the way to unregistered, and the manager still knowing the name
    /// once the bound has passed is the one refusal.
    ///
    /// **A unit systemd's running units still want is let go by one reload,
    /// and only a real install's.** The links [`Registrar::enable`] makes are
    /// read as the user's manager starts, and from then on `default.target`
    /// wants the unit, which keeps it loaded once its file and links are gone
    /// until the manager reads its units again. Of a unit kept outside the
    /// runtime directory — where a login reads it — the manager is asked
    /// `daemon-reload` then, once; of one kept in it, a test's, never, and the
    /// unregistration says what still holds it ([`Error::StillWanted`]).
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a name this manager does not take,
    /// [`Error::Io`] when the tool cannot be run or the definition cannot be
    /// removed, [`Error::Refused`] and [`Error::Unreadable`] when what the
    /// manager knows cannot be asked, [`Error::StillWanted`] for a unit in the
    /// runtime directory a running unit still wants, and
    /// [`Error::StillRegistered`] when the manager still knows the name.
    pub fn unregister(&self, name: &str, directory: &Path) -> Result<(), Error> {
        self.takes(name)?;
        for arguments in self.to_stop(name, directory) {
            self.ask(&arguments)?;
        }
        remove_if_there(&directory.join(self.manager.file_for(name)))?;
        for arguments in self.to_forget(name) {
            self.ask(&arguments)?;
        }
        let until = std::time::Instant::now() + FORGOTTEN_WITHIN;
        let mut reloaded = false;
        while self.is_registered(name)? {
            if !reloaded && self.still_wanted(name)? {
                let reload = self
                    .to_let_go(directory)
                    .ok_or_else(|| Error::StillWanted(name.to_owned()))?;
                self.ask(&reload)?.succeeded()?;
                reloaded = true;
                continue;
            }
            if std::time::Instant::now() >= until {
                return Err(Error::StillRegistered(name.to_owned()));
            }
            std::thread::sleep(ASKED_EVERY);
        }
        Ok(())
    }

    /// Whether the manager holds the service `name` only because a unit it
    /// runs still wants it.
    ///
    /// **On systemd alone**: a unit loaded and inactive whose file is gone,
    /// which a running unit names among those it wants; under the others a
    /// service is its registration alone.
    fn still_wanted(&self, name: &str) -> Result<bool, Error> {
        match self.manager {
            Manager::Systemd => Ok(wanted_once_gone(
                &self.ask(&Self::to_ask_what_wants(name))?.succeeded()?.out,
            )),
            Manager::Launchd | Manager::TaskScheduler => Ok(false),
        }
    }

    /// Whether the manager knows a service named `name`, running or not.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a name this manager does not take,
    /// [`Error::Io`] when the tool cannot be run, and [`Error::Refused`] when
    /// it refuses to answer.
    pub fn is_registered(&self, name: &str) -> Result<bool, Error> {
        self.takes(name)?;
        match self.manager {
            Manager::Systemd => {
                let answer = self
                    .ask(&words([
                        "--user",
                        "show",
                        "--property=LoadState",
                        "--value",
                        name,
                    ]))?
                    .succeeded()?;
                Ok(answer.out.trim() != "not-found")
            }
            // **A task asked for by its name first**: `schtasks` answers every
            // failure with exit 1, a task that is not there among them, so a
            // refusal is read as absence only once the whole list says so.
            Manager::TaskScheduler
                if self
                    .ask(&words(["/Query", "/TN", name, "/FO", "CSV", "/NH"]))?
                    .succeeded =>
            {
                Ok(true)
            }
            Manager::Launchd | Manager::TaskScheduler => Ok(self
                .listed()?
                .iter()
                .any(|listed| listed.eq_ignore_ascii_case(name))),
        }
    }

    /// Every service the manager knows whose name begins with `prefix`.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a prefix no name this manager takes begins
    /// with, [`Error::Io`] when the tool cannot be run, and [`Error::Refused`]
    /// when it refuses to answer.
    pub fn registered_under(&self, prefix: &str) -> Result<Vec<String>, Error> {
        if !self.manager.takes_as_a_prefix(prefix) {
            return Err(Error::Described(format!(
                "`{}` does not begin a name {} takes, which is {}",
                shown(prefix),
                self.manager,
                self.manager.the_rule_for_a_name()
            )));
        }
        let found = match self.manager {
            Manager::Systemd => {
                let pattern = format!("{prefix}*");
                let mut found = Vec::new();
                for listing in [
                    words([
                        "--user",
                        "list-units",
                        "--all",
                        "--plain",
                        "--no-legend",
                        "--full",
                        &pattern,
                    ]),
                    words([
                        "--user",
                        "list-unit-files",
                        "--no-legend",
                        "--full",
                        &pattern,
                    ]),
                ] {
                    let answer = self.ask(&listing)?;
                    // **A pattern no unit file matches is a failure that says
                    // nothing**: measured with systemd 260, `list-unit-files`
                    // answers it with exit 1 and no word on either stream,
                    // where `list-units` answers 0. Anything said is read as
                    // the refusal it is.
                    if answer.out.trim().is_empty() && answer.err.trim().is_empty() {
                        continue;
                    }
                    found.extend(first_words(&answer.succeeded()?.out));
                }
                found
            }
            // **Every service there is**: every label in the user's domain and
            // every task on the machine, the system's own and other programs'
            // among them, which `under` alone keeps out of the answer.
            Manager::Launchd | Manager::TaskScheduler => self.listed()?,
        };
        Ok(under(prefix, found))
    }

    /// What the manager counts of the service named `name`.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a name this manager does not take,
    /// [`Error::Io`] when the tool cannot be run, [`Error::Refused`] when it
    /// refuses — a service it does not hold among them — and
    /// [`Error::Unreadable`] when its answer is not one this reads.
    pub fn counters(&self, name: &str) -> Result<Counters, Error> {
        self.takes(name)?;
        let answer = self.ask(&self.to_count(name))?.succeeded()?;
        Counters::read(self.manager, &answer.out, name).ok_or(Error::Unreadable {
            asked: answer.asked,
            said: answer.out,
        })
    }

    /// Kills the instance `pid` of the service named `name` at once, as a
    /// crash ends one: **a door for tests**, behind `test-support`.
    ///
    /// By the manager's own kill where it has one — `systemctl --user kill
    /// --signal=SIGKILL`, `launchctl kill 9` — and by `taskkill /F` on
    /// Windows, whose Task Scheduler names no process of a task's.
    ///
    /// # Errors
    ///
    /// [`Error::Described`] for a name this manager does not take,
    /// [`Error::Io`] when the tool cannot be run, and [`Error::Refused`] when
    /// it refuses.
    #[cfg(any(test, feature = "test-support"))]
    pub fn crash(&self, name: &str, pid: u32) -> Result<(), Error> {
        self.takes(name)?;
        let (tool, arguments) = self.to_crash(name, pid);
        self.ask_with(&tool, &arguments)
            .and_then(Answer::succeeded)
            .map(drop)
    }

    /// The tool, and what it is asked, that kills the instance `pid` of the
    /// service `name` as a crash would.
    #[cfg(any(test, feature = "test-support"))]
    fn to_crash(&self, name: &str, pid: u32) -> (PathBuf, Vec<OsString>) {
        match self.manager {
            Manager::Systemd => (
                self.tool.clone(),
                words(["--user", "kill", "--signal=SIGKILL", name]),
            ),
            Manager::Launchd => (self.tool.clone(), words(["kill", "9", &self.target(name)])),
            // Beside `schtasks.exe`, written as text as that path was: a
            // Windows path is split at its `\` by Windows' `Path` alone.
            Manager::TaskScheduler => {
                let tool = self.tool.to_string_lossy();
                let system32 = tool.strip_suffix("schtasks.exe").unwrap_or(&tool);
                (
                    PathBuf::from(format!("{system32}taskkill.exe")),
                    words(["/F", "/PID", &pid.to_string()]),
                )
            }
        }
    }

    /// `name`, if this manager takes it.
    fn takes(&self, name: &str) -> Result<(), Error> {
        if self.manager.takes(name) {
            return Ok(());
        }
        Err(Error::Described(format!(
            "`{}` is not a name {} registers a service under: write it as {}",
            shown(name),
            self.manager,
            self.manager.the_rule_for_a_name()
        )))
    }

    /// The name launchd gives the user's graphical domain.
    fn domain(&self) -> String {
        format!("gui/{}", self.user)
    }

    /// The name launchd gives the service `name` in the user's domain.
    fn target(&self, name: &str) -> String {
        format!("gui/{}/{name}", self.user)
    }

    /// What loads the definition at `file` and starts the service.
    fn to_load(&self, name: &str, file: &Path) -> Vec<Vec<OsString>> {
        match self.manager {
            // **No `daemon-reload`**: a unit file in a directory the user
            // manager reads is loaded the first time its name is asked for —
            // measured with systemd 260, a unit written into a new
            // `$XDG_RUNTIME_DIR/systemd/user` started at once — and a
            // reload would re-read every unit of the developer's manager.
            Manager::Systemd => vec![words(["--user", "start", name])],
            Manager::Launchd => vec![vec![
                "bootstrap".into(),
                self.domain().into(),
                file.as_os_str().to_owned(),
            ]],
            // **No `/F`**, which replaces a task already there: without it,
            // and with nothing to read on its input, `schtasks` refuses one —
            // so a registration racing another never replaces it.
            Manager::TaskScheduler => vec![vec![
                "/Create".into(),
                "/TN".into(),
                name.into(),
                "/XML".into(),
                file.as_os_str().to_owned(),
            ]],
        }
    }

    /// What has the manager start the service at each login, its definition
    /// being in `directory`.
    fn to_enable(&self, name: &str, directory: &Path) -> Vec<Vec<OsString>> {
        match self.manager {
            Manager::Systemd => vec![self.on_systemds_links("enable", name, directory)],
            Manager::Launchd | Manager::TaskScheduler => Vec::new(),
        }
    }

    /// `systemctl --user <verb>` of the links to the unit `name`.
    ///
    /// Its file is in `directory`. Never reloading the manager, and in the runtime
    /// directory's own links when the unit is there.
    fn on_systemds_links(&self, verb: &str, name: &str, directory: &Path) -> Vec<OsString> {
        let mut arguments = words(["--user", verb, "--no-reload"]);
        if self.volatile_units().as_deref() == Some(directory) {
            arguments.push("--runtime".into());
        }
        arguments.push(name.into());
        arguments
    }

    /// What asks logind whether it keeps this user's manager from boot on.
    fn to_ask_lingering(&self) -> Vec<OsString> {
        words(["show-user", &self.user, "--property=Linger", "--value"])
    }

    /// What asks logind to keep this user's manager from boot on.
    fn to_linger(&self) -> Vec<OsString> {
        words(["enable-linger", &self.user])
    }

    /// What starts the service now.
    fn to_start(&self, name: &str) -> Vec<OsString> {
        match self.manager {
            Manager::Systemd => words(["--user", "start", name]),
            Manager::Launchd => words(["kickstart", &self.target(name)]),
            Manager::TaskScheduler => words(["/Run", "/TN", name]),
        }
    }

    /// What stops the service and takes it from the manager.
    ///
    /// It is asked before the definition, in `directory`, is removed: on systemd
    /// the links [`Registrar::enable`] made go first, which a unit never enabled
    /// answers with nothing done.
    fn to_stop(&self, name: &str, directory: &Path) -> Vec<Vec<OsString>> {
        match self.manager {
            Manager::Systemd => vec![
                self.on_systemds_links("disable", name, directory),
                words(["--user", "stop", name]),
            ],
            Manager::Launchd => vec![words(["bootout", &self.target(name)])],
            Manager::TaskScheduler => vec![
                words(["/End", "/TN", name]),
                words(["/Delete", "/TN", name, "/F"]),
            ],
        }
    }

    /// What the manager is told once the definition is gone: systemd keeps a
    /// failed unit's state until it is reset.
    fn to_forget(&self, name: &str) -> Vec<Vec<OsString>> {
        match self.manager {
            Manager::Systemd => vec![words(["--user", "reset-failed", name])],
            Manager::Launchd | Manager::TaskScheduler => Vec::new(),
        }
    }

    /// What asks systemd whether a running unit still wants the unit `name`:
    /// its load and its activity, whether its file changed since it was
    /// loaded, and the units that want it, as the manager holds them.
    fn to_ask_what_wants(name: &str) -> Vec<OsString> {
        words([
            "--user",
            "show",
            "--property=LoadState",
            "--property=ActiveState",
            "--property=NeedDaemonReload",
            "--property=WantedBy",
            name,
        ])
    }

    /// What has systemd let go of a unit a running unit still wants once its
    /// definition, in `directory`, and its links are gone.
    ///
    /// **One reload, and only of a unit kept outside the runtime directory**:
    /// a real install's, which the user's manager read at a login or a boot.
    /// One kept in the runtime directory is a test's, and nothing here
    /// reloads the manager for it.
    fn to_let_go(&self, directory: &Path) -> Option<Vec<OsString>> {
        (self.manager == Manager::Systemd && self.volatile_units().as_deref() != Some(directory))
            .then(|| words(["--user", "daemon-reload"]))
    }

    /// What asks the manager its counters of the service.
    fn to_count(&self, name: &str) -> Vec<OsString> {
        match self.manager {
            Manager::Systemd => words([
                "--user",
                "show",
                "--property=ActiveState",
                "--property=MainPID",
                "--property=NRestarts",
                name,
            ]),
            Manager::Launchd => words(["print", &self.target(name)]),
            Manager::TaskScheduler => words(["/Query", "/TN", name, "/V", "/FO", "CSV"]),
        }
    }

    /// Every service launchd's domain or Task Scheduler holds.
    fn listed(&self) -> Result<Vec<String>, Error> {
        Ok(match self.manager {
            Manager::Launchd => launchd_labels(
                &self
                    .ask(&words(["print", &self.domain()]))?
                    .succeeded()?
                    .out,
            ),
            Manager::TaskScheduler => task_names(
                &self
                    .ask(&words(["/Query", "/FO", "CSV", "/NH"]))?
                    .succeeded()?
                    .out,
            ),
            Manager::Systemd => Vec::new(),
        })
    }

    /// What the manager's tool answers to `arguments`.
    fn ask(&self, arguments: &[OsString]) -> Result<Answer, Error> {
        self.ask_with(&self.tool, arguments)
    }

    /// What `tool` answers to `arguments`, handed what this registrar hands
    /// every tool and nothing to read.
    #[expect(
        clippy::disallowed_methods,
        reason = "a service manager's own tool, named by an absolute path, its output captured \
                  and never printed: asking it is the one way to register with the manager"
    )]
    fn ask_with(&self, tool: &Path, arguments: &[OsString]) -> Result<Answer, Error> {
        let asked = core::iter::once(tool.as_os_str())
            .chain(arguments.iter().map(OsString::as_os_str))
            .map(OsStr::to_string_lossy)
            .collect::<Vec<_>>()
            .join(" ");
        let mut command = std::process::Command::new(tool);
        command.args(arguments).stdin(std::process::Stdio::null());
        for (key, value) in &self.environment {
            command.env(key, value);
        }
        let output = command.output().map_err(|error| {
            Error::Io(Failed::new(format!("`{asked}` could not be run"), error))
        })?;
        Ok(Answer {
            asked,
            succeeded: output.status.success(),
            status: output.status.to_string(),
            out: String::from_utf8_lossy(&output.stdout).into_owned(),
            err: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// What logind's `answer` to whether it keeps the user's manager from boot
/// on says, and lingering enabled by `enable` where it does not.
///
/// **Enabled only where it is not already**, and a user logind knows nothing
/// of — no session and no lingering, which `show-user` refuses — is one it
/// does not keep. What `enable` was refused with is the answer, in logind's
/// words.
fn lingered(
    answer: &Answer,
    enable: impl FnOnce() -> Result<Answer, Error>,
) -> Result<Lingering, Error> {
    if answer.succeeded && answer.out.trim() == "yes" {
        return Ok(Lingering::Already);
    }
    let enabled = enable()?;
    if enabled.succeeded {
        return Ok(Lingering::Enabled);
    }
    let said = if enabled.err.trim().is_empty() {
        enabled.out.trim()
    } else {
        enabled.err.trim()
    };
    Ok(Lingering::Refused(format!(
        "`{}` {}: {}",
        enabled.asked,
        enabled.status,
        shown(said)
    )))
}

/// Whether systemd's `answer`, the `KEY=value` lines
/// [`Registrar::unregister`] asks for, says a running unit still wants a unit
/// whose file is gone.
///
/// **Loaded, inactive and changed on disk, and wanted**: the manager loads a
/// unit's file when it is first asked for and forgets an inactive unit nothing
/// holds, so one still loaded once its file is removed is held by what the
/// manager read before — measured with systemd 260, a unit a started unit
/// wants stays loaded, `NeedDaemonReload` true, until that unit goes.
fn wanted_once_gone(answer: &str) -> bool {
    let value = |key: &str| {
        answer
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
    };
    value("LoadState") == Some("loaded")
        && value("ActiveState") == Some("inactive")
        && value("NeedDaemonReload") == Some("yes")
        && value("WantedBy").is_some_and(|wanted| !wanted.trim().is_empty())
}

/// `list`, as the arguments a tool is handed.
fn words<const N: usize>(list: [&str; N]) -> Vec<OsString> {
    list.into_iter().map(OsString::from).collect()
}

/// Whether `root` is an absolute Windows path, from a drive's root or a share's.
///
/// **Read as text**, `C:\` or `\\` first, since the path is Windows' while
/// this is compiled for every system: a path with no drive, or a drive and
/// no `\` after it, is read by Windows from the current directory.
fn is_an_absolute_windows_path(root: &str) -> bool {
    let mut characters = root.chars();
    root.starts_with("\\\\")
        || matches!(
            (characters.next(), characters.next(), characters.next()),
            (Some(drive), Some(':'), Some('\\')) if drive.is_ascii_alphabetic()
        )
}

/// The first `name` in an absolute entry of `path`, which is read as this
/// system reads `PATH`.
fn in_an_absolute_entry(path: &OsStr, name: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|entry| entry.is_absolute())
        .map(|entry| entry.join(name))
        .find(|candidate| candidate.is_file())
}

/// `bytes` in a new private file at `path`, made durable before a manager
/// reads it.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let failed = |error| {
        Error::Io(Failed::new(
            format!("`{}` could not be written", path.display()),
            error,
        ))
    };
    let mut file = crate::create_private(path).map_err(failed)?;
    file.write_all(bytes).map_err(failed)?;
    file.sync_all().map_err(failed)
}

/// Removes the file at `path`, which may already be gone.
fn remove_if_there(path: &Path) -> Result<(), Error> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(Error::Io(Failed::new(
            format!("`{}` could not be removed", path.display()),
            error,
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::{
        Counters, Error, Found, Lingering, Manager, Registrar, Service, csv_fields, definition,
        first_words, in_an_absolute_entry, launchd_labels, launchd_plist, not_held, systemd_unit,
        systemd_word, task_names, task_xml, under, utf16_with_its_mark, wanted_once_gone,
        windows_argument, words, xml_text,
    };

    /// The user every definition below is written for, a SID, so that a task's
    /// two `<UserId>`s read as Windows writes them.
    const USER: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.0));
        }
    }

    impl Scratch {
        /// A directory of its own, made private as this crate makes one.
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-services-{}-{name}", std::process::id()));
            drop(std::fs::remove_dir_all(&root));
            crate::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }
    }

    /// A program named by a path that is absolute on every system: beside
    /// this test binary.
    fn program() -> PathBuf {
        std::env::current_exe()
            .expect("this test binary")
            .with_file_name("noctua-daemon")
    }

    /// The program's path, which every system here writes in UTF-8.
    fn written(program: &Path) -> String {
        program.to_str().expect("a path in UTF-8").to_owned()
    }

    /// A service named `name`, running [`program`] with `arguments`.
    fn described(name: &str, arguments: &[&str]) -> Result<Service, Error> {
        Service::described(
            name,
            "Noctua's daemon, 100% & <ours>",
            &program(),
            &arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect::<Vec<_>>(),
            Duration::from_secs(2),
            USER.to_owned(),
        )
    }

    /// A service every format carries.
    fn service(name: &str, arguments: &[&str]) -> Service {
        described(name, arguments).expect("a service every manager here takes")
    }

    /// `stem` as `manager` names a service.
    fn named(manager: Manager, stem: &str) -> String {
        match manager {
            Manager::Systemd => format!("{stem}.service"),
            Manager::Launchd => stem.to_owned(),
            Manager::TaskScheduler => format!("\\{stem}"),
        }
    }

    /// The registrar `manager` would be for user `501`, its tool where a
    /// system keeps it, with nothing found around it but a runtime directory.
    fn registrar(manager: Manager) -> Registrar {
        let runtime = std::env::current_exe().expect("this test binary");
        Registrar {
            manager,
            user: "501".to_owned(),
            tool: PathBuf::from(match manager {
                Manager::Systemd => "/usr/bin/systemctl",
                Manager::Launchd => "/bin/launchctl",
                Manager::TaskScheduler => "C:\\Windows\\System32\\schtasks.exe",
            }),
            environment: Vec::new(),
            runtime: (manager == Manager::Systemd).then_some(runtime),
            loginctl: (manager == Manager::Systemd).then(|| PathBuf::from("/usr/bin/loginctl")),
        }
    }

    /// The lock a scratch service's owner holds in its scratch for as long as
    /// it lives.
    const OWNER: &str = "owner.lock";

    /// How long a scratch is too new for a sweep to judge whether its owner
    /// lives: past the moment between its directory and its lock.
    const NEW_FOR: Duration = Duration::from_secs(60);

    /// This checkout: the directory two above this crate's, fixed when this
    /// test binary is built.
    fn checkout() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("a crate two directories into its checkout")
            .to_owned()
    }

    /// The prefix of every scratch service's name this checkout registers.
    fn prefix() -> String {
        format!("noctua-dev-test-{}-", nc_channel::Checkout::of(&checkout()))
    }

    /// Where every scratch service of this checkout has its scratch: in the
    /// directory `nc-registered`, under the checkout's own `target`.
    ///
    /// **The same for every run of this checkout, whatever its environment**:
    /// a sweep reading another directory than the one a live run made its
    /// scratch in — its own `TMPDIR` — finds no lock held there, and would
    /// unregister that run's service as a dead owner's.
    fn scratches() -> PathBuf {
        checkout().join("target").join("nc-registered")
    }

    /// Whether the owner of the scratch at `scratch` still holds its lock.
    fn owner_lives(scratch: &Path) -> bool {
        std::fs::File::open(scratch.join(OWNER))
            .is_ok_and(|lock| matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
    }

    /// Whether what is `age` old is younger than `new_for`, an age that could
    /// not be read counting as new: what a sweep cannot judge, it leaves for
    /// one later.
    fn too_new(age: Option<Duration>, new_for: Duration) -> bool {
        age.is_none_or(|age| age < new_for)
    }

    /// Whether the scratch at `scratch` was made, or written in, less than
    /// the bound `new_for` ago, or when it was cannot be read.
    fn too_new_to_judge(scratch: &Path, new_for: Duration) -> bool {
        too_new(
            std::fs::metadata(scratch)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok()),
            new_for,
        )
    }

    /// Takes away each scratch in `scratches` under `prefix` whose owner is
    /// dead, once it is `new_for` old.
    ///
    /// **A scratch too new to judge is left alone**, before its lock is asked:
    /// its owner makes the directory and then takes the lock in it, and asked
    /// in between, a lock not yet held would read as an owner that died — and
    /// the asking would hold it, for a moment, against that owner.
    fn sweep_scratches(scratches: &Path, prefix: &str, new_for: Duration) {
        let Ok(entries) = std::fs::read_dir(scratches) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ours = entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(prefix));
            let a_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if ours && a_directory && !too_new_to_judge(&path, new_for) && !owner_lives(&path) {
                drop(std::fs::remove_dir_all(&path));
            }
        }
    }

    /// Every word of an `ExecStart` line is one quoted word, with systemd's
    /// four characters written so that it reads each back as itself.
    #[test]
    fn each_word_of_a_unit_is_one_quoted_word_with_systemds_four_characters_escaped() {
        for (word, as_written) in [
            ("plain", r#""plain""#),
            ("with space", r#""with space""#),
            (r#"a "quoted" word"#, r#""a \"quoted\" word""#),
            (r"back\slash\", r#""back\\slash\\""#),
            ("100%", r#""100%%""#),
            ("%h %%", r#""%%h %%%%""#),
            ("$HOME and ${HOME}", r#""$$HOME and $${HOME}""#),
            ("it's & <it>", r#""it's & <it>""#),
            ("naïve 日本 🦉", "\"naïve 日本 🦉\""),
            ("", r#""""#),
        ] {
            assert_eq!(systemd_word(word), as_written, "{word}");
        }
    }

    /// A unit restarts its program on failure, a second after it dies.
    ///
    /// It hands the program every word as one argument, and its description
    /// keeps its `%` from systemd's specifiers.
    #[test]
    fn a_unit_restarts_on_failure_and_hands_each_word_as_one_argument() {
        let unit = systemd_unit(&service(
            "noctua-dev-test.service",
            &["--restored", "a b", "100%", "$HOME"],
        ));
        assert_eq!(
            unit,
            format!(
                "[Unit]\n\
                 Description=Noctua's daemon, 100%% & <ours>\n\
                 \n\
                 [Service]\n\
                 Type=exec\n\
                 ExecStart={} \"--restored\" \"a b\" \"100%%\" \"$$HOME\"\n\
                 Restart=on-failure\n\
                 RestartSec=2\n\
                 StandardOutput=null\n\
                 StandardError=null\n\
                 \n\
                 [Install]\n\
                 WantedBy=default.target\n",
                systemd_word(&written(&program()))
            )
        );
    }

    /// Text in XML writes the five characters XML reads as entities, and
    /// every other one as it is.
    #[test]
    fn xml_text_writes_the_five_characters_xml_reads_as_their_entities() {
        assert_eq!(
            xml_text(r#"a & b < c > d "e" 'f' &amp; naïve 🦉"#),
            "a &amp; b &lt; c &gt; d &quot;e&quot; &apos;f&apos; &amp;amp; naïve 🦉"
        );
    }

    /// An agent is kept alive unless it exits 0.
    ///
    /// It is started as it is loaded and throttled by its delay, and handed its
    /// program and each argument as a string of its own.
    #[test]
    fn an_agent_is_kept_alive_unless_it_exits_cleanly_and_handed_each_argument_as_a_string() {
        let plist = launchd_plist(&service(
            "io.github.noctua_world.noctua.daemon.501",
            &["--restored", "a & <b>", "naïve"],
        ));
        assert_eq!(
            plist,
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
                 \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\">\n\
                 <dict>\n\
                 \t<key>Label</key>\n\
                 \t<string>io.github.noctua_world.noctua.daemon.501</string>\n\
                 \t<key>ProgramArguments</key>\n\
                 \t<array>\n\
                 \t\t<string>{}</string>\n\
                 \t\t<string>--restored</string>\n\
                 \t\t<string>a &amp; &lt;b&gt;</string>\n\
                 \t\t<string>naïve</string>\n\
                 \t</array>\n\
                 \t<key>RunAtLoad</key>\n\
                 \t<true/>\n\
                 \t<key>KeepAlive</key>\n\
                 \t<dict>\n\
                 \t\t<key>SuccessfulExit</key>\n\
                 \t\t<false/>\n\
                 \t</dict>\n\
                 \t<key>ThrottleInterval</key>\n\
                 \t<integer>2</integer>\n\
                 </dict>\n\
                 </plist>\n",
                xml_text(&written(&program()))
            )
        );
    }

    /// Each argument of a task is quoted as the Microsoft C runtime, and the
    /// reader in Rust's own runtime, split a command line back into it.
    #[test]
    fn each_argument_of_a_task_is_quoted_as_windows_splits_a_command_line() {
        for (argument, as_written) in [
            ("plain", "plain"),
            ("", r#""""#),
            ("with space", r#""with space""#),
            ("with\ttab", "\"with\ttab\""),
            (r#"a"b"#, r#""a\"b""#),
            (r"back\slash", r"back\slash"),
            (r"\\server\share", r"\\server\share"),
            (r"a dir\", r#""a dir\\""#),
            (r"a dir\\", r#""a dir\\\\""#),
            (r#"a\"b"#, r#""a\\\"b""#),
            (r#"a\\"b"#, r#""a\\\\\"b""#),
            ("naïve 🦉", "\"naïve 🦉\""),
        ] {
            assert_eq!(windows_argument(argument), as_written, "{argument}");
        }
    }

    /// A task repeats every minute without end, from its registration and
    /// from each logon of its user.
    ///
    /// It refuses a second instance while one runs, and runs with the user's own
    /// token at least privilege, for as long as it runs, whatever the power or
    /// idleness.
    #[test]
    fn a_task_repeats_every_minute_from_its_registration_and_each_logon_unelevated() {
        let xml = task_xml(&service(
            "\\noctua-dev-test",
            &["--restored", "a \"b\" & <c>", r"a dir\"],
        ))
        .expect("nothing in it Task Scheduler would expand");
        let repetition = "<Repetition><Interval>PT1M</Interval><StopAtDurationEnd>false\
                          </StopAtDurationEnd></Repetition>";
        assert_eq!(
            xml,
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
                 <Task version=\"1.4\" \
                 xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
                 \x20 <RegistrationInfo>\n\
                 \x20   <Description>Noctua&apos;s daemon, 100% &amp; &lt;ours&gt;</Description>\n\
                 \x20 </RegistrationInfo>\n\
                 \x20 <Triggers>\n\
                 \x20   <LogonTrigger>\n\
                 \x20     <UserId>{USER}</UserId>\n\
                 \x20     {repetition}\n\
                 \x20   </LogonTrigger>\n\
                 \x20   <RegistrationTrigger>\n\
                 \x20     {repetition}\n\
                 \x20   </RegistrationTrigger>\n\
                 \x20 </Triggers>\n\
                 \x20 <Principals>\n\
                 \x20   <Principal id=\"Author\">\n\
                 \x20     <UserId>{USER}</UserId>\n\
                 \x20     <LogonType>InteractiveToken</LogonType>\n\
                 \x20     <RunLevel>LeastPrivilege</RunLevel>\n\
                 \x20   </Principal>\n\
                 \x20 </Principals>\n\
                 \x20 <Settings>\n\
                 \x20   <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n\
                 \x20   <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n\
                 \x20   <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n\
                 \x20   <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n\
                 \x20   <Priority>5</Priority>\n\
                 \x20   <IdleSettings>\n\
                 \x20     <StopOnIdleEnd>false</StopOnIdleEnd>\n\
                 \x20     <RestartOnIdle>false</RestartOnIdle>\n\
                 \x20   </IdleSettings>\n\
                 \x20 </Settings>\n\
                 \x20 <Actions Context=\"Author\">\n\
                 \x20   <Exec>\n\
                 \x20     <Command>{}</Command>\n\
                 \x20     <Arguments>--restored &quot;a \\&quot;b\\&quot; &amp; &lt;c&gt;&quot; \
                 &quot;a dir\\\\&quot;</Arguments>\n\
                 \x20   </Exec>\n\
                 \x20 </Actions>\n\
                 </Task>\n",
                xml_text(&written(&program()))
            )
        );
    }

    /// A `%` in what a task runs is refused, since Task Scheduler expands a
    /// variable's name written between two there.
    ///
    /// The two managers that read one as itself carry it: the one table
    /// [`Manager::carries_a_percent_sign`] and the definitions are both held
    /// to.
    #[test]
    fn a_percent_sign_is_refused_only_where_task_scheduler_would_expand_it() {
        for manager in Manager::ALL {
            let in_an_argument = definition(
                manager,
                &service(&named(manager, "noctua-dev-test"), &["100%"]),
            );
            assert_eq!(
                in_an_argument.is_ok(),
                manager.carries_a_percent_sign(),
                "{manager}: {in_an_argument:?}"
            );
            let in_the_program = Service::described(
                &named(manager, "noctua-dev-test"),
                "A description",
                &program().with_file_name("100%").join("noctua-daemon"),
                &[],
                Duration::from_secs(1),
                USER.to_owned(),
            )
            .expect("a program every manager here could be told of");
            let refused = definition(manager, &in_the_program);
            assert_eq!(
                refused.is_ok(),
                manager.carries_a_percent_sign(),
                "{manager}: {refused:?}"
            );
            if let Err(error) = refused {
                assert!(
                    matches!(error, Error::Described(_))
                        && error.to_string().contains("holds a `%`"),
                    "{manager}: {error}"
                );
            }
        }
        assert!(
            Manager::TaskScheduler.restarts_a_clean_exit()
                && !Manager::Systemd.restarts_a_clean_exit()
                && !Manager::Launchd.restarts_a_clean_exit(),
            "only a repetition starts again what exited 0"
        );
    }

    /// A unit's program holding what systemd does not read back is refused.
    ///
    /// A `$`, a `\`, a `"` or a `'` in the program is refused for systemd alone,
    /// while each stands for itself as an argument and in the other formats, and
    /// a `%` and a space in the program are carried. **Found by a review**: the
    /// four were escaped in the program as in an argument, and systemd read the
    /// unit back as another path or refused it.
    #[test]
    fn a_units_program_holding_what_systemd_does_not_read_back_is_refused() {
        let in_the_program = |directory: &str| {
            Service::described(
                &named(Manager::Systemd, "noctua-dev-test"),
                "A description",
                &program().with_file_name(directory).join("noctua-daemon"),
                &[directory.to_owned()],
                Duration::from_secs(1),
                USER.to_owned(),
            )
            .expect("a program a manager could be told of")
        };
        for character in ['$', '\\', '"', '\''] {
            let holding = in_the_program(&format!("a{character}b"));
            match definition(Manager::Systemd, &holding) {
                Err(Error::Described(why)) => {
                    assert!(why.contains(&format!("holds a `{character}`")), "{why}");
                }
                answered => panic!("a program holding `{character}` was answered {answered:?}"),
            }
            assert!(
                definition(Manager::Launchd, &holding).is_ok(),
                "launchd was refused a program holding `{character}`"
            );
            let argument = service(
                &named(Manager::Systemd, "noctua-dev-test"),
                &[&format!("a{character}b")],
            );
            definition(Manager::Systemd, &argument).expect("an argument holding it is carried");
        }
        definition(Manager::Systemd, &in_the_program("100% a"))
            .expect("a `%` and a space in the program are carried");
    }

    /// Each manager's definition is its format, under the file name it is
    /// read from.
    ///
    /// A unit and a plist are in UTF-8, and a task in UTF-16 after its
    /// byte-order mark, with a character past the basic plane as its two
    /// surrogates.
    #[test]
    fn each_definition_is_its_format_and_a_task_is_written_in_utf16_after_its_mark() {
        let bytes = utf16_with_its_mark("<a>🦉</a>");
        assert_eq!(&bytes[..2], [0xFF, 0xFE], "the byte-order mark");
        assert_eq!(
            &bytes[8..12],
            [0x3E, 0xD8, 0x89, 0xDD],
            "🦉 as its two surrogates, each little-endian"
        );
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        assert_eq!(String::from_utf16(&units).expect("UTF-16"), "<a>🦉</a>");

        let systemd = service("noctua-dev-test.service", &["a"]);
        assert_eq!(
            definition(Manager::Systemd, &systemd).expect("a unit"),
            (
                "noctua-dev-test.service".to_owned(),
                systemd_unit(&systemd).into_bytes()
            )
        );
        let launchd = service("noctua-dev-test", &["a"]);
        assert_eq!(
            definition(Manager::Launchd, &launchd).expect("a plist"),
            (
                "noctua-dev-test.plist".to_owned(),
                launchd_plist(&launchd).into_bytes()
            )
        );
        let task = service("\\noctua-dev-test", &["🦉"]);
        assert_eq!(
            definition(Manager::TaskScheduler, &task).expect("a task"),
            (
                "noctua-dev-test.xml".to_owned(),
                utf16_with_its_mark(&task_xml(&task).expect("a task"))
            )
        );
        for (manager, foreign) in [
            (Manager::Systemd, &launchd),
            (Manager::Launchd, &task),
            (Manager::TaskScheduler, &systemd),
        ] {
            assert!(
                matches!(definition(manager, foreign), Err(Error::Described(_))),
                "{manager} wrote a definition under `{}`, a name it does not take",
                foreign.name()
            );
        }
    }

    /// A name is taken only in the shape its manager registers one under, and
    /// never in one a tool could read as an option, a path or a pattern.
    #[test]
    fn a_name_is_taken_only_in_the_shape_its_manager_registers() {
        let long = format!("a{}.service", "b".repeat(200));
        for (manager, name, taken) in [
            (Manager::Systemd, "noctua-dev-0123abcd.1001.service", true),
            (Manager::Systemd, "noctua", false),
            (Manager::Systemd, ".service", false),
            (Manager::Systemd, "-x.service", false),
            (Manager::Systemd, "a b.service", false),
            (Manager::Systemd, "a*.service", false),
            (Manager::Systemd, long.as_str(), false),
            (
                Manager::Launchd,
                "io.github.noctua_world.noctua_dev.daemon.501",
                true,
            ),
            (Manager::Launchd, "_hidden", false),
            (Manager::Launchd, "a/b", false),
            (Manager::Launchd, "\\noctua", false),
            (Manager::Launchd, "", false),
            (Manager::TaskScheduler, "\\noctua.S-1-5-21-1-2-3-1001", true),
            (Manager::TaskScheduler, "noctua", false),
            (Manager::TaskScheduler, "\\a\\b", false),
            (Manager::TaskScheduler, "\\", false),
        ] {
            assert_eq!(manager.takes(name), taken, "{manager}: `{name}`");
        }
    }

    /// A service is refused for each thing no definition can carry, with the
    /// sentence that names it.
    #[test]
    fn a_service_is_refused_for_each_thing_no_definition_can_carry() {
        let one = Duration::from_secs(1);
        let program = program();
        let said = |name: &str, description: &str, program: &Path, argument: &str, after| {
            let arguments = vec![argument.to_owned()];
            match Service::described(
                name,
                description,
                program,
                &arguments,
                after,
                USER.to_owned(),
            ) {
                Err(Error::Described(why)) => why,
                other => panic!(
                    "`{name}`, `{description}`, `{}`, `{argument}`, {after:?}: {other:?}",
                    program.display()
                ),
            }
        };
        Service::described(
            "noctua",
            "A description",
            &program,
            &[],
            one,
            USER.to_owned(),
        )
        .expect("a service every format carries");
        for name in ["-x", "a b", "_x", "a/b", ""] {
            let why = said(name, "A description", &program, "a", one);
            assert!(why.contains("not a name"), "`{name}`: {why}");
        }
        let why = said(
            "noctua",
            "A description",
            Path::new("noctua-daemon"),
            "a",
            one,
        );
        assert!(why.contains("not an absolute path"), "{why}");
        let escape = program.with_file_name("a\u{1b}[2J");
        for (description, program, argument) in [
            ("A description", escape.as_path(), "a"),
            ("A\ndescription", program.as_path(), "a"),
            ("A description", program.as_path(), "a\nb"),
            ("A description", program.as_path(), "a\u{7}b"),
            ("A description", program.as_path(), "a\0b"),
            ("A description", program.as_path(), "a\u{85}b"),
        ] {
            let why = said("noctua", description, program, argument, one);
            assert!(why.contains("control character"), "`{argument}`: {why}");
        }
        for description in ["", " A description", "A description ", "A description\\"] {
            let why = said("noctua", description, &program, "a", one);
            assert!(why.contains("the description"), "`{description}`: {why}");
        }
        for after in [
            Duration::ZERO,
            Duration::from_millis(1_500),
            Duration::from_secs(3_601),
        ] {
            let why = said("noctua", "A description", &program, "a", after);
            assert!(why.contains("cannot be told to wait"), "{after:?}: {why}");
        }
    }

    /// What `systemctl show` answers is read as the counters it names, and
    /// an answer missing one, or holding one that is not a number, is not.
    #[test]
    fn systemds_counters_are_read_from_what_systemctl_show_answers() {
        // Measured with systemd 260, a unit killed by `SIGKILL` and restarted.
        let restarted = "MainPID=3589142\nNRestarts=1\nActiveState=active\n";
        let read = Counters::read(Manager::Systemd, restarted, "x.service").expect("read");
        assert_eq!(
            read,
            Counters::Systemd {
                active_state: "active".to_owned(),
                main_pid: 3_589_142,
                restarts: 1
            }
        );
        assert!(read.running() && read.pid() == Some(3_589_142));
        let stopped = Counters::read(
            Manager::Systemd,
            "MainPID=0\nNRestarts=0\nActiveState=inactive\n",
            "x.service",
        )
        .expect("read");
        assert!(!stopped.running() && stopped.pid().is_none());
        for unreadable in [
            "MainPID=0\nActiveState=inactive\n",
            "MainPID=x\nNRestarts=0\nActiveState=inactive\n",
            "MainPID=0\nNRestarts=-1\nActiveState=inactive\n",
            "",
        ] {
            assert_eq!(
                Counters::read(Manager::Systemd, unreadable, "x.service"),
                None,
                "{unreadable:?}"
            );
        }
    }

    /// What `launchctl print` answers of a service is read from its own lines.
    ///
    /// Those are one tab in, and the blocks nested inside the service are
    /// never read for its counters.
    #[test]
    fn launchds_counters_are_read_from_the_services_own_lines() {
        let running = "gui/501/noctua-dev-test = {\n\
                       \tactive count = 1\n\
                       \tpath = /var/folders/h5/T/noctua-dev-test/noctua-dev-test.plist\n\
                       \ttype = LaunchAgent\n\
                       \tstate = running\n\
                       \n\
                       \tprogram identifier = /Users/runner/work/deps/nc_platform-0123\n\
                       \targuments = {\n\
                       \t\t/Users/runner/work/deps/nc_platform-0123\n\
                       \t\t--exact\n\
                       \t}\n\
                       \n\
                       \tenvironment = {\n\
                       \t\tpid = 99\n\
                       \t\truns = 99\n\
                       \t\tXPC_SERVICE_NAME => noctua-dev-test\n\
                       \t}\n\
                       \n\
                       \tdomain = gui/501 [100002]\n\
                       \tasid = 100002\n\
                       \truns = 2\n\
                       \tpid = 1413\n\
                       \tlast terminating signal = Killed: 9\n\
                       \n\
                       \tevent triggers = {\n\
                       \t\tnoctua.watched = {\n\
                       \t\t\tstate = 1\n\
                       \t\t\truns = 99\n\
                       \t\t\tpid = 99\n\
                       \t\t}\n\
                       \t}\n\
                       }\n";
        let read = Counters::read(Manager::Launchd, running, "noctua-dev-test").expect("read");
        assert_eq!(
            read,
            Counters::Launchd {
                state: "running".to_owned(),
                pid: Some(1_413),
                runs: 2,
                last_exit: None,
            }
        );
        assert!(read.running() && read.pid() == Some(1_413));
        assert_eq!(read.ended_well(), None, "killed, and no exit read");
        let exited = "gui/501/noctua-dev-test = {\n\
                      \tstate = not running\n\
                      \truns = 1\n\
                      \tlast exit code = 0\n\
                      }\n";
        let read = Counters::read(Manager::Launchd, exited, "noctua-dev-test").expect("read");
        assert_eq!(
            read,
            Counters::Launchd {
                state: "not running".to_owned(),
                pid: None,
                runs: 1,
                last_exit: Some(0),
            }
        );
        assert!(!read.running() && read.pid().is_none());
        assert_eq!(read.ended_well(), Some(true));
        // **No code until one has exited**, which launchd writes as words.
        let never = "gui/501/x = {\n\tstate = spawn scheduled\n\truns = 0\n\
                     \tlast exit code = (never exited)\n}\n";
        let read = Counters::read(Manager::Launchd, never, "x").expect("read");
        assert_eq!(read.ended_well(), None, "{read:?}");
        let failed = never.replace("(never exited)", "78");
        let read = Counters::read(Manager::Launchd, &failed, "x").expect("read");
        assert_eq!(read.ended_well(), Some(false), "{read:?}");
        for unreadable in [
            "gui/501/x = {\n\tstate = running\n}\n",
            "gui/501/x = {\n\truns = 1\n}\n",
            "gui/501/x = {\n\tstate = running\n\truns = 1\n\tpid = x\n}\n",
        ] {
            assert_eq!(
                Counters::read(Manager::Launchd, unreadable, "x"),
                None,
                "{unreadable:?}"
            );
        }
    }

    /// What `schtasks /Query /V /FO CSV` answers of a task is read by the
    /// place of its columns, in any display language, from its first row,
    /// which every row of a task's triggers repeats.
    ///
    /// **Whether it runs is its last result**, never the status Windows
    /// translates: an instance started, or a repetition refused because one
    /// runs.
    #[test]
    fn task_schedulers_counters_are_read_by_the_place_of_their_columns() {
        let header = "\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\
                      \"Last Run Time\",\"Last Result\",\"Author\",\"Task To Run\"";
        let row = |task: &str, status: &str, at: &str, result: &str| {
            format!(
                "\"RUNNERVM\",\"{task}\",\"N/A\",\"{status}\",\"Interactive only\",\"{at}\",\
                 \"{result}\",\"RUNNERVM\\runneradmin\",\"D:\\a\\x.exe \"\"a b\"\"\""
            )
        };
        let task = "\\noctua-dev-test";
        // The values a hosted runner answered, across a repetition refused by
        // `IgnoreNew` (`-2147020576`) while an instance ran (`267009`).
        let running = format!(
            "\r\n{header}\r\n{}\r\n{}\r\n",
            row(task, "Running", "11/8/2026 12:29:20 PM", "267009"),
            row(task, "Running", "11/8/2026 12:29:20 PM", "267009")
        );
        let read = Counters::read(Manager::TaskScheduler, &running, task).expect("read");
        assert_eq!(
            read,
            Counters::TaskScheduler {
                status: "Running".to_owned(),
                last_run: "11/8/2026 12:29:20 PM".to_owned(),
                last_result: 267_009
            }
        );
        assert!(read.running() && read.pid().is_none());
        let refused = format!(
            "{header}\n{}\n",
            row(task, "Running", "11/8/2026 12:30:20 PM", "-2147020576")
        );
        let later = Counters::read(Manager::TaskScheduler, &refused, task).expect("read");
        assert!(later.ran_since(&read) && later.running(), "{later:?}");
        // **Another display language**: every word of the header and the
        // status translated, and the time written its way.
        let translated = "\"Hostname\",\"Aufgabenname\",\"Nächste Laufzeit\",\"Status\",\
                          \"Anmeldemodus\",\"Letzte Laufzeit\",\"Letztes Ergebnis\",\"Autor\",\
                          \"Auszuführende Aufgabe\"";
        for (status, result, runs) in [
            ("Wird ausgeführt", "267009", true),
            ("Wird ausgeführt", "-2147020576", true),
            ("Bereit", "1", false),
            ("Bereit", "0", false),
            ("Bereit", "267011", false),
        ] {
            let answer = format!(
                "{translated}\n{}\n",
                row("\\NOCTUA-DEV-TEST", status, "08.11.2026 12:29:20", result)
            );
            let read = Counters::read(Manager::TaskScheduler, &answer, task).expect("read");
            assert_eq!(read.running(), runs, "{read:?}");
            assert_eq!(
                read,
                Counters::TaskScheduler {
                    status: status.to_owned(),
                    last_run: "08.11.2026 12:29:20".to_owned(),
                    last_result: result.parse().expect("a number")
                }
            );
        }
        for unreadable in [
            header.to_owned(),
            format!("{header}\n{}\n", row(task, "Ready", "N/A", "x")),
            format!("{header}\n{}\n", row("\\another-task", "Ready", "N/A", "0")),
            format!("{header}\n\"RUNNERVM\",\"{task}\",\"N/A\",\"Ready\"\n"),
        ] {
            assert_eq!(
                Counters::read(Manager::TaskScheduler, &unreadable, task),
                None,
                "{unreadable:?}"
            );
        }
        assert_eq!(
            csv_fields("\"a\",\"b \"\"c\"\", d\",,\"e\"\r"),
            ["a", "b \"c\", d", "", "e"]
        );
    }

    /// A manager started a program again only when its own counter says so,
    /// and never across two managers' counters.
    #[test]
    fn a_program_ran_again_only_when_its_managers_own_counter_moved() {
        let systemd = |restarts| Counters::Systemd {
            active_state: "active".to_owned(),
            main_pid: 7,
            restarts,
        };
        let launchd = |runs| Counters::Launchd {
            state: "running".to_owned(),
            pid: Some(7),
            runs,
            last_exit: None,
        };
        let task = |last_run: &str| Counters::TaskScheduler {
            status: "Ready".to_owned(),
            last_run: last_run.to_owned(),
            last_result: 0,
        };
        // **The status is never what decides**: in English and running, an
        // instance that ended with its own result is not.
        assert!(
            !Counters::TaskScheduler {
                status: "Running".to_owned(),
                last_run: "12:29:20".to_owned(),
                last_result: 1,
            }
            .running()
        );
        assert!(systemd(1).ran_since(&systemd(0)));
        assert!(!systemd(1).ran_since(&systemd(1)));
        assert!(!systemd(0).ran_since(&systemd(1)));
        assert!(launchd(2).ran_since(&launchd(1)));
        assert!(!launchd(1).ran_since(&launchd(1)));
        assert!(task("12:30:20").ran_since(&task("12:29:20")));
        assert!(!task("12:29:20").ran_since(&task("12:29:20")));
        assert!(!systemd(9).ran_since(&launchd(0)) && !launchd(9).ran_since(&task("x")));
        let between = |active_state: &str, main_pid| Counters::Systemd {
            active_state: active_state.to_owned(),
            main_pid,
            restarts: 0,
        };
        assert!(!between("activating", 7).running() && !between("active", 0).running());
        assert!(
            !Counters::Launchd {
                state: "running".to_owned(),
                pid: None,
                runs: 1,
                last_exit: None,
            }
            .running()
        );
    }

    /// How the last instance ended is what its manager says of it, and
    /// nothing where it keeps no word.
    ///
    /// For systemd that is only a unit that failed, since one that ended well is
    /// forgotten with its counters; a task's last result is an end only once
    /// no instance runs.
    #[test]
    fn how_the_last_instance_ended_is_its_managers_word_or_none() {
        let systemd = |active_state: &str| Counters::Systemd {
            active_state: active_state.to_owned(),
            main_pid: 0,
            restarts: 0,
        };
        assert_eq!(systemd("failed").ended_well(), Some(false));
        for forgotten in ["inactive", "active", "activating"] {
            assert_eq!(systemd(forgotten).ended_well(), None, "{forgotten}");
        }
        let task = |last_result| Counters::TaskScheduler {
            status: "Ready".to_owned(),
            last_run: "12:29:20".to_owned(),
            last_result,
        };
        assert_eq!(task(0).ended_well(), Some(true));
        assert_eq!(task(1).ended_well(), Some(false));
        for running in [super::TASK_RUNNING, super::REFUSED_WHILE_RUNNING] {
            assert_eq!(task(running).ended_well(), None, "{running}");
        }
        let launchd = |last_exit| Counters::Launchd {
            state: "not running".to_owned(),
            pid: None,
            runs: 2,
            last_exit,
        };
        assert_eq!(launchd(Some(0)).ended_well(), Some(true));
        assert_eq!(launchd(Some(1)).ended_well(), Some(false));
        assert_eq!(launchd(None).ended_well(), None);
    }

    /// What each manager reads at login is beside the user's configuration:
    /// launchd's agents beside the Library's folder of preferences, systemd's
    /// units under it; Task Scheduler keeps its own.
    #[test]
    fn what_each_manager_reads_at_login_is_beside_the_users_configuration() {
        assert_eq!(
            Manager::Systemd.loaded_at_login(Path::new("/home/u/.config")),
            Some(PathBuf::from("/home/u/.config/systemd/user"))
        );
        assert_eq!(
            Manager::Launchd.loaded_at_login(Path::new("/Users/u/Library/Preferences")),
            Some(PathBuf::from("/Users/u/Library/LaunchAgents"))
        );
        assert_eq!(
            Manager::TaskScheduler.loaded_at_login(Path::new("C:\\Users\\u\\AppData\\Roaming")),
            None
        );
    }

    /// Lingering is enabled only where logind does not keep the user's
    /// manager already, and a refusal is answered in logind's words.
    ///
    /// A user logind knows nothing of, whose `show-user` is refused, is one it
    /// does not keep. No test asks logind to enable it: these are its answers
    /// as recorded.
    #[test]
    fn lingering_is_enabled_only_where_it_is_not_and_a_refusal_says_why() {
        let answer = |succeeded: bool, out: &str, err: &str| super::Answer {
            asked: "loginctl enable-linger 1000".to_owned(),
            succeeded,
            status: if succeeded {
                "exit status: 0"
            } else {
                "exit status: 1"
            }
            .to_owned(),
            out: out.to_owned(),
            err: err.to_owned(),
        };
        let asked = std::cell::Cell::new(0);
        let enable = |enabled: super::Answer| {
            || {
                asked.set(asked.get() + 1);
                Ok(enabled)
            }
        };
        let already = super::lingered(&answer(true, "yes\n", ""), enable(answer(true, "", "")))
            .expect("asked");
        assert_eq!((already, asked.get()), (Lingering::Already, 0));
        for shown in [
            answer(true, "no\n", ""),
            answer(
                false,
                "",
                "Failed to get user: User ID 1000 is not logged in or lingering",
            ),
        ] {
            let enabled = super::lingered(&shown, enable(answer(true, "", ""))).expect("asked");
            assert_eq!(enabled, Lingering::Enabled, "{shown:?}");
        }
        assert_eq!(asked.get(), 2);
        let refused = super::lingered(
            &answer(true, "no\n", ""),
            enable(answer(false, "", "Could not enable linger: Access denied")),
        )
        .expect("asked");
        assert!(
            matches!(&refused, Lingering::Refused(said)
                if said.contains("enable-linger") && said.contains("Access denied")),
            "{refused:?}"
        );
        let registrar = registrar(Manager::Launchd);
        assert_eq!(registrar.linger().expect("asked"), Lingering::NotApplicable);
    }

    /// What each manager lists of its services is read as their names.
    ///
    /// They are the first field of each of Task Scheduler's rows, the first
    /// word of systemd's lines, and the last of each in launchd's `services`
    /// block.
    #[test]
    fn each_managers_list_of_services_is_read_as_their_names() {
        assert_eq!(
            first_words(
                "noctua-dev-test-a.service loaded active running A scratch service\n\
                 noctua-dev-test-b.service disabled enabled\n\n"
            ),
            ["noctua-dev-test-a.service", "noctua-dev-test-b.service"]
        );
        let domain = "gui/501 = {\n\
                      \ttype = login\n\
                      \thandle = 100002\n\
                      \n\
                      \tsubdomains = {\n\
                      \t\tpid/1234\n\
                      \t}\n\
                      \n\
                      \tservices = {\n\
                      \t\t       0      -        com.apple.xpc.otherbsd\n\
                      \t\t    1413      -        noctua-dev-test-0123abcd-77-1\n\
                      \t\t       0     78        com.apple.something\n\
                      \t}\n\
                      \n\
                      \tunmanaged processes = {\n\
                      \t\tcom.apple.unmanaged.loginwindow.156 = {\n\
                      \t\t\tactive count = 4\n\
                      \t\t}\n\
                      \t}\n\
                      }\n";
        assert_eq!(
            launchd_labels(domain),
            [
                "com.apple.xpc.otherbsd",
                "noctua-dev-test-0123abcd-77-1",
                "com.apple.something"
            ]
        );
        assert_eq!(
            launchd_labels("gui/501 = {\n\ttype = login\n}\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            task_names(
                "\r\n\"\\noctua-dev-test-a\",\"N/A\",\"Ready\"\r\n\
                 \"\\Microsoft\\Windows\\X\",\"11/9/2026 1:00:00 AM\",\"Ready\"\r\n"
            ),
            ["\\noctua-dev-test-a", "\\Microsoft\\Windows\\X"]
        );
    }

    /// `systemctl` is found in an absolute entry and handed a runtime directory.
    ///
    /// It is the first absolute entry of `PATH` that holds it, never a relative
    /// one, and `XDG_RUNTIME_DIR` is handed to it as logind makes it wherever this
    /// process has none of its own to give; `loginctl` is found the same way, in
    /// whichever entry holds it.
    #[test]
    fn systemctl_is_found_in_an_absolute_entry_and_handed_a_runtime_directory() {
        let scratch = Scratch::new("tools");
        let (empty, first, second) = (
            scratch.0.join("empty"),
            scratch.0.join("first"),
            scratch.0.join("second"),
        );
        for directory in [&empty, &first, &second] {
            crate::create_private_directory(directory).expect("a directory");
        }
        for directory in [&first, &second] {
            drop(crate::create_private(&directory.join("systemctl")).expect("a tool"));
            drop(crate::create_private(&directory.join("Cargo.toml")).expect("a file"));
        }
        drop(crate::create_private(&second.join("loginctl")).expect("logind's tool"));
        let path = std::env::join_paths([&empty, &first, &second]).expect("a PATH");
        assert_eq!(
            in_an_absolute_entry(&path, "systemctl"),
            Some(first.join("systemctl"))
        );
        // **A relative entry that holds the name**: the tests run in this
        // crate's directory, which holds a `Cargo.toml`, and `.` is that
        // directory to anything that searches it.
        let relative = std::env::join_paths([PathBuf::from("."), second.clone()]).expect("a PATH");
        assert_eq!(
            in_an_absolute_entry(&relative, "Cargo.toml"),
            Some(second.join("Cargo.toml"))
        );
        assert_eq!(
            in_an_absolute_entry(
                &std::env::join_paths([&empty]).expect("a PATH"),
                "systemctl"
            ),
            None
        );

        let runtime = std::env::current_exe().expect("this test binary");
        for (named, handed) in [
            (Some(runtime.clone().into_os_string()), None),
            (None, Some("/run/user/1001")),
            (Some(OsString::new()), Some("/run/user/1001")),
            (Some(OsString::from("relative/run")), Some("/run/user/1001")),
        ] {
            let found = Registrar::found(
                Manager::Systemd,
                "1001",
                &Found {
                    path: Some(path.clone()),
                    runtime: named.clone(),
                    system_root: None,
                },
            )
            .expect("found");
            assert_eq!(found.tool, first.join("systemctl"));
            assert_eq!(found.loginctl, Some(second.join("loginctl")));
            let expected: Vec<(String, String)> = handed
                .map(|runtime| ("XDG_RUNTIME_DIR".to_owned(), runtime.to_owned()))
                .into_iter()
                .collect();
            assert_eq!(found.environment, expected, "{named:?}");
            let units = handed
                .map_or_else(|| runtime.clone(), PathBuf::from)
                .join("systemd")
                .join("user");
            assert_eq!(found.volatile_units(), Some(units), "{named:?}");
        }
        let nowhere = Registrar::found(
            Manager::Systemd,
            "1001",
            &Found {
                path: Some(std::env::join_paths([&empty]).expect("a PATH")),
                ..Found::default()
            },
        );
        assert!(matches!(nowhere, Err(Error::NoTool(_))), "{nowhere:?}");
    }

    /// The tools of launchd and Task Scheduler are the system's own, named by
    /// absolute paths, and neither keeps a directory of its own.
    #[test]
    fn launchctl_and_schtasks_are_the_systems_own() {
        let launchd = Registrar::found(Manager::Launchd, "501", &Found::default()).expect("found");
        assert_eq!(
            (
                launchd.tool.clone(),
                launchd.environment.clone(),
                launchd.volatile_units()
            ),
            (PathBuf::from("/bin/launchctl"), Vec::new(), None)
        );
        let task = Registrar::found(
            Manager::TaskScheduler,
            USER,
            &Found {
                system_root: Some(OsString::from("C:\\Windows")),
                ..Found::default()
            },
        )
        .expect("found");
        assert_eq!(
            (task.tool.clone(), task.volatile_units()),
            (PathBuf::from("C:\\Windows\\System32\\schtasks.exe"), None)
        );
        let share = Registrar::found(
            Manager::TaskScheduler,
            USER,
            &Found {
                system_root: Some(OsString::from("\\\\server\\share\\Windows")),
                ..Found::default()
            },
        )
        .expect("found");
        assert_eq!(
            share.tool,
            PathBuf::from("\\\\server\\share\\Windows\\System32\\schtasks.exe")
        );
        // **A `SystemRoot` Windows would read from the current directory**:
        // none, a relative one, one rooted on no drive, a drive with no root.
        for root in [
            None,
            Some(OsString::new()),
            Some(OsString::from("Windows")),
            Some(OsString::from("\\Windows")),
            Some(OsString::from("C:Windows")),
            Some(OsString::from("1:\\Windows")),
        ] {
            let refused = Registrar::found(
                Manager::TaskScheduler,
                USER,
                &Found {
                    system_root: root,
                    ..Found::default()
                },
            );
            assert!(matches!(refused, Err(Error::NoTool(_))), "{refused:?}");
        }
    }

    /// Each manager is asked what its own tool takes, for each thing done to
    /// a service.
    #[test]
    fn each_manager_is_asked_what_its_tool_takes() {
        let file = Path::new("/scratch/definition");
        let systemd = registrar(Manager::Systemd);
        let unit = "x.service";
        assert_eq!(
            systemd.to_load(unit, file),
            [words(["--user", "start", unit])]
        );
        assert_eq!(systemd.to_start(unit), words(["--user", "start", unit]));
        assert_eq!(
            systemd.to_forget(unit),
            [words(["--user", "reset-failed", unit])]
        );
        assert_eq!(
            systemd.to_count(unit),
            words([
                "--user",
                "show",
                "--property=ActiveState",
                "--property=MainPID",
                "--property=NRestarts",
                unit
            ])
        );
        assert_eq!(
            systemd.to_crash(unit, 7),
            (
                PathBuf::from("/usr/bin/systemctl"),
                words(["--user", "kill", "--signal=SIGKILL", unit])
            )
        );

        let launchd = registrar(Manager::Launchd);
        assert_eq!(
            launchd.to_load("x", file),
            [vec![
                OsString::from("bootstrap"),
                OsString::from("gui/501"),
                file.as_os_str().to_owned()
            ]]
        );
        assert_eq!(launchd.to_start("x"), words(["kickstart", "gui/501/x"]));
        assert_eq!(
            launchd.to_stop("x", file),
            [words(["bootout", "gui/501/x"])]
        );
        assert_eq!(launchd.to_forget("x"), Vec::<Vec<OsString>>::new());
        assert_eq!(launchd.to_count("x"), words(["print", "gui/501/x"]));
        assert_eq!(
            launchd.to_crash("x", 7),
            (
                PathBuf::from("/bin/launchctl"),
                words(["kill", "9", "gui/501/x"])
            )
        );

        let task = registrar(Manager::TaskScheduler);
        // **Never `/F`**, which would replace a task already there.
        assert_eq!(
            task.to_load("\\x", file),
            [vec![
                OsString::from("/Create"),
                OsString::from("/TN"),
                OsString::from("\\x"),
                OsString::from("/XML"),
                file.as_os_str().to_owned(),
            ]]
        );
        assert_eq!(task.to_start("\\x"), words(["/Run", "/TN", "\\x"]));
        assert_eq!(
            task.to_stop("\\x", file),
            [
                words(["/End", "/TN", "\\x"]),
                words(["/Delete", "/TN", "\\x", "/F"])
            ]
        );
        assert_eq!(task.to_forget("\\x"), Vec::<Vec<OsString>>::new());
        assert_eq!(
            task.to_count("\\x"),
            words(["/Query", "/TN", "\\x", "/V", "/FO", "CSV"])
        );
        assert_eq!(
            task.to_crash("\\x", 7),
            (
                PathBuf::from("C:\\Windows\\System32\\taskkill.exe"),
                words(["/F", "/PID", "7"])
            )
        );
        for (manager, name) in [
            (Manager::Systemd, "x"),
            (Manager::Launchd, "-x"),
            (Manager::TaskScheduler, "x"),
        ] {
            let refused = registrar(manager).start(name);
            assert!(
                matches!(refused, Err(Error::Described(_))),
                "{manager}: {refused:?}"
            );
            let listed = registrar(manager).registered_under("-");
            assert!(
                matches!(listed, Err(Error::Described(_))),
                "{manager}: {listed:?}"
            );
        }
    }

    /// A unit is linked to start at login without a reload.
    ///
    /// Its links are made and taken away so, in the runtime directory's own links
    /// for a unit kept there; launchd and Task Scheduler are told nothing more,
    /// and logind is asked of the user by its number.
    #[test]
    fn a_unit_is_linked_to_start_at_login_without_a_reload() {
        let systemd = registrar(Manager::Systemd);
        let unit = "x.service";
        let volatile = systemd.volatile_units().expect("systemd's runtime units");
        let persistent = Path::new("/home/u/.config/systemd/user");
        // **Never a reload**, and the runtime directory's own links for a unit
        // kept there.
        assert_eq!(
            systemd.to_enable(unit, persistent),
            [words(["--user", "enable", "--no-reload", unit])]
        );
        assert_eq!(
            systemd.to_enable(unit, &volatile),
            [words([
                "--user",
                "enable",
                "--no-reload",
                "--runtime",
                unit
            ])]
        );
        assert_eq!(
            systemd.to_stop(unit, persistent),
            [
                words(["--user", "disable", "--no-reload", unit]),
                words(["--user", "stop", unit])
            ]
        );
        assert_eq!(
            systemd.to_stop(unit, &volatile),
            [
                words(["--user", "disable", "--no-reload", "--runtime", unit]),
                words(["--user", "stop", unit])
            ]
        );
        assert_eq!(
            systemd.to_ask_lingering(),
            words(["show-user", "501", "--property=Linger", "--value"])
        );
        assert_eq!(systemd.to_linger(), words(["enable-linger", "501"]));
        let file = Path::new("/scratch/definition");
        for manager in [Manager::Launchd, Manager::TaskScheduler] {
            assert_eq!(
                registrar(manager).to_enable("x", file),
                Vec::<Vec<OsString>>::new(),
                "{manager}"
            );
        }
    }

    /// A unit still wanted once its file is gone is read so from systemd's
    /// own answer, and only one kept where a login reads it is reloaded away.
    ///
    /// The answers are the shapes systemd 260 gave for a unit a started unit
    /// wants once its file was removed, and for a name nothing holds; a unit
    /// in the runtime directory, a test's, is never reloaded away, and
    /// launchd and Task Scheduler hold nothing so.
    #[test]
    fn a_unit_still_wanted_is_read_so_and_reloaded_away_only_where_a_login_reads_it() {
        assert!(wanted_once_gone(
            "WantedBy=noctua-dev-test-0123abcd-77-1a-wants.service\nLoadState=loaded\n\
             ActiveState=inactive\nNeedDaemonReload=yes\n"
        ));
        for (answer, what) in [
            (
                "WantedBy=\nLoadState=not-found\nActiveState=inactive\nNeedDaemonReload=yes\n",
                "a name nothing holds",
            ),
            (
                "WantedBy=default.target\nLoadState=not-found\nActiveState=inactive\n\
                 NeedDaemonReload=yes\n",
                "a link left to a unit that is not loaded",
            ),
            (
                "WantedBy=\nLoadState=loaded\nActiveState=inactive\nNeedDaemonReload=yes\n",
                "a unit nothing wants, not yet forgotten",
            ),
            (
                "WantedBy=default.target\nLoadState=loaded\nActiveState=inactive\n\
                 NeedDaemonReload=no\n",
                "a unit whose file is there",
            ),
            (
                "WantedBy=default.target\nLoadState=loaded\nActiveState=active\n\
                 NeedDaemonReload=yes\n",
                "a unit that runs",
            ),
            ("", "no answer"),
        ] {
            assert!(!wanted_once_gone(answer), "{what} was read as still wanted");
        }
        assert_eq!(
            Registrar::to_ask_what_wants("x.service"),
            words([
                "--user",
                "show",
                "--property=LoadState",
                "--property=ActiveState",
                "--property=NeedDaemonReload",
                "--property=WantedBy",
                "x.service"
            ])
        );
        let systemd = registrar(Manager::Systemd);
        let volatile = systemd.volatile_units().expect("systemd's runtime units");
        assert_eq!(
            systemd.to_let_go(Path::new("/home/u/.config/systemd/user")),
            Some(words(["--user", "daemon-reload"]))
        );
        assert_eq!(
            systemd.to_let_go(&volatile),
            None,
            "a unit in the runtime directory, a test's, was reloaded away"
        );
        for manager in [Manager::Launchd, Manager::TaskScheduler] {
            assert_eq!(
                registrar(manager).to_let_go(Path::new("/scratch")),
                None,
                "{manager}"
            );
        }
    }

    /// A registration never replaces a service its manager already holds,
    /// whoever registered it, and says to unregister it first.
    ///
    /// One its manager refuses takes back what it made alone: the unit
    /// systemd's failed start keeps loaded, and under the others its file.
    #[test]
    fn a_registration_never_replaces_a_service_its_manager_holds() {
        assert!(
            Manager::Systemd.a_refused_load_leaves_it_loaded()
                && !Manager::Launchd.a_refused_load_leaves_it_loaded()
                && !Manager::TaskScheduler.a_refused_load_leaves_it_loaded(),
            "a refused bootstrap or create unregistered the name it was refused for"
        );
        assert!(not_held("noctua-dev-test.service", false).is_ok());
        match not_held("noctua-dev-test.service", true) {
            Err(Error::Described(why)) => assert!(
                why.contains("`noctua-dev-test.service` is already registered")
                    && why.contains("unregister it first"),
                "{why}"
            ),
            other => panic!("a name the manager holds was registered over: {other:?}"),
        }
    }

    /// A registration its manager took stands, whether or not the copy left
    /// behind can be removed then.
    ///
    /// That copy is a task's XML, which Task Scheduler no longer reads. One its
    /// manager refused answers that refusal, its file taken back; a plist taken
    /// stays where launchd reads it.
    #[test]
    fn a_registration_its_manager_took_stands_whatever_becomes_of_its_copy() {
        let scratch = Scratch::new("loaded");
        let path = scratch.0.join("definition");
        for manager in [Manager::Launchd, Manager::TaskScheduler] {
            let registrar = registrar(manager);
            let taken = registrar.loaded("x", &scratch.0, "definition", b"bytes", |_| Ok(()));
            assert!(taken.is_ok(), "{manager}: {taken:?}");
            assert_eq!(path.exists(), manager == Manager::Launchd, "{manager}");
            drop(std::fs::remove_file(&path));
            let refused = registrar.loaded("x", &scratch.0, "definition", b"bytes", |_| {
                Err(Error::Refused {
                    asked: "the load".to_owned(),
                    said: "refused".to_owned(),
                })
            });
            assert!(
                matches!(refused, Err(Error::Refused { .. })) && !path.exists(),
                "{manager}: {refused:?}"
            );
        }
        // **A copy that cannot be removed once the task is taken**: here a
        // directory holding a file has taken its name, which no system
        // removes as a file.
        let held = registrar(Manager::TaskScheduler).loaded(
            "x",
            &scratch.0,
            "definition",
            b"bytes",
            |copy| {
                std::fs::remove_file(copy).expect("the copy written");
                crate::create_private_directory(copy).expect("a directory in its place");
                drop(crate::create_private(&copy.join("held")).expect("a file in it"));
                Ok(())
            },
        );
        assert!(
            held.is_ok() && path.is_dir(),
            "a task Task Scheduler took was reported refused, for a copy it could not \
             remove: {held:?}"
        );
    }

    /// What a manager lists is narrowed to the names under the prefix asked.
    ///
    /// **Every service the manager holds is listed**, by launchd every label of
    /// the user's domain and by Task Scheduler every task of the machine, and a
    /// sweep unregisters what this answers: the system's names, another
    /// checkout's and one a shorter prefix shares are all left out.
    #[test]
    fn a_listing_is_narrowed_to_the_names_under_its_prefix() {
        // The shape a hosted macOS runner answered `launchctl print gui/501`
        // in, its `services` block cut to the rows that matter here.
        let domain = "gui/501 = {\n\
                      \ttype = login\n\
                      \tservices = {\n\
                      \t\t       0      -        com.apple.xpc.otherbsd\n\
                      \t\t     561      -        com.apple.Finder\n\
                      \t\t    1413      -        noctua-dev-test-0123abcd-77-1a\n\
                      \t\t       0     78        noctua-dev-test-0123abcd\n\
                      \t\t       0      -        noctua-dev-test-0123abce-77-1a\n\
                      \t\t       0      -        noctua-dev-test-0123abcd-77-1a\n\
                      \t\t    1500      -        noctua-dev-test-0123abcd-80-2b\n\
                      \t\t       0      -        io.github.noctua_world.noctua.daemon.501\n\
                      \t}\n\
                      }\n";
        assert_eq!(
            under("noctua-dev-test-0123abcd-", launchd_labels(domain)),
            [
                "noctua-dev-test-0123abcd-77-1a",
                "noctua-dev-test-0123abcd-80-2b"
            ]
        );
        // The shape `schtasks /Query /FO CSV /NH` answers in on a hosted
        // Windows runner: every task of the machine, under every folder.
        let tasks = "\r\n\
                     \"\\MicrosoftEdgeUpdateTaskMachineCore\",\"11/9/2026 1:00:00 AM\",\"Ready\"\r\n\
                     \"\\noctua-dev-test-0123abcd-77-1a\",\"N/A\",\"Running\"\r\n\
                     \"\\noctua-dev-test-0123abcd\",\"N/A\",\"Ready\"\r\n\
                     \"\\noctua-dev-test-0123abce-77-1a\",\"N/A\",\"Ready\"\r\n\
                     \"\\Microsoft\\Windows\\noctua-dev-test-0123abcd-77-1a\",\"N/A\",\"Ready\"\r\n\
                     \"\\noctua.S-1-5-21-1-2-3-1001\",\"N/A\",\"Ready\"\r\n";
        assert_eq!(
            under("\\noctua-dev-test-0123abcd-", task_names(tasks)),
            ["\\noctua-dev-test-0123abcd-77-1a"]
        );
        assert_eq!(
            under("noctua-dev-test-", vec!["noctua".to_owned()]),
            Vec::<String>::new()
        );
    }

    /// Every scratch service of this checkout has its scratch in the
    /// checkout's own `target`, where every run of it looks: fixed when the
    /// test binary is built, and never by a run's environment.
    #[test]
    fn a_scratch_services_scratch_is_where_every_run_of_its_checkout_looks() {
        assert_eq!(
            checkout().join("crates").join("nc-platform"),
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "the checkout is not the directory this crate sits two levels into"
        );
        assert_eq!(
            scratches().strip_prefix(checkout()).ok(),
            Some(Path::new("target").join("nc-registered").as_path()),
            "a scratch service's scratch is not in its checkout's `target`"
        );
    }

    /// A sweep takes away an old scratch of this checkout whose owner is
    /// dead, its lock let go or never made.
    ///
    /// It keeps one whose owner holds its lock, one too new to judge, and
    /// every other checkout's; an age that cannot be read is too new.
    #[test]
    fn a_sweep_takes_away_only_an_old_scratch_whose_owner_is_dead() {
        let scratch = Scratch::new("sweep");
        let prefix = "noctua-dev-test-0123abcd-";
        let made = |stem: &str, locked: bool| {
            let path = scratch.0.join(stem);
            crate::create_private_directory(&path).expect("a scratch");
            if locked {
                drop(crate::create_private(&path.join(OWNER)).expect("its lock"));
            }
            path
        };
        let dead = made("noctua-dev-test-0123abcd-77-1a", true);
        let never_locked = made("noctua-dev-test-0123abcd-77-2b", false);
        let living = made("noctua-dev-test-0123abcd-78-3c", true);
        let theirs = made("noctua-dev-test-0123abce-77-1a", true);
        let bare = made("noctua-dev-test-0123abcd", true);
        let a_file = scratch.0.join("noctua-dev-test-0123abcd-79-4d");
        drop(crate::create_private(&a_file).expect("a file"));
        let held = std::fs::File::open(living.join(OWNER)).expect("the living owner's lock");
        held.try_lock().expect("a lock nobody holds");
        assert!(
            owner_lives(&living),
            "an owner holding its lock is read as dead"
        );
        assert!(
            !owner_lives(&dead) && !owner_lives(&never_locked),
            "an owner that holds no lock is read as living"
        );
        assert!(too_new_to_judge(&dead, Duration::from_secs(3_600)));
        sweep_scratches(&scratch.0, prefix, Duration::from_secs(3_600));
        for kept in [&dead, &never_locked, &living, &theirs, &bare, &a_file] {
            assert!(
                kept.exists(),
                "`{}` was swept too new to judge",
                kept.display()
            );
        }
        // **Swept again until a bound, each time.** A child another test of
        // this binary starts holds a copy of each descriptor from its fork
        // until its exec, and the sweep's own look at a free lock takes it for
        // a moment — so a sibling's fork in that moment holds a dead owner's
        // lock again, and one sweep reads it as living: measured by reviews, a
        // share of runs failed here when the sweep came once. A scratch the
        // sweep never takes away still fails, at the bound; the shipped sweep
        // is single-shot, and the next run takes away what this one left.
        let swept_until_gone = |gone: &[&PathBuf]| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                sweep_scratches(&scratch.0, prefix, Duration::ZERO);
                if gone.iter().all(|path| !path.exists()) || std::time::Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        swept_until_gone(&[&dead, &never_locked]);
        assert!(
            !dead.exists() && !never_locked.exists(),
            "a dead owner's old scratch stays"
        );
        for kept in [&living, &theirs, &bare, &a_file] {
            assert!(kept.exists(), "`{}` was swept", kept.display());
        }
        drop(held);
        swept_until_gone(&[&living]);
        assert!(
            !living.exists(),
            "the scratch of an owner that let go of its lock stays"
        );
        assert!(theirs.exists() && bare.exists() && a_file.exists());
        assert!(
            too_new(None, Duration::ZERO)
                && too_new(Some(Duration::from_secs(59)), NEW_FOR)
                && !too_new(Some(NEW_FOR), NEW_FOR),
            "a scratch's age is judged against its bound, and one unread is too new"
        );
    }

    /// The tests that register a real service with this system's manager.
    ///
    /// **Skipped by the mutation runner**, which judges the generators and
    /// the readers above through the tests beside them: a run of these per
    /// operator would register a unit with the developer's manager hundreds
    /// of times.
    ///
    /// **A scratch service's program is this test binary**, started again by
    /// the manager with the name of the test that registered it: it records
    /// its process and a nonce of its own in its scratch — in `nc-registered`
    /// under this checkout's `target`, where every run of the checkout looks —
    /// and the arguments it was handed, then waits until its scratch is gone,
    /// or exits 0, as it is told. Every name it is registered under opens with
    /// this checkout's prefix — `noctua-dev-test-`, the checkout's digest and
    /// `-` — and carries this process and a nonce; on systemd its unit is
    /// written only in `$XDG_RUNTIME_DIR/systemd/user`, which the user manager
    /// forgets at the next boot, and under the others its definition stays in
    /// the scratch. A guard unregisters it however the test leaves, and each
    /// test first sweeps this checkout's leftovers whose owner's lock is no
    /// longer held — a dead owner, whose process ended without its guard, and
    /// never a live one, a process of another checkout or a pid since reused.
    mod with_the_manager {
        use std::io::Write as _;
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{Duration, Instant};

        use super::{NEW_FOR, OWNER, owner_lives, prefix, scratches, sweep_scratches};
        use crate::service::{Answer, Counters, MANAGER, Manager, Registrar, Service};

        /// What a scratch service's program is told it is, before its mode.
        const CHILD: &str = "nc-platform-scratch-service=";

        /// Where a scratch service's instances are recorded, one a line.
        const INSTANCES: &str = "instances";

        /// How long a manager has to start, or to restore, a scratch service:
        /// a repetition's minute on Windows, and the slack around it.
        const WITHIN: Duration = Duration::from_secs(150);

        /// How long a service asked to start has to start: well under the
        /// minute between two of Task Scheduler's repetitions.
        ///
        /// **So the instance waited for is the one asked for**: on Windows it is
        /// asked for just after a repetition started one, and the next is a
        /// minute away.
        const ASKED_TO_START_WITHIN: Duration = Duration::from_secs(20);

        /// How long a service that exited 0 is watched staying down: five of
        /// its restorer's delays.
        const LEFT_DOWN_FOR: Duration = Duration::from_secs(5);

        /// How long a manager's tool, asked to load a definition over a service
        /// it holds, has to answer before it is taken to wait on its input.
        const ANSWERS_WITHIN: Duration = Duration::from_secs(30);

        /// The longest a scratch service's program waits for its scratch to
        /// go: past every wait above, so one whose owner died ends anyway.
        const A_CHILD_WAITS_AT_MOST: Duration = Duration::from_secs(300);

        /// What a scratch leaves in the user manager's runtime unit directory
        /// when it is the one that made it: a name systemd reads no unit from.
        ///
        /// **The directory's maker, written in the directory**, so that any
        /// later scratch, of any process, takes it away once nothing but the
        /// mark is left in it — after a run that ended without its guards too
        /// — and none takes away one the developer, or systemd, made.
        const MADE_BY_A_SCRATCH: &str = ".made-by-nc-platform-tests";

        /// Makes the user manager's runtime unit directory `units` where it is
        /// missing, marked as a scratch's.
        fn make_the_units(units: &Path) {
            if units.is_dir() {
                return;
            }
            match crate::create_private_directory(units) {
                Ok(()) => drop(
                    crate::create_private(&units.join(MADE_BY_A_SCRATCH))
                        .expect("a mark in the directory just made"),
                ),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("`{}` could not be made: {error}", units.display()),
            }
        }

        /// Takes the runtime unit directory `units` away if a scratch made it
        /// and nothing but the mark is left in it.
        fn take_the_units_away(units: &Path) {
            let Ok(entries) = std::fs::read_dir(units) else {
                return;
            };
            let names: Vec<std::ffi::OsString> =
                entries.flatten().map(|entry| entry.file_name()).collect();
            if names == [MADE_BY_A_SCRATCH] {
                drop(std::fs::remove_file(units.join(MADE_BY_A_SCRATCH)));
                drop(std::fs::remove_dir(units));
            }
        }

        /// Every argument a scratch service is handed after its scratch: each
        /// character a definition escapes, read back by the program it starts.
        fn carried() -> Vec<String> {
            let mut carried = vec![
                "with space",
                "a \"quoted\" word",
                r"back\slash",
                r"a dir\",
                "it's",
                "$HOME and ${HOME}",
                "&amp; <tag>",
                "naïve 日本 🦉",
                "",
            ];
            if MANAGER.carries_a_percent_sign() {
                carried.extend(["100%", "%h %%"]);
            }
            carried.into_iter().map(str::to_owned).collect()
        }

        /// `stem` as this system's manager names a service.
        fn registered_as(stem: &str) -> String {
            match MANAGER {
                Manager::Systemd => format!("{stem}.service"),
                Manager::Launchd => stem.to_owned(),
                Manager::TaskScheduler => format!("\\{stem}"),
            }
        }

        /// The stem of a name this system's manager knows a service by.
        fn stem_of(name: &str) -> &str {
            name.strip_suffix(".service")
                .unwrap_or(name)
                .trim_start_matches('\\')
        }

        /// A nonce no other scratch of this process carries.
        fn nonce() -> String {
            static MADE: AtomicU64 = AtomicU64::new(0);
            let made = MADE.fetch_add(1, Ordering::Relaxed);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.subsec_nanos());
            format!("{now:x}{made:x}")
        }

        /// What `registrar`'s tool answers to `arguments`, once it has.
        ///
        /// **Within a bound**, past which the tool is taken to wait on its
        /// input, which a registrar hands it nothing on, and the test fails
        /// saying so rather than waiting with it.
        fn answered_within(registrar: &Registrar, arguments: Vec<std::ffi::OsString>) -> Answer {
            let (answered, answer) = std::sync::mpsc::channel();
            let asking = registrar.clone();
            std::thread::spawn(move || drop(answered.send(asking.ask(&arguments))));
            answer
                .recv_timeout(ANSWERS_WITHIN)
                .unwrap_or_else(|_| {
                    panic!(
                        "{} was asked to load a definition over a service it holds and did not \
                         answer within {ANSWERS_WITHIN:?}: it waits on its input",
                        registrar.manager()
                    )
                })
                .expect("the tool was run")
        }

        /// Whether a task's status, where Windows wrote it in English, says what
        /// its last result does: true of every other manager's counters.
        ///
        /// **The one display language a runner has**, against which the
        /// judgement no language changes is measured each time a scratch
        /// service's counters are waited for.
        fn in_english_too(counted: &Counters) -> bool {
            match counted {
                Counters::TaskScheduler { status, .. }
                    if status == "Running" || status == "Ready" =>
                {
                    (status == "Running") == counted.running()
                }
                _ => true,
            }
        }

        /// The mode, the scratch and the carried arguments this binary was
        /// started with, when it is a scratch service's program.
        fn as_a_scratch_service() -> Option<(String, PathBuf, Vec<String>)> {
            let arguments: Vec<String> = std::env::args().collect();
            let at = arguments
                .iter()
                .position(|argument| argument.starts_with(CHILD))?;
            let mode = arguments[at].strip_prefix(CHILD)?.to_owned();
            let scratch = PathBuf::from(arguments.get(at + 1)?);
            Some((mode, scratch, arguments[at + 2..].to_vec()))
        }

        /// What a scratch service's program does: records the arguments it was
        /// handed and then itself, and waits for its scratch to go or exits 0.
        ///
        /// **It never panics**: a panic is an exit its manager would restore,
        /// so a scratch it cannot write in is one it leaves at once, and the
        /// test waiting for it is the one that says so.
        fn be_a_scratch_service(mode: &str, scratch: &Path, carried: &[String]) {
            let pid = std::process::id();
            let Ok(mut handed) = crate::create_private(&scratch.join(format!("arguments-{pid}")))
            else {
                return;
            };
            if handed.write_all(carried.join("\n").as_bytes()).is_err() {
                return;
            }
            drop(handed);
            #[expect(
                clippy::disallowed_methods,
                reason = "a scratch service's record of its instances, appended to in a private \
                          scratch its test made — never a terminal"
            )]
            let opened = std::fs::OpenOptions::new()
                .append(true)
                .open(scratch.join(INSTANCES));
            let Ok(mut instances) = opened else {
                return;
            };
            if instances
                .write_all(format!("{pid} {}\n", nonce()).as_bytes())
                .is_err()
            {
                return;
            }
            drop(instances);
            if mode == "wait" {
                let until = Instant::now() + A_CHILD_WAITS_AT_MOST;
                while scratch.is_dir() && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }

        /// One scratch service, taken away however the test leaves.
        ///
        /// It is its directory, the lock that says its owner lives, and its
        /// registration.
        struct Scratch {
            /// The scratch directory, named for the service.
            root: PathBuf,
            /// The service's name, without what its manager adds to one.
            stem: String,
            /// The lock its owner holds, let go before the directory goes.
            owner: Option<std::fs::File>,
            /// This system's manager, for this user.
            registrar: Option<Registrar>,
            /// The service's name, and the directory its definition is in.
            registered: Option<(String, PathBuf)>,
            /// The service as it was registered.
            service: Option<Service>,
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                if let (Some(registrar), Some((name, directory))) =
                    (&self.registrar, self.registered.take())
                {
                    drop(registrar.unregister(&name, &directory));
                }
                if let Some(units) = self.registrar.as_ref().and_then(Registrar::volatile_units) {
                    take_the_units_away(&units);
                }
                drop(self.owner.take());
                drop(std::fs::remove_dir_all(&self.root));
            }
        }

        impl Scratch {
            /// A scratch for one service of this process, private, its owner's
            /// lock held.
            ///
            /// It is made once this checkout's leftovers whose owner is dead are
            /// swept.
            fn new() -> Self {
                let stem = format!("{}{}-{}", prefix(), std::process::id(), nonce());
                let scratches = scratches();
                std::fs::create_dir_all(&scratches).expect("this checkout's scratches");
                let mut scratch = Self {
                    root: scratches.join(&stem),
                    stem,
                    owner: None,
                    registrar: None,
                    registered: None,
                    service: None,
                };
                crate::create_private_directory(&scratch.root).expect("a scratch directory");
                let owner = crate::create_private(&scratch.root.join(OWNER)).expect("a lock");
                owner.try_lock().expect("a lock nobody else holds");
                scratch.owner = Some(owner);
                drop(crate::create_private(&scratch.root.join(INSTANCES)).expect("a record"));
                let user = crate::user_in(&scratch.root).expect("the scratch is private");
                scratch.registrar =
                    Some(Registrar::for_user(&user).expect("this system's service manager"));
                scratch.sweep();
                scratch
            }

            /// The registrar this scratch registers with.
            fn registrar(&self) -> &Registrar {
                self.registrar.as_ref().expect("made in `new`")
            }

            /// Unregisters every service of this checkout whose owner is dead.
            ///
            /// It removes their scratches too, and every scratch of this checkout
            /// that is no living owner's — an owner being dead once its lock is
            /// held no more.
            fn sweep(&self) {
                let registrar = self.registrar();
                let scratches = scratches();
                let prefix = prefix();
                let listed = match MANAGER {
                    Manager::TaskScheduler => format!("\\{prefix}"),
                    Manager::Systemd | Manager::Launchd => prefix.clone(),
                };
                for name in registrar
                    .registered_under(&listed)
                    .expect("this checkout's services are listed")
                {
                    // **Held here too, before anything is unregistered**: the
                    // listing is every service the manager holds, and only
                    // `registered_under` keeps the others out of it.
                    assert!(
                        name.starts_with(&listed),
                        "`{name}` was answered as one of this checkout's services, under \
                         `{listed}`, and is not: it is never unregistered"
                    );
                    let theirs = scratches.join(stem_of(&name));
                    if owner_lives(&theirs) {
                        continue;
                    }
                    let directory = registrar.volatile_units().unwrap_or_else(|| theirs.clone());
                    registrar
                        .unregister(&name, &directory)
                        .unwrap_or_else(|error| {
                            panic!("`{name}`, a leftover of this checkout, stays: {error}")
                        });
                    drop(std::fs::remove_dir_all(&theirs));
                }
                sweep_scratches(&scratches, &prefix, NEW_FOR);
                if let Some(units) = registrar.volatile_units() {
                    take_the_units_away(&units);
                }
            }

            /// Registers this scratch's service, whose program is this test
            /// binary started again as `test` in `mode`, and answers its name.
            fn register(&mut self, test: &str, mode: &str) -> String {
                let name = registered_as(&self.stem);
                let mut arguments = vec![
                    "--exact".to_owned(),
                    format!("service::tests::with_the_manager::{test}"),
                    format!("{CHILD}{mode}"),
                    self.root.to_str().expect("a scratch in UTF-8").to_owned(),
                ];
                arguments.extend(carried());
                let user = crate::user_in(&self.root).expect("the scratch is private");
                let service = Service::new(
                    &name,
                    "A scratch service of nc-platform's tests, 100% & <its own>",
                    &std::env::current_exe().expect("this test binary"),
                    &arguments,
                    Duration::from_secs(1),
                    &user,
                )
                .expect("a service this manager takes");
                let registrar = self.registrar().clone();
                let directory = registrar
                    .volatile_units()
                    .unwrap_or_else(|| self.root.clone());
                // **Before registering**, so a registration that fails half
                // way is taken away by the guard too.
                self.registered = Some((name.clone(), directory.clone()));
                make_the_units(&directory);
                // **Once more if the directory went in between**: another
                // scratch takes it away once only the mark is left, which may
                // be the moment after this one found it there.
                let registered = registrar.register(&service, &directory).or_else(|error| {
                    if matches!(&error, super::super::Error::Io(failed)
                        if failed.error().kind() == std::io::ErrorKind::NotFound)
                    {
                        make_the_units(&directory);
                        registrar.register(&service, &directory)
                    } else {
                        Err(error)
                    }
                });
                registered.unwrap_or_else(|error| panic!("`{name}` was not registered: {error}"));
                self.service = Some(service);
                name
            }

            /// Registers this scratch's service again, from a directory of its
            /// own, and holds the registrar to refusing it.
            ///
            /// The manager, asked to load that definition over the service it
            /// holds, refuses too, and the instance `running` is still the
            /// one that runs.
            fn is_never_registered_over(&self, name: &str, running: u32) {
                let registrar = self.registrar();
                let service = self.service.as_ref().expect("registered");
                let elsewhere = self.root.join("again");
                crate::create_private_directory(&elsewhere).expect("a directory of its own");
                let instances = self.instances().len();
                let refused = registrar.register(service, &elsewhere);
                assert!(
                    matches!(&refused, Err(super::super::Error::Described(why))
                        if why.contains("already registered")),
                    "`{name}` was registered over: {refused:?}. {}",
                    self.what_the_manager_says(name)
                );
                // **And the manager's own load, asked past that refusal**, as a
                // registration racing this one would ask it: launchd's
                // bootstrap and Task Scheduler's create, given no `/F`, each
                // refuse a name they hold. systemd's `start` reads no file, and
                // of a unit that runs starts nothing.
                if MANAGER != Manager::Systemd {
                    let (file, bytes) =
                        super::super::definition(MANAGER, service).expect("a definition");
                    let path = elsewhere.join(file);
                    super::super::write_new(&path, &bytes).expect("written");
                    for arguments in registrar.to_load(name, &path) {
                        let answer = answered_within(registrar, arguments);
                        assert!(
                            !answer.succeeded,
                            "`{}` loaded a definition over `{name}`, which it held: {} {} {}",
                            answer.asked, answer.status, answer.out, answer.err
                        );
                    }
                }
                assert!(
                    registrar.is_registered(name).expect("asked"),
                    "`{name}` was taken from its manager by a registration refused over it"
                );
                let counted = registrar.counters(name).expect("counted");
                assert!(
                    counted.running() && counted.pid().is_none_or(|pid| pid == running),
                    "`{name}` runs another instance than {running} after a registration \
                     refused over it: {counted:?}"
                );
                assert_eq!(
                    self.instances().len(),
                    instances,
                    "`{name}` was started again by a registration refused over it"
                );
            }

            /// Unregisters this scratch's service, which its manager must
            /// then not know.
            fn unregister(&mut self) {
                let (name, directory) = self.registered.take().expect("registered");
                let registrar = self.registrar();
                registrar
                    .unregister(&name, &directory)
                    .unwrap_or_else(|error| panic!("`{name}` was not unregistered: {error}"));
                assert!(
                    !registrar.is_registered(&name).expect("asked"),
                    "`{name}` was unregistered and its manager still knows it"
                );
                let definition = directory.join(MANAGER.file_for(&name));
                assert!(
                    !definition.exists(),
                    "`{name}` was unregistered and its definition `{}` stays",
                    definition.display()
                );
            }

            /// Every instance of this scratch's service so far: its process and
            /// its nonce.
            fn instances(&self) -> Vec<(u32, String)> {
                std::fs::read_to_string(self.root.join(INSTANCES))
                    .unwrap_or_default()
                    .lines()
                    .filter_map(|line| {
                        let (pid, nonce) = line.split_once(' ')?;
                        Some((pid.parse().ok()?, nonce.to_owned()))
                    })
                    .collect()
            }

            /// The arguments the instance `pid` was handed after its scratch.
            fn carried_by(&self, pid: u32) -> Vec<String> {
                std::fs::read_to_string(self.root.join(format!("arguments-{pid}")))
                    .expect("an instance records its arguments")
                    .split('\n')
                    .map(str::to_owned)
                    .collect()
            }

            /// The first `count` instances of the service named `name`, once
            /// its manager has started as many, `within` the bound given.
            fn wait_for(&self, count: usize, name: &str, within: Duration) -> Vec<(u32, String)> {
                let until = Instant::now() + within;
                loop {
                    let found = self.instances();
                    if found.len() >= count {
                        return found;
                    }
                    assert!(
                        Instant::now() < until,
                        "{count} instance(s) of `{name}` were waited for {within:?} and {} \
                         came. {}",
                        found.len(),
                        self.what_the_manager_says(name)
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
            }

            /// The manager's counters of `name`, once `holds` is true of them.
            fn counted_once(
                &self,
                name: &str,
                holds: impl Fn(&Counters) -> bool,
                what: &str,
            ) -> Counters {
                let until = Instant::now() + WITHIN;
                loop {
                    let counted = self
                        .registrar()
                        .counters(name)
                        .unwrap_or_else(|error| panic!("`{name}` was not counted: {error}"));
                    if holds(&counted) && in_english_too(&counted) {
                        return counted;
                    }
                    assert!(
                        Instant::now() < until,
                        "`{name}` was waited for {WITHIN:?} to be {what}, a status in English \
                         agreeing, and is {counted:?}. {}",
                        self.what_the_manager_says(name)
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
            }

            /// Everything the manager answers about `name`, verbatim, for a
            /// failure to show: its counters as asked, and the listing the
            /// registrar reads whether it is registered from.
            fn what_the_manager_says(&self, name: &str) -> String {
                let registrar = self.registrar();
                let raw = |arguments: &[std::ffi::OsString]| match registrar.ask(arguments) {
                    Ok(answer) => format!(
                        "`{}` answered {}:\n{}\n{}",
                        answer.asked,
                        answer.status,
                        answer.out.chars().take(6_000).collect::<String>(),
                        answer.err.chars().take(2_000).collect::<String>()
                    ),
                    Err(error) => error.to_string(),
                };
                let listing = match MANAGER {
                    Manager::Systemd => raw(&super::super::words([
                        "--user",
                        "show",
                        "--property=LoadState",
                        "--property=ActiveState",
                        "--property=SubState",
                        "--property=Result",
                        "--property=NRestarts",
                        "--property=MainPID",
                        "--property=FragmentPath",
                        name,
                    ])),
                    Manager::Launchd => raw(&super::super::words(["print", &registrar.domain()])),
                    Manager::TaskScheduler => raw(&super::super::words([
                        "/Query", "/TN", name, "/V", "/FO", "LIST",
                    ])),
                };
                format!(
                    "The instances so far: {:?}. {}\n{listing}",
                    self.instances(),
                    raw(&registrar.to_count(name))
                )
            }
        }

        /// A scratch service killed is restored by its manager as a new
        /// instance, and once unregistered is gone from the manager.
        ///
        /// The restoration is read through the manager's own counter as well as
        /// the instance's record of itself, and the instance was handed the
        /// arguments it was registered with, character for character. A second
        /// registration under its name is refused, and leaves it running. On
        /// Windows it is killed past a repetition refused while it ran, and
        /// read as down until the next one.
        #[test]
        fn a_service_killed_is_restored_as_a_new_instance_and_is_gone_once_unregistered() {
            if let Some((mode, scratch, carried)) = as_a_scratch_service() {
                be_a_scratch_service(&mode, &scratch, &carried);
                return;
            }
            let mut scratch = Scratch::new();
            let name = scratch.register(
                "a_service_killed_is_restored_as_a_new_instance_and_is_gone_once_unregistered",
                "wait",
            );
            let registrar = scratch.registrar().clone();
            assert!(
                registrar.is_registered(&name).expect("asked"),
                "`{name}` was registered and its manager does not know it. {}",
                scratch.what_the_manager_says(&name)
            );
            let first = scratch.wait_for(1, &name, WITHIN)[0].clone();
            assert_eq!(
                scratch.carried_by(first.0),
                carried(),
                "`{name}` was started with arguments other than the ones it was registered with"
            );
            let before = scratch.counted_once(&name, Counters::running, "running");
            if let Some(pid) = before.pid() {
                assert_eq!(
                    pid, first.0,
                    "its manager runs another process than the instance"
                );
            }
            scratch.is_never_registered_over(&name, first.0);
            // **On Windows, past a repetition refused while it runs**, so the
            // kill is read after the one last result that says nothing of how
            // an instance ends, and the instance's end writes its own.
            let refusing = MANAGER == Manager::TaskScheduler;
            if refusing {
                scratch.counted_once(
                    &name,
                    |now| {
                        matches!(now, Counters::TaskScheduler { last_result, .. }
                            if *last_result == super::super::REFUSED_WHILE_RUNNING)
                    },
                    "past a repetition refused while it ran",
                );
                assert_eq!(
                    scratch.instances().len(),
                    1,
                    "`{name}` was started again while it ran. {}",
                    scratch.what_the_manager_says(&name)
                );
            }
            registrar
                .crash(&name, first.0)
                .unwrap_or_else(|error| panic!("`{name}` was not killed: {error}"));
            if refusing {
                scratch.counted_once(&name, |now| !now.running(), "down once it was killed");
            }
            let second = scratch.wait_for(2, &name, WITHIN)[1].clone();
            assert_ne!(
                second.1, first.1,
                "the instance after the kill is the one killed"
            );
            let restored = scratch.counted_once(
                &name,
                |now| now.ran_since(&before) && now.running(),
                "restored by its manager",
            );
            if let Some(pid) = restored.pid() {
                assert_eq!(
                    pid, second.0,
                    "its manager restored another process than the instance"
                );
            }
            scratch.unregister();
        }

        /// A scratch service that exits 0 is left down by a restorer that
        /// reads how a program ended.
        ///
        /// Task Scheduler's, which reads nothing of it, starts it again; and it
        /// is started again when it is asked to be, and once unregistered is
        /// gone from the manager.
        #[test]
        fn a_service_that_exits_cleanly_is_left_down_unless_its_restorer_reads_no_exit() {
            if let Some((mode, scratch, carried)) = as_a_scratch_service() {
                be_a_scratch_service(&mode, &scratch, &carried);
                return;
            }
            let mut scratch = Scratch::new();
            let name = scratch.register(
                "a_service_that_exits_cleanly_is_left_down_unless_its_restorer_reads_no_exit",
                "exit",
            );
            let registrar = scratch.registrar().clone();
            scratch.wait_for(1, &name, WITHIN);
            let after = scratch.counted_once(&name, |now| !now.running(), "down after it exited 0");
            if MANAGER.restarts_a_clean_exit() {
                scratch.wait_for(2, &name, WITHIN);
                scratch.counted_once(
                    &name,
                    |now| now.ran_since(&after),
                    "started again by its repetition",
                );
            } else {
                let until = Instant::now() + LEFT_DOWN_FOR;
                while Instant::now() < until {
                    assert_eq!(
                        scratch.instances().len(),
                        1,
                        "`{name}` exited 0 and was started again. {}",
                        scratch.what_the_manager_says(&name)
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
                let now = registrar.counters(&name).expect("counted");
                assert!(
                    !now.ran_since(&after) && !now.running(),
                    "`{name}` exited 0 and its manager started it again: {now:?} after {after:?}"
                );
            }
            // **Within a bound a repetition cannot meet**, so on Windows the
            // instance waited for is the one `/Run` started.
            let started = scratch.instances().len();
            registrar
                .start(&name)
                .unwrap_or_else(|error| panic!("`{name}` was not started: {error}"));
            scratch.wait_for(started + 1, &name, ASKED_TO_START_WITHIN);
            scratch.unregister();
        }

        /// A unit that wants a scratch service and stays active, as a user
        /// manager's `default.target` does once it has read an enabled unit's
        /// link as it started.
        ///
        /// Its own unit in the runtime directory, under this checkout's prefix,
        /// with a scratch of its own whose lock says its owner lives, as a
        /// sweep reads one; unregistered however the test leaves.
        struct Holder {
            /// This system's manager, for this user.
            registrar: Registrar,
            /// The holder's name, until it is let go.
            name: Option<String>,
            /// The runtime directory its unit is in.
            directory: PathBuf,
            /// Its scratch, named for it, holding its owner's lock.
            root: PathBuf,
            /// The lock, let go before the scratch goes.
            owner: Option<std::fs::File>,
        }

        impl Drop for Holder {
            fn drop(&mut self) {
                if let Some(name) = self.name.take() {
                    drop(self.registrar.unregister(&name, &self.directory));
                }
                drop(self.owner.take());
                drop(std::fs::remove_dir_all(&self.root));
            }
        }

        impl Holder {
            /// A unit wanting the service `wanted` of `scratch`, started: one
            /// shot of this test binary that lists no test, and remains active.
            fn wanting(scratch: &Scratch, wanted: &str) -> Self {
                let registrar = scratch.registrar().clone();
                let directory = registrar.volatile_units().expect("systemd's runtime units");
                let stem = format!("{}-wants", scratch.stem);
                let name = registered_as(&stem);
                // **Its owner's lock before its unit**, so a sweep another
                // test of this process runs meanwhile reads it as living.
                let root = scratches().join(&stem);
                crate::create_private_directory(&root).expect("the holder's scratch");
                let owner = crate::create_private(&root.join(OWNER)).expect("a lock");
                owner.try_lock().expect("a lock nobody else holds");
                let program = std::env::current_exe().expect("this test binary");
                let unit = format!(
                    "[Unit]\n\
                     Description=A scratch unit of nc-platform's tests that wants another\n\
                     Wants={wanted}\n\
                     \n\
                     [Service]\n\
                     Type=oneshot\n\
                     RemainAfterExit=yes\n\
                     ExecStart={} \"--exact\" \"no::such::test\"\n\
                     StandardOutput=null\n\
                     StandardError=null\n",
                    super::super::systemd_word(program.to_str().expect("a path in UTF-8"))
                );
                let holder = Self {
                    registrar,
                    name: Some(name.clone()),
                    directory,
                    root,
                    owner: Some(owner),
                };
                super::super::write_new(&holder.directory.join(&name), unit.as_bytes())
                    .expect("the holder's unit written");
                holder
                    .registrar
                    .start(&name)
                    .unwrap_or_else(|error| panic!("`{name}` was not started: {error}"));
                holder
            }

            /// The holder unregistered, which its manager must then not know.
            fn let_go(mut self) {
                let name = self.name.take().expect("held");
                self.registrar
                    .unregister(&name, &self.directory)
                    .unwrap_or_else(|error| panic!("`{name}` was not unregistered: {error}"));
            }
        }

        /// A unit something its manager runs still wants is said to be once it
        /// is unregistered, and goes with what wants it; elsewhere a service is
        /// its registration alone, and goes with it.
        ///
        /// **What a login makes of a real install's unit, made here without
        /// one**: the user's manager reads `default.target`'s links as it
        /// starts, and from then on wants an enabled unit as the holder here
        /// wants this one. A test reloads nothing, so the unregistration of a
        /// unit in the runtime directory leaves it stopped, its file gone, and
        /// says what still holds it.
        #[test]
        fn a_unit_still_wanted_once_unregistered_is_said_to_be_and_goes_with_what_wants_it() {
            if let Some((mode, scratch, carried)) = as_a_scratch_service() {
                be_a_scratch_service(&mode, &scratch, &carried);
                return;
            }
            let mut scratch = Scratch::new();
            let name = scratch.register(
                "a_unit_still_wanted_once_unregistered_is_said_to_be_and_goes_with_what_wants_it",
                "exit",
            );
            scratch.wait_for(1, &name, WITHIN);
            let holder = (MANAGER == Manager::Systemd).then(|| Holder::wanting(&scratch, &name));
            let (_, directory) = scratch.registered.clone().expect("registered");
            let registrar = scratch.registrar().clone();
            let unregistered = registrar.unregister(&name, &directory);
            let Some(holder) = holder else {
                unregistered.unwrap_or_else(|error| panic!("`{name}` stays: {error}"));
                scratch.registered = None;
                assert!(!registrar.is_registered(&name).expect("asked"));
                return;
            };
            assert!(
                matches!(&unregistered, Err(super::super::Error::StillWanted(said)) if *said == name),
                "`{name}`, which a running unit wants, was unregistered as {unregistered:?}. {}",
                scratch.what_the_manager_says(&name)
            );
            assert!(
                !directory.join(MANAGER.file_for(&name)).exists(),
                "`{name}` is said to be still wanted and its definition stays"
            );
            assert!(
                registrar.is_registered(&name).expect("asked")
                    && !registrar.counters(&name).expect("counted").running(),
                "`{name}`, still wanted, is not held stopped by its manager. {}",
                scratch.what_the_manager_says(&name)
            );
            holder.let_go();
            let until = Instant::now() + WITHIN;
            while registrar.is_registered(&name).expect("asked") {
                assert!(
                    Instant::now() < until,
                    "`{name}` outlived the unit that wanted it by {WITHIN:?}. {}",
                    scratch.what_the_manager_says(&name)
                );
                std::thread::sleep(Duration::from_millis(100));
            }
            scratch.registered = None;
        }
    }
}
