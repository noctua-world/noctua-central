//! The installer (`NOCTUA.md §17.1`): an install written where its manifest
//! says, from binaries a build left behind, which `cargo xtask dev --install`
//! runs for a development checkout and the suites run under a scratch root.
//!
//! # What an install is made of, in order
//!
//! 1. **The base directories**, each made where it is missing — one level at
//!    a time, private — as the XDG specification asks of a program that
//!    writes under one; one that is there is the user's and is left as it is.
//! 2. **The profile's state directory and its `bin`**, made private or judged
//!    so; a daemon an earlier install left running is stopped first, through
//!    the CLI's own [`crate::control`], so no binary is replaced under it.
//! 3. **The binaries, copied out of `target/`** — never linked to it — under
//!    the names the layout gives them, each written private beside its name,
//!    synced and renamed over it: a daemon run from `target/` would hold its
//!    file, so a build could not replace it on Windows, and an install pointed
//!    into a checkout's `target/` would run whatever the next build left there.
//! 4. **The manifest**, with the resolved bases, written the same way, and the
//!    install read back through `nc_channel::found` — judged as every binary
//!    of it will read it.
//! 5. **The device and the world**: the copied daemon's `--initialize`,
//!    handed the world, makes the profile private, draws the device's seed
//!    when the profile has none — never again — and copies the world in.
//! 6. **The plugins**, each its manifest and its component, written into its
//!    own private directory under the profile's plugins.
//! 7. **The restorer**, where the plan asks for one (`cargo xtask dev
//!    --install` does): the daemon registered with this system's service
//!    manager as `noctua-daemon --restored`, under the name the layout gives
//!    it there, to be started again when it dies and at each login
//!    (`NOCTUA.md §17.3`). A restorer an earlier install registered is
//!    unregistered before any binary is replaced, so none starts one half
//!    written. Then the daemon is started through it — **unless the profile
//!    carried the `stopped` mark before this install stopped an earlier
//!    daemon**: that is a person's stop, which an install keeps.
//!
//! **Where the restorer's definition is kept is the install's**: a real
//! install's where its manager reads it at each login
//! (`nc_platform::service::Manager::loaded_at_login`, under the bases the
//! manifest holds); a relocated one's — a test's — in systemd's runtime
//! directory, which the next boot forgets, or in its own profile, from which
//! launchd loads it for this login alone, while Task Scheduler keeps a test's
//! task as its own and starts it at each logon until it is unregistered.
//! Nothing is added to `PATH`: the CLI is reached at its path in the install,
//! which the installer prints. **An uninstall reaches the manager first** —
//! from an environment that does not, it stops and removes nothing, since
//! that manager may hold the restorer — then stops the daemon and unregisters
//! its restorer.
//!
//! # What this does not answer
//!
//! A process running as the same user, which can replace any file of an
//! install between its writing and its use, or rewrite or remove the
//! restorer's definition.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use core::time::Duration;

use nc_canon::Value;
use nc_channel::found::Found;
use nc_channel::{Install, Layout};
use nc_platform::service::{Manager, Registrar, Service};
use nc_platform::{Judged, Kind, User};

/// The realm a development install's world decides in.
pub const REALM: &str = "home";

/// The person a development install's world decides as.
pub const PERSON: &str = "ana";

/// The grant a development install's world gives its person.
///
/// **`g.by_policy`, the membership the family pack confers**: it has begun
/// and has no end, so a daemon deciding on the system's clock holds it on
/// every day it runs.
pub const GRANT: &str = "g.by_policy";

/// The epoch a development install's world gives its realm.
pub const EPOCH: i64 = 7;

/// How long a restorer waits before it starts a daemon that died again.
///
/// **One second, the least launchd's throttle counts**, so a crash is
/// restored at once; a daemon that dies at each start is then cut off by
/// systemd's own start limit within seconds, and started no more until it is
/// started on purpose. Task Scheduler's repetition, every minute, is its own.
pub const RESTORED_AFTER: Duration = Duration::from_secs(1);

/// The variable an installer reads a namespace from: an install relocated
/// under one is a test's, and every name it gives the system is its own
/// (`nc_channel::Namespace`).
pub const NAMESPACE: &str = "NC_INSTALL_NAMESPACE";

/// The development install of the checkout at `checkout`, from the
/// environment `variable` reads: the base directories this system keeps a
/// user's profiles under (`nc_platform::base_directories`), and the namespace
/// [`NAMESPACE`] names, when it names one.
///
/// **`log_endpoint` is a real install's alone**: one relocated under a
/// namespace is a test's and has no stack, so it names no central log and
/// its daemon writes to its standard error.
///
/// # Errors
///
/// [`Unmade`] for a base directory the environment does not name, or names
/// as no plain absolute path, and for a namespace `nc_channel` refuses.
pub fn development(
    checkout: &Path,
    variable: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    log_endpoint: Option<String>,
) -> Result<Install, Unmade> {
    let found = nc_platform::base_directories(variable)
        .map_err(|error| Unmade(format!("the base directories were not read: {error}")))?;
    let bases = nc_channel::Bases::new(found.data, found.config, found.cache, found.state)
        .map_err(|refused| Unmade(format!("the base directories were refused: {refused}")))?;
    let namespace = match variable(NAMESPACE) {
        None => None,
        Some(value) => Some(
            value
                .to_str()
                .ok_or_else(|| Unmade(format!("`{NAMESPACE}` is not UTF-8")))
                .and_then(|text| {
                    nc_channel::Namespace::parse(text)
                        .map_err(|refused| Unmade(format!("`{NAMESPACE}` was refused: {refused}")))
                })?,
        ),
    };
    Ok(Install {
        channel: nc_channel::Channel::Development(nc_channel::Checkout::of(checkout)),
        bases,
        log_endpoint: log_endpoint.filter(|_| namespace.is_none()),
        namespace,
    })
}

/// What an install is made from.
#[derive(Debug, Clone)]
pub struct Plan<'a> {
    /// What the manifest says.
    pub install: Install,
    /// The CLI's executable, as a build left it.
    pub cli: &'a Path,
    /// The daemon's executable, as a build left it.
    pub daemon: &'a Path,
    /// The world file's text, copied into the profile.
    pub world: &'a str,
    /// The plugins installed in the profile.
    pub plugins: Vec<Plugin<'a>>,
    /// Whether the daemon is registered with the system's service manager, which
    /// restores it when it dies and starts it at each login.
    pub restorer: bool,
}

/// The plan of a development install, with its restorer.
///
/// **The one `cargo xtask dev --install` makes**, and the suite that proves
/// the restorer installs: `install` made from the binaries `cli` and `daemon`
/// a build left, the world `world` and the `plugins`, its daemon registered
/// with this system's service manager.
#[must_use]
pub const fn development_plan<'a>(
    install: Install,
    cli: &'a Path,
    daemon: &'a Path,
    world: &'a str,
    plugins: Vec<Plugin<'a>>,
) -> Plan<'a> {
    Plan {
        install,
        cli,
        daemon,
        world,
        plugins,
        restorer: true,
    }
}

/// A plugin a build left, installed under `name`.
#[derive(Debug, Clone)]
pub struct Plugin<'a> {
    /// The directory it is installed in, under the profile's plugins.
    pub name: &'a str,
    /// Its manifest.
    pub manifest: &'a Path,
    /// Its component.
    pub component: &'a Path,
}

/// An install made, read back as its binaries read it.
#[derive(Debug)]
pub struct Installed {
    /// The install, judged and read beside its CLI.
    pub found: Found,
    /// The CLI's executable, in the install.
    pub cli: PathBuf,
    /// What an earlier install's daemon was told, when one was running.
    pub stopped: Option<String>,
    /// The restorer registered, when the plan asked for one.
    pub registered: Option<Registered>,
}

/// A daemon registered with this system's service manager, which restores it.
#[derive(Debug, Clone)]
pub struct Registered {
    /// The manager.
    pub manager: Manager,
    /// The name the manager knows it by.
    pub name: String,
    /// The directory its definition is kept in.
    pub kept_in: PathBuf,
    /// What its start said, if it was started.
    ///
    /// Nothing when the profile carried the `stopped` mark and the daemon was
    /// left down.
    pub started: Option<String>,
}

/// An install removed.
#[derive(Debug)]
pub struct Uninstalled {
    /// Every name it gave the system.
    pub layout: Layout,
    /// The restorer unregistered, by the name its manager knew it by; nothing
    /// when none was registered.
    pub unregistered: Option<String>,
}

/// Why an install was not made, or not removed.
#[derive(Debug)]
pub struct Unmade(String);

impl core::fmt::Display for Unmade {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for Unmade {}

/// `doing` `path`, which the system answered with `error`.
fn unmade(doing: &str, path: &Path, error: &impl core::fmt::Display) -> Unmade {
    Unmade(format!(
        "`{}` could not be {doing}: {error}",
        path.display()
    ))
}

/// A path judged and not taken.
fn not_taken(judged: &Judged) -> Unmade {
    match judged {
        Judged::Refused(refused) => Unmade(format!(
            "{refused}, so it was left as it was and nothing was installed there"
        )),
        Judged::Failed(failed) => Unmade(failed.to_string()),
    }
}

/// The world a development install's profile decides with.
///
/// It is the test world `world` holds, with its realm [`REALM`], its person
/// [`PERSON`], the grant [`GRANT`] and the epoch [`EPOCH`], as
/// the daemon's `nc_authz::LocalWorld` reads one.
///
/// # Errors
///
/// [`Unmade`] for a world that is not JSON `nc_canon` reads.
pub fn development_world(world: &str) -> Result<String, Unmade> {
    let world = nc_canon::parse(world)
        .map_err(|error| Unmade(format!("the world is not JSON this reads: {error}")))?;
    Ok(Value::object([
        ("epoch", Value::Integer(EPOCH)),
        ("grant", Value::text(GRANT)),
        ("person", Value::text(PERSON)),
        ("realm", Value::text(REALM)),
        ("world", world),
    ])
    .to_canonical())
}

/// Makes the install `plan` describes, as this module's header says.
///
/// # Errors
///
/// [`Unmade`], saying which step did not finish: a path not this user's
/// alone, one that could not be made or written, an earlier daemon that did
/// not stop, or a daemon that refused to initialize the profile.
pub fn install(plan: &Plan<'_>) -> Result<Installed, Unmade> {
    install_with(plan, initialized)
}

/// What [`install`] does, with the profile initialized by `initialize`,
/// handed the copied daemon and the world file.
fn install_with(
    plan: &Plan<'_>,
    initialize: impl FnOnce(&Path, &Path) -> Result<(), Unmade>,
) -> Result<Installed, Unmade> {
    install_restored_by(plan, initialize, Restorer::of)
}

/// What [`install_with`] does, with the restorer — where the plan asks for
/// one — the one `restorer_of` answers for the install, its layout and its
/// user.
fn install_restored_by<R: Restoring>(
    plan: &Plan<'_>,
    initialize: impl FnOnce(&Path, &Path) -> Result<(), Unmade>,
    restorer_of: impl FnOnce(&Install, &Layout, &User) -> Result<R, Unmade>,
) -> Result<Installed, Unmade> {
    for base in [
        plan.install.bases.data(),
        plan.install.bases.config(),
        plan.install.bases.cache(),
        plan.install.bases.state(),
    ] {
        made_with_its_parents(base)?;
    }
    // The paths an install derives do not depend on its user, which is read
    // off the state directory once it is there.
    let state = nc_channel::layout(&plan.install, "").state;
    directory(&state, None)?;
    let user = nc_platform::user_in(&state).map_err(|judged| not_taken(&judged))?;
    let layout = nc_channel::layout(&plan.install, &user.to_string());
    directory(&layout.install, Some(&user))?;
    let cli = layout
        .install
        .join(format!("{}{}", layout.cli, nc_platform::EXECUTABLE_SUFFIX));
    let daemon = layout.install.join(format!(
        "{}{}",
        layout.daemon,
        nc_platform::EXECUTABLE_SUFFIX
    ));
    // Read before an earlier daemon is stopped, whose stop writes the mark.
    let marked = plan.restorer && marked(&layout, &user)?;
    let stopped = stopped_before(&cli)?;
    let restorer = if plan.restorer {
        let restorer = restorer_of(&plan.install, &layout, &user)?;
        restorer.unregistered()?;
        Some(restorer)
    } else {
        None
    };
    replaced(&cli, &read_whole(plan.cli)?, &user, true)?;
    replaced(&daemon, &read_whole(plan.daemon)?, &user, true)?;
    replaced(
        &layout.manifest,
        plan.install.to_canonical().as_bytes(),
        &user,
        false,
    )?;
    let found = nc_channel::found::beside(&cli)
        .map_err(|refused| Unmade(format!("the install written does not read back: {refused}")))?;
    let world = layout.state.join("world.json.install");
    replaced(&world, plan.world.as_bytes(), &user, false)?;
    let initialized = initialize(&daemon, &world);
    let removed = std::fs::remove_file(&world);
    initialized?;
    removed.map_err(|error| unmade("removed", &world, &error))?;
    for plugin in &plan.plugins {
        let directory_of = layout.plugins.join(plugin.name);
        directory(&directory_of, Some(&user))?;
        replaced(
            &directory_of.join(nc_channel::PLUGIN_MANIFEST),
            &read_whole(plugin.manifest)?,
            &user,
            false,
        )?;
        replaced(
            &directory_of.join(nc_channel::PLUGIN_COMPONENT),
            &read_whole(plugin.component)?,
            &user,
            false,
        )?;
    }
    let registered = match restorer {
        Some(restorer) => Some(restorer.registered(&daemon, &found, marked)?),
        None => None,
    };
    Ok(Installed {
        found,
        cli,
        stopped,
        registered,
    })
}

/// The name `manager` knows the daemon of `layout` by: its unit, the label
/// of its agent, or its task.
#[must_use]
pub fn restorer_name(layout: &Layout, manager: Manager) -> &str {
    match manager {
        Manager::Systemd => &layout.systemd_unit,
        Manager::Launchd => &layout.launchd_label,
        Manager::TaskScheduler => &layout.task,
    }
}

/// Where `manager` keeps the definition of the restorer of `install`, laid
/// out as `layout`; `volatile` is where systemd reads a unit it forgets at the
/// next boot.
///
/// **A real install's is read at each login**: under the configuration base
/// its manifest holds, where its manager reads it
/// (`nc_platform::service::Manager::loaded_at_login`), or in its own profile
/// for Task Scheduler, whose copy is its own once the task is registered.
/// **A relocated install's — a test's — is kept where no login reads it** on
/// systemd and launchd: in systemd's runtime directory, which the next boot
/// forgets, and in its own profile, from which launchd loads it for this
/// login alone. Under Task Scheduler it is in its own profile too, but the task
/// is the manager's whatever became of the file, and starts at each logon by
/// its trigger until it is unregistered.
#[must_use]
pub fn kept_in(
    install: &Install,
    layout: &Layout,
    manager: Manager,
    volatile: Option<&Path>,
) -> Option<PathBuf> {
    match (&install.namespace, manager) {
        (Some(_), Manager::Systemd) => volatile.map(Path::to_path_buf),
        (Some(_), Manager::Launchd | Manager::TaskScheduler) => Some(layout.state.clone()),
        (None, _) => manager
            .loaded_at_login(install.bases.config())
            .or_else(|| Some(layout.state.clone())),
    }
}

/// Whether an install starts its daemon once its restorer is registered.
///
/// Unless the profile carried the `stopped` mark before the install, which is
/// a person's stop and an install keeps.
const fn started_once_registered(marked_before: bool) -> bool {
    !marked_before
}

/// Whether the profile of `layout` carries the `stopped` mark, the mark
/// judged this user's alone first.
fn marked(layout: &Layout, user: &User) -> Result<bool, Unmade> {
    match nc_platform::judge(&layout.stopped, Kind::File, user) {
        Ok(()) => Ok(true),
        Err(Judged::Failed(absent)) if absent.error().kind() == std::io::ErrorKind::NotFound => {
            Ok(false)
        }
        Err(judged) => Err(not_taken(&judged)),
    }
}

/// What an install asks of the restorer it registers, in the order this
/// module's header gives.
///
/// The one an earlier install registered is let go before any binary is
/// replaced, and the daemon is registered once everything else is written.
trait Restoring: Sized {
    /// The restorer an earlier install registered, unregistered if its
    /// manager holds it: what it was known by, or nothing.
    fn unregistered(&self) -> Result<Option<String>, Unmade>;

    /// The daemon at `daemon`, of `found`'s install, registered `--restored`
    /// and started, unless the profile was `marked` stopped before the
    /// install.
    fn registered(self, daemon: &Path, found: &Found, marked: bool) -> Result<Registered, Unmade>;
}

/// This system's service manager as an install's restorer is registered with.
///
/// The manager, the name it knows the daemon by, and where its definition is
/// kept.
struct Restorer {
    /// The manager, for the install's user.
    registrar: Registrar,
    /// The name it knows the daemon by.
    name: String,
    /// Where the definition is kept.
    kept_in: PathBuf,
}

impl Restorer {
    /// The restorer of `install`, laid out as `layout`, for `user`.
    fn of(install: &Install, layout: &Layout, user: &User) -> Result<Self, Unmade> {
        let registrar = Registrar::for_user(user).map_err(|error| {
            Unmade(format!(
                "this system's service manager could not be found: {error}"
            ))
        })?;
        let manager = registrar.manager();
        let kept_in = kept_in(
            install,
            layout,
            manager,
            registrar.volatile_units().as_deref(),
        )
        .ok_or_else(|| {
            Unmade(format!(
                "{manager} names no directory a definition of the daemon is read from"
            ))
        })?;
        Ok(Self {
            name: restorer_name(layout, manager).to_owned(),
            registrar,
            kept_in,
        })
    }
}

impl Restoring for Restorer {
    /// The restorer unregistered, if its manager holds it: what it was known
    /// by, or nothing when the manager held none.
    fn unregistered(&self) -> Result<Option<String>, Unmade> {
        let manager = self.registrar.manager();
        let held = self.registrar.is_registered(&self.name).map_err(|error| {
            Unmade(format!(
                "{manager} could not say whether it holds `{}`: {error}",
                self.name
            ))
        })?;
        if !held {
            return Ok(None);
        }
        self.registrar
            .unregister(&self.name, &self.kept_in)
            .map_err(|error| {
                Unmade(format!("{manager} did not let `{}` go: {error}", self.name))
            })?;
        Ok(Some(self.name.clone()))
    }

    /// The daemon at `daemon`, of `found`'s install, registered `--restored`.
    ///
    /// It is to start at each login, and is started through the manager — unless
    /// the profile was `marked` stopped before the install.
    fn registered(self, daemon: &Path, found: &Found, marked: bool) -> Result<Registered, Unmade> {
        let manager = self.registrar.manager();
        let refused = |doing: &str, error: &nc_platform::service::Error| {
            Unmade(format!(
                "{manager} did not {doing} the daemon of {} as `{}`: {error}",
                found.layout.cli, self.name
            ))
        };
        made_with_its_parents(&self.kept_in)?;
        let service = Service::new(
            &self.name,
            &format!("Noctua's daemon of {}", found.layout.cli),
            daemon,
            &["--restored".to_owned()],
            RESTORED_AFTER,
            &found.user,
        )
        .map_err(|error| refused("take", &error))?;
        self.registrar
            .register(&service, &self.kept_in)
            .map_err(|error| refused("register", &error))?;
        if let Err(error) = self.registrar.enable(&self.name, &self.kept_in) {
            // A restorer that would not start at login is taken back whole.
            drop(self.registrar.unregister(&self.name, &self.kept_in));
            return Err(refused("enable at login", &error));
        }
        let started = if started_once_registered(marked) {
            Some(controlled(found, crate::args::Control::Start)?)
        } else {
            None
        };
        Ok(Registered {
            manager,
            name: self.name,
            kept_in: self.kept_in,
            started,
        })
    }
}

/// Removes the install `install` describes: its daemon stopped, its restorer
/// unregistered where this system's service manager holds one, then its
/// binaries and its manifest, and its `bin` once empty. **The profile stays**
/// — its device, its database and its world — so an install made again is
/// the same device with the same data. Answers what was removed, if there
/// was an install.
///
/// # Errors
///
/// [`Unmade`] for a service manager this environment does not say how to
/// reach, which leaves the install as it was, a daemon that did not stop, a
/// restorer its manager could not say it holds or did not let go, or a file
/// another account can reach or that could not be removed.
pub fn uninstall(install: &Install) -> Result<Option<Uninstalled>, Unmade> {
    uninstall_restored_by(install, Restorer::of)
}

/// What [`uninstall`] does, with the restorer the one `restorer_of` answers
/// for the install, its layout and its user.
fn uninstall_restored_by<R: Restoring>(
    install: &Install,
    restorer_of: impl FnOnce(&Install, &Layout, &User) -> Result<R, Unmade>,
) -> Result<Option<Uninstalled>, Unmade> {
    let layout = nc_channel::layout(install, "");
    let cli = layout
        .install
        .join(format!("{}{}", layout.cli, nc_platform::EXECUTABLE_SUFFIX));
    let found = match nc_channel::found::beside(&cli) {
        Ok(found) => found,
        Err(nc_channel::found::Refused::NoManifest { .. }) => return Ok(None),
        Err(nc_channel::found::Refused::Unavailable { error, .. })
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(None);
        }
        Err(refused) => return Err(Unmade(format!("the install does not read: {refused}"))),
    };
    // **The manager found before anything is touched**: one this environment
    // does not say how to reach may hold the restorer, which would start a
    // daemon removed under it, so the install is left whole.
    let restorer = restorer_of(&found.install, &found.layout, &found.user).map_err(|unmade| {
        Unmade(format!(
            "{unmade}, so whether it holds the daemon's restorer cannot be said and nothing was \
             removed: uninstall from an environment that reaches it"
        ))
    })?;
    stopped(&found)?;
    let unregistered = restorer.unregistered()?;
    let layout = found.layout;
    let daemon = layout.install.join(format!(
        "{}{}",
        layout.daemon,
        nc_platform::EXECUTABLE_SUFFIX
    ));
    for file in [&cli, &daemon, &layout.manifest] {
        match nc_platform::judge(file, Kind::File, &found.user) {
            Ok(()) => {
                std::fs::remove_file(file).map_err(|error| unmade("removed", file, &error))?;
            }
            Err(Judged::Failed(absent))
                if absent.error().kind() == std::io::ErrorKind::NotFound => {}
            Err(judged) => return Err(not_taken(&judged)),
        }
    }
    std::fs::remove_dir(&layout.install)
        .map_err(|error| unmade("removed", &layout.install, &error))?;
    Ok(Some(Uninstalled {
        layout,
        unregistered,
    }))
}

/// What an earlier install's daemon was told, when the CLI at `cli` is one:
/// it is stopped, so no binary is replaced under it.
fn stopped_before(cli: &Path) -> Result<Option<String>, Unmade> {
    match nc_channel::found::beside(cli) {
        Ok(found) => stopped(&found).map(Some),
        Err(_) => Ok(None),
    }
}

/// The daemon of `found`'s install stopped, through the CLI's own stop.
fn stopped(found: &Found) -> Result<String, Unmade> {
    controlled(found, crate::args::Control::Stop)
}

/// The daemon of `found`'s install, asked `verb` through the CLI's process
/// control: what it said, or why it did not.
fn controlled(found: &Found, verb: crate::args::Control) -> Result<String, Unmade> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| Unmade(format!("the runtime process control runs on: {error}")))?;
    match runtime.block_on(crate::control::controlled(found, verb)) {
        Ok(crate::control::Done::Said(said)) => Ok(said),
        Ok(crate::control::Done::Status(_)) => Ok(String::new()),
        Err(failure) => Err(Unmade(format!(
            "the daemon of {} did not {} [{}]: {}",
            found.layout.cli,
            verb.word(),
            failure.code.code(),
            failure.detail
        ))),
    }
}

/// `path` made, and every parent it lacks, one level at a time and each
/// private; what is there already is left as it is.
fn made_with_its_parents(path: &Path) -> Result<(), Unmade> {
    let mut missing = Vec::new();
    let mut at = path;
    while std::fs::symlink_metadata(at).is_err() {
        missing.push(at);
        at = at
            .parent()
            .ok_or_else(|| Unmade(format!("`{}` has no parent that is there", path.display())))?;
    }
    for directory in missing.into_iter().rev() {
        nc_platform::create_private_directory(directory)
            .map_err(|error| unmade("made", directory, &error))?;
    }
    Ok(())
}

/// The directory `path`, made private when it is not there or judged so.
///
/// Judged this user's alone by `user`, or, when no user is known yet, by its
/// own owner and then the user a probe in it names.
fn directory(path: &Path, user: Option<&User>) -> Result<(), Unmade> {
    if std::fs::symlink_metadata(path).is_err() {
        return nc_platform::create_private_directory(path)
            .map_err(|error| unmade("made", path, &error));
    }
    match user {
        Some(user) => nc_platform::judge(path, Kind::Directory, user),
        None => nc_platform::user_in(path).map(drop),
    }
    .map_err(|judged| not_taken(&judged))
}

/// Every byte of the file `path`.
fn read_whole(path: &Path) -> Result<Vec<u8>, Unmade> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| unmade("read", path, &error))?;
    Ok(bytes)
}

/// `bytes` written at `path`, whole, through a private file beside it.
///
/// The file is runnable when `executable`, synced and renamed over `path`, and
/// the directory synced, so `path` holds all of them or what it held before; a
/// file an interrupted write left beside it is removed once judged this user's
/// alone.
fn replaced(path: &Path, bytes: &[u8], user: &User, executable: bool) -> Result<(), Unmade> {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".new");
    let beside = path.with_file_name(name);
    match nc_platform::judge(&beside, Kind::File, user) {
        Ok(()) => {
            std::fs::remove_file(&beside).map_err(|error| unmade("removed", &beside, &error))?;
        }
        Err(Judged::Failed(absent)) if absent.error().kind() == std::io::ErrorKind::NotFound => {}
        Err(judged) => return Err(not_taken(&judged)),
    }
    let mut file = if executable {
        nc_platform::create_private_executable(&beside)
    } else {
        nc_platform::create_private(&beside)
    }
    .map_err(|error| unmade("made", &beside, &error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| unmade("written", &beside, &error))?;
    drop(file);
    std::fs::rename(&beside, path).map_err(|error| unmade("renamed over", path, &error))?;
    if let Some(directory) = path.parent() {
        nc_platform::sync_directory(directory)
            .map_err(|error| unmade("synced", directory, &error))?;
    }
    Ok(())
}

/// The profile initialized by the install's own daemon: `daemon
/// --initialize --world <world>`, its output read, never this process's.
#[expect(
    clippy::disallowed_methods,
    reason = "the installed daemon's own entry for an installer, run with its standard input \
              and output null and its standard error read back, never this process's"
)]
fn initialized(daemon: &Path, world: &Path) -> Result<(), Unmade> {
    let output = std::process::Command::new(daemon)
        .arg("--initialize")
        .arg("--world")
        .arg(world)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|error| unmade("run", daemon, &error))?;
    if output.status.success() {
        return Ok(());
    }
    Err(Unmade(format!(
        "`{}` --initialize ended {}: {}",
        daemon.display(),
        output.status,
        crate::render::escaped(String::from_utf8_lossy(&output.stderr).trim_end())
    )))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Plan, Plugin, Unmade};

    /// A scratch directory for one test, removed however the test leaves.
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
                std::env::temp_dir().join(format!("nc-install-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        /// `text` written at `name` in the scratch, as a build leaves a file.
        fn file(&self, name: &str, text: &str) -> PathBuf {
            use std::io::Write as _;
            let path = self.0.join(name);
            nc_platform::create_private(&path)
                .expect("a file")
                .write_all(text.as_bytes())
                .expect("written");
            path
        }

        /// A development install relocated under `namespace`, its bases
        /// under `bases`, which are not there yet.
        fn install(&self, namespace: &str) -> nc_channel::Install {
            let base = |name: &str| self.0.join("home").join(name);
            nc_channel::Install {
                channel: nc_channel::Channel::Development(nc_channel::Checkout::of(&self.0)),
                bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                    .expect("plain absolute bases"),
                namespace: Some(nc_channel::Namespace::parse(namespace).expect("a namespace")),
                log_endpoint: None,
            }
        }
    }

    /// What an initialize stand-in does: the profile's plugins directory made
    /// as the daemon's own would make it, and the world it was handed kept.
    fn initialize_as_the_daemon(
        layout: &nc_channel::Layout,
        kept: &std::cell::RefCell<Option<String>>,
    ) -> impl FnOnce(&Path, &Path) -> Result<(), Unmade> {
        move |daemon: &Path, world: &Path| {
            assert!(
                daemon.is_file(),
                "the daemon was not copied before it was run"
            );
            kept.replace(Some(
                std::fs::read_to_string(world).expect("the world reads"),
            ));
            for directory in [&layout.data, &layout.plugins] {
                nc_platform::create_private_directory(directory).expect("a directory");
            }
            Ok(())
        }
    }

    /// An install is its binaries, its manifest, its world and its plugins.
    ///
    /// The binaries are copied under the layout's names, the manifest written
    /// and read back as the binaries read it, the world handed to the daemon's
    /// initialize and removed, and each plugin written into its own directory —
    /// every file private, every missing base made.
    #[test]
    fn an_install_is_its_binaries_its_manifest_its_world_and_its_plugins() {
        let scratch = Scratch::new("made");
        let (cli, daemon) = (
            scratch.file("noctua", "cli"),
            scratch.file("noctua-daemon", "dmn"),
        );
        let manifest = scratch.file("plugin.toml", "id = \"x\"\n");
        let component = scratch.file("x.wasm", "wasm");
        let install = scratch.install("ti1");
        let plan = Plan {
            install: install.clone(),
            cli: &cli,
            daemon: &daemon,
            world: "{\"world\":1}",
            plugins: vec![Plugin {
                name: "x",
                manifest: &manifest,
                component: &component,
            }],
            restorer: false,
        };
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let layout = nc_channel::layout(&install, &user.to_string());
        let kept = std::cell::RefCell::new(None);
        let installed =
            super::install_with(&plan, initialize_as_the_daemon(&layout, &kept)).expect("made");
        assert_eq!(installed.found.install, install);
        assert_eq!(installed.found.layout, layout);
        assert_eq!(kept.take().as_deref(), Some("{\"world\":1}"));
        assert!(!layout.state.join("world.json.install").exists());
        let suffix = nc_platform::EXECUTABLE_SUFFIX;
        let copied_cli = layout.install.join(format!("{}{suffix}", layout.cli));
        assert_eq!(installed.cli, copied_cli);
        for (path, holds) in [
            (copied_cli, "cli"),
            (
                layout.install.join(format!("{}{suffix}", layout.daemon)),
                "dmn",
            ),
            (
                layout.plugins.join("x").join(nc_channel::PLUGIN_MANIFEST),
                "id = \"x\"\n",
            ),
            (
                layout.plugins.join("x").join(nc_channel::PLUGIN_COMPONENT),
                "wasm",
            ),
        ] {
            assert_eq!(std::fs::read_to_string(&path).expect("there"), holds);
            nc_platform::judge(&path, nc_platform::Kind::File, &user).expect("private");
        }
        assert_eq!(
            std::fs::read_to_string(&layout.manifest).expect("a manifest"),
            install.to_canonical()
        );
        assert!(installed.stopped.is_none(), "nothing ran before");
    }

    /// Whether the copy of this test binary at `binary` runs and ends well,
    /// listing no test.
    #[expect(
        clippy::disallowed_methods,
        reason = "a copy of this test binary an install made is started to show it runs; it \
                  lists nothing and ends, its standard handles null"
    )]
    fn runs(binary: &Path) -> bool {
        // A file just written may still be open in a sibling test's child,
        // between its fork and its exec, which Linux answers as a file busy:
        // asked again, for a bounded time.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            match std::process::Command::new(binary)
                .args(["--exact", "no::such::test"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
            {
                Ok(status) => return status.success(),
                Err(error)
                    if error.kind() == std::io::ErrorKind::ExecutableFileBusy
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(error) => panic!("`{}` did not start: {error}", binary.display()),
            }
        }
    }

    /// The binaries are copied runnable by their owner alone.
    ///
    /// This test binary stands in for both, and each copy is run as an
    /// install runs it: the daemon by the installer, the CLI by its user.
    #[test]
    fn the_binaries_are_copied_runnable_by_their_owner_alone() {
        let scratch = Scratch::new("runs");
        let this = std::env::current_exe().expect("this test binary");
        let install = scratch.install("ti4");
        let plan = Plan {
            install: install.clone(),
            cli: &this,
            daemon: &this,
            world: "{}",
            plugins: Vec::new(),
            restorer: false,
        };
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let installed = super::install_with(&plan, |daemon: &Path, _: &Path| {
            assert!(runs(daemon), "the daemon was copied and does not run");
            nc_platform::judge(daemon, nc_platform::Kind::File, &user).expect("its owner's alone");
            Ok(())
        })
        .expect("made");
        assert!(runs(&installed.cli), "the CLI was copied and does not run");
        nc_platform::judge(&installed.cli, nc_platform::Kind::File, &user)
            .expect("its owner's alone");
    }

    /// An install made again replaces every file, stops what the earlier one
    /// runs first, and a refused initialize is the install's refusal.
    #[test]
    fn an_install_made_again_replaces_it_and_a_refused_initialize_refuses() {
        let scratch = Scratch::new("again");
        let (cli, daemon) = (
            scratch.file("noctua", "one"),
            scratch.file("noctua-daemon", "one"),
        );
        let install = scratch.install("ti2");
        let plan = Plan {
            install: install.clone(),
            cli: &cli,
            daemon: &daemon,
            world: "{}",
            plugins: Vec::new(),
            restorer: false,
        };
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let layout = nc_channel::layout(&install, &user.to_string());
        let kept = std::cell::RefCell::new(None);
        super::install_with(&plan, initialize_as_the_daemon(&layout, &kept)).expect("made");
        let newer = scratch.file("noctua-2", "two");
        let again = Plan {
            cli: &newer,
            ..plan.clone()
        };
        let installed = super::install_with(&again, |_: &Path, _: &Path| Ok(())).expect("again");
        assert_eq!(
            std::fs::read_to_string(&installed.cli).expect("there"),
            "two"
        );
        let said = installed
            .stopped
            .expect("the earlier install was stopped first");
        assert!(said.contains("is stopped"), "{said}");
        let refused = super::install_with(&plan, |_: &Path, _: &Path| {
            Err(Unmade("refused".to_owned()))
        })
        .expect_err("a refused initialize");
        assert_eq!(refused.to_string(), "refused");
        assert!(
            !layout.state.join("world.json.install").exists(),
            "the world handed over outlived a refusal"
        );
    }

    /// An uninstall removes the binaries, the manifest and `bin`, and leaves
    /// the profile; with nothing installed it says so.
    #[test]
    fn an_uninstall_removes_the_binaries_and_leaves_the_profile() {
        let scratch = Scratch::new("removed");
        let (cli, daemon) = (
            scratch.file("noctua", "c"),
            scratch.file("noctua-daemon", "d"),
        );
        let install = scratch.install("ti3");
        assert!(super::uninstall(&install).expect("nothing").is_none());
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let layout = nc_channel::layout(&install, &user.to_string());
        let kept = std::cell::RefCell::new(None);
        let plan = Plan {
            install: install.clone(),
            cli: &cli,
            daemon: &daemon,
            world: "{}",
            plugins: Vec::new(),
            restorer: false,
        };
        super::install_with(&plan, initialize_as_the_daemon(&layout, &kept)).expect("made");
        let removed = super::uninstall(&install)
            .expect("removed")
            .expect("there was one");
        assert_eq!(removed.layout.install, layout.install);
        assert_eq!(removed.unregistered, None, "nothing was registered");
        assert!(!layout.install.exists(), "`bin` outlived the uninstall");
        assert!(
            layout.state.is_dir() && layout.data.is_dir(),
            "the profile went"
        );
        assert!(super::uninstall(&install).expect("again").is_none());
    }

    /// An uninstall stops the daemon first: one that did not stop leaves
    /// every file of the install where it was, and the mark the stop writes
    /// stays with the profile either way.
    #[test]
    fn an_uninstall_stops_the_daemon_first() {
        use std::io::Write as _;
        let scratch = Scratch::new("first");
        let (cli, daemon) = (
            scratch.file("noctua", "c"),
            scratch.file("noctua-daemon", "d"),
        );
        let install = scratch.install("ti5");
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let layout = nc_channel::layout(&install, &user.to_string());
        let kept = std::cell::RefCell::new(None);
        let plan = Plan {
            install: install.clone(),
            cli: &cli,
            daemon: &daemon,
            world: "{}",
            plugins: Vec::new(),
            restorer: false,
        };
        let installed =
            super::install_with(&plan, initialize_as_the_daemon(&layout, &kept)).expect("made");
        // A record that does not read names nothing a stop may signal, so the
        // stop is refused.
        nc_platform::create_private_directory(&layout.runtime).expect("a runtime directory");
        nc_platform::create_private(&layout.pid_file)
            .expect("a record")
            .write_all(b"{\"pid\":0}")
            .expect("written");
        let refused = super::uninstall(&install).expect_err("the daemon did not stop");
        assert!(
            refused
                .to_string()
                .contains(nc_catalog::ErrorCode::CliProcessUnverified.code()),
            "{refused}"
        );
        let suffix = nc_platform::EXECUTABLE_SUFFIX;
        for file in [
            installed.cli,
            layout.install.join(format!("{}{suffix}", layout.daemon)),
            layout.manifest.clone(),
        ] {
            assert!(
                file.is_file(),
                "`{}` went with a daemon that did not stop",
                file.display()
            );
        }
        assert!(layout.stopped.is_file(), "the stop wrote no mark");
        std::fs::remove_file(&layout.pid_file).expect("removed");
        super::uninstall(&install)
            .expect("removed")
            .expect("there was one");
        assert!(!layout.install.exists(), "`bin` outlived the uninstall");
        assert!(layout.stopped.is_file(), "the mark went with the install");
    }

    /// A restorer is known to each manager by the name the layout gives it
    /// there: the unit, the label of the agent, the task.
    #[test]
    fn a_restorer_is_known_by_the_name_the_layout_gives_it_there() {
        use nc_platform::service::Manager;
        let scratch = Scratch::new("named");
        let layout = nc_channel::layout(&scratch.install("tn1"), "1000");
        assert_eq!(
            super::restorer_name(&layout, Manager::Systemd),
            layout.systemd_unit
        );
        assert_eq!(
            super::restorer_name(&layout, Manager::Launchd),
            layout.launchd_label
        );
        assert_eq!(
            super::restorer_name(&layout, Manager::TaskScheduler),
            layout.task
        );
        for manager in Manager::ALL {
            assert!(
                manager.takes(super::restorer_name(&layout, manager)),
                "{manager} takes no name the layout gives it"
            );
        }
    }

    /// A real restorer is kept where a login reads it, and a test's where
    /// none does: in systemd's runtime directory, or in its own profile.
    ///
    /// **Where its definition is**: under Task Scheduler a task is the
    /// manager's own whatever became of its file, and starts at each logon.
    #[test]
    fn a_real_restorer_is_kept_where_a_login_reads_it_and_a_tests_where_none_does() {
        use nc_platform::service::Manager;
        let scratch = Scratch::new("kept");
        let relocated = scratch.install("tk1");
        let real = nc_channel::Install {
            namespace: None,
            ..relocated.clone()
        };
        let volatile = scratch.0.join("run").join("systemd").join("user");
        let (layout, real_layout) = (
            nc_channel::layout(&relocated, "1000"),
            nc_channel::layout(&real, "1000"),
        );
        let kept =
            |install, layout, manager| super::kept_in(install, layout, manager, Some(&volatile));
        assert_eq!(
            kept(&relocated, &layout, Manager::Systemd),
            Some(volatile.clone())
        );
        for manager in [Manager::Launchd, Manager::TaskScheduler] {
            assert_eq!(
                kept(&relocated, &layout, manager),
                Some(layout.state.clone()),
                "{manager}"
            );
        }
        assert_eq!(
            super::kept_in(&relocated, &layout, Manager::Systemd, None),
            None,
            "a test's unit was kept somewhere other than the runtime directory"
        );
        for manager in [Manager::Systemd, Manager::Launchd] {
            assert_eq!(
                kept(&real, &real_layout, manager),
                manager.loaded_at_login(real.bases.config()),
                "{manager}"
            );
        }
        assert_eq!(
            kept(&real, &real_layout, Manager::TaskScheduler),
            Some(real_layout.state.clone())
        );
    }

    /// A development plan registers its restorer: the plan `cargo xtask dev
    /// --install` makes, and the restorer's suite installs, with what it was
    /// handed.
    #[test]
    fn a_development_plan_registers_its_restorer() {
        let scratch = Scratch::new("plan");
        let (cli, daemon) = (scratch.0.join("noctua"), scratch.0.join("noctua-daemon"));
        let install = scratch.install("tp1");
        let plan = super::development_plan(install.clone(), &cli, &daemon, "{}", Vec::new());
        assert!(plan.restorer, "a development install registers no restorer");
        assert_eq!(plan.install, install);
        assert_eq!(
            (plan.cli, plan.daemon, plan.world),
            (cli.as_path(), daemon.as_path(), "{}")
        );
        assert!(plan.plugins.is_empty());
    }

    /// What a stand-in restorer saw of an install, in the order it saw it.
    struct Recorded {
        /// The CLI the install writes, read as the earlier restorer is let go.
        cli: PathBuf,
        /// Everything seen.
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl super::Restoring for Recorded {
        fn unregistered(&self) -> Result<Option<String>, Unmade> {
            let held = std::fs::read_to_string(&self.cli).unwrap_or_default();
            self.seen
                .borrow_mut()
                .push(format!("let go over a CLI holding `{held}`"));
            Ok(None)
        }

        fn registered(
            self,
            _: &Path,
            found: &nc_channel::found::Found,
            marked: bool,
        ) -> Result<super::Registered, Unmade> {
            self.seen
                .borrow_mut()
                .push(format!("registered, marked {marked}"));
            Ok(super::Registered {
                manager: nc_platform::service::MANAGER,
                name: "x".to_owned(),
                kept_in: found.layout.state.clone(),
                started: None,
            })
        }
    }

    /// A reinstall lets the earlier restorer go before any binary is
    /// replaced, and reads the mark before its own stop writes one.
    ///
    /// So no restorer starts a daemon half replaced, a daemon an earlier
    /// install ran is started again, and one a person stopped stays down.
    #[test]
    fn a_reinstall_lets_the_earlier_restorer_go_first_and_reads_the_mark_before_its_own_stop() {
        let scratch = Scratch::new("order");
        let (one, two, daemon) = (
            scratch.file("noctua", "one"),
            scratch.file("noctua-2", "two"),
            scratch.file("noctua-daemon", "d"),
        );
        let install = scratch.install("to1");
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let layout = nc_channel::layout(&install, &user.to_string());
        let cli = layout
            .install
            .join(format!("{}{}", layout.cli, nc_platform::EXECUTABLE_SUFFIX));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let restorer = || {
            let (cli, seen) = (cli.clone(), std::rc::Rc::clone(&seen));
            move |_: &nc_channel::Install, _: &nc_channel::Layout, _: &nc_platform::User| {
                Ok(Recorded { cli, seen })
            }
        };
        let kept = std::cell::RefCell::new(None);
        let first = super::development_plan(install.clone(), &one, &daemon, "{}", Vec::new());
        super::install_restored_by(&first, initialize_as_the_daemon(&layout, &kept), restorer())
            .expect("made");
        assert_eq!(
            seen.take(),
            ["let go over a CLI holding ``", "registered, marked false"]
        );
        let again = super::development_plan(install.clone(), &two, &daemon, "{}", Vec::new());
        let installed = super::install_restored_by(&again, |_: &Path, _: &Path| Ok(()), restorer())
            .expect("made again");
        assert!(
            installed.stopped.is_some(),
            "the earlier daemon was not stopped"
        );
        assert!(
            layout.stopped.is_file(),
            "the install's own stop wrote no mark, so where it is read is not held"
        );
        assert_eq!(
            seen.take(),
            [
                "let go over a CLI holding `one`",
                "registered, marked false"
            ],
            "the earlier restorer was let go after a binary was replaced, or the mark the \
             install's own stop wrote was read as a person's"
        );
        // **A person's stop**: the mark the start through the restorer would
        // have removed, written again.
        std::fs::remove_file(&layout.stopped).expect("the start's part");
        crate::control::mark(&layout, &user).expect("a person's stop");
        super::install_restored_by(&again, |_: &Path, _: &Path| Ok(()), restorer())
            .expect("made over a stop");
        assert_eq!(
            seen.take(),
            ["let go over a CLI holding `two`", "registered, marked true"],
            "a person's stop was not read before the install"
        );
    }

    /// An uninstall that cannot reach the service manager removes nothing.
    ///
    /// It stops no daemon and removes no file, since that manager may hold a
    /// restorer that would start what is removed under it.
    #[test]
    fn an_uninstall_that_cannot_reach_its_manager_removes_nothing() {
        let scratch = Scratch::new("unreached");
        let (cli, daemon) = (
            scratch.file("noctua", "c"),
            scratch.file("noctua-daemon", "d"),
        );
        let install = scratch.install("tu1");
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let layout = nc_channel::layout(&install, &user.to_string());
        let kept = std::cell::RefCell::new(None);
        let plan = Plan {
            install: install.clone(),
            cli: &cli,
            daemon: &daemon,
            world: "{}",
            plugins: Vec::new(),
            restorer: false,
        };
        let installed =
            super::install_with(&plan, initialize_as_the_daemon(&layout, &kept)).expect("made");
        let refused = super::uninstall_restored_by(
            &install,
            |_: &nc_channel::Install,
             _: &nc_channel::Layout,
             _: &nc_platform::User|
             -> Result<Recorded, Unmade> {
                Err(Unmade(
                    "this system's service manager could not be found: no `systemctl`".to_owned(),
                ))
            },
        )
        .expect_err("an uninstall with no manager to ask");
        assert!(
            refused.to_string().contains("nothing was removed"),
            "{refused}"
        );
        assert!(
            installed.cli.is_file() && layout.manifest.is_file(),
            "a file went"
        );
        assert!(!layout.stopped.exists(), "the daemon was stopped");
    }

    /// An install keeps a person's stop.
    ///
    /// It starts its daemon once its restorer is registered, unless the profile
    /// carried the `stopped` mark before it.
    #[test]
    fn an_install_keeps_a_persons_stop() {
        assert!(super::started_once_registered(false));
        assert!(!super::started_once_registered(true));
    }

    /// A development install is read from the environment it is handed.
    ///
    /// Its bases are the system's, it is relocated under a namespace when one
    /// is named — with no central log then — and refused without a home.
    #[test]
    fn a_development_install_is_read_from_the_environment_handed_over() {
        use std::ffi::OsString;
        let scratch = Scratch::new("environment");
        let home = scratch.0.join("home");
        let environment = |pairs: Vec<(&'static str, OsString)>| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(variable, _)| *variable == name)
                    .map(|(_, value)| value.clone())
            }
        };
        let everything = |namespace: Option<&str>| {
            let mut pairs = vec![
                ("HOME", home.clone().into_os_string()),
                ("LOCALAPPDATA", home.join("local").into_os_string()),
                ("APPDATA", home.join("roaming").into_os_string()),
            ];
            if let Some(namespace) = namespace {
                pairs.push((super::NAMESPACE, OsString::from(namespace)));
            }
            environment(pairs)
        };
        let endpoint = Some("http://127.0.0.1:20001/insert/jsonline".to_owned());
        let real =
            super::development(&scratch.0, &everything(None), endpoint.clone()).expect("read");
        let bases = nc_platform::base_directories(everything(None)).expect("read");
        assert_eq!(real.bases.data(), bases.data);
        assert_eq!(real.bases.state(), bases.state);
        assert!(real.bases.state().starts_with(&home), "{real:?}");
        assert_eq!(
            real.channel,
            nc_channel::Channel::Development(nc_channel::Checkout::of(&scratch.0))
        );
        assert_eq!(real.namespace, None);
        assert_eq!(real.log_endpoint, endpoint);
        let relocated =
            super::development(&scratch.0, &everything(Some("t9")), endpoint).expect("read");
        assert_eq!(
            relocated
                .namespace
                .as_ref()
                .map(nc_channel::Namespace::as_str),
            Some("t9")
        );
        assert_eq!(relocated.log_endpoint, None, "a test's install named a log");
        assert!(
            super::development(&scratch.0, &everything(Some("a/b")), None).is_err(),
            "a namespace with a separator was taken"
        );
        assert!(super::development(&scratch.0, &environment(Vec::new()), None).is_err());
    }

    /// A development install's world is the test world with its realm, its
    /// person, the grant and the epoch, and a world that is not JSON is
    /// refused.
    #[test]
    fn a_development_world_names_its_realm_person_grant_and_epoch() {
        let world = super::development_world("{\"entities\":[]}").expect("written");
        assert_eq!(
            world,
            "{\"epoch\":7,\"grant\":\"g.by_policy\",\"person\":\"ana\",\"realm\":\"home\",\
             \"world\":{\"entities\":[]}}"
        );
        assert!(super::development_world("{").is_err());
    }
}
