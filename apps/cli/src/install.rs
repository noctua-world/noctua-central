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
//!
//! **No service manager is told** yet, and nothing is added to `PATH`: the
//! CLI is reached at its path in the install, which the installer prints.
//!
//! # What this does not answer
//!
//! A process running as the same user, which can replace any file of an
//! install between its writing and its use.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use nc_canon::Value;
use nc_channel::found::Found;
use nc_channel::{Install, Layout};
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
    let stopped = stopped_before(&cli)?;
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
    Ok(Installed {
        found,
        cli,
        stopped,
    })
}

/// Removes the install `install` describes: its daemon stopped, then its
/// binaries and its manifest, and its `bin` once empty. **The profile stays**
/// — its device, its database and its world — so an install made again is
/// the same device with the same data. Answers whether there was one.
///
/// # Errors
///
/// [`Unmade`] for a daemon that did not stop, or a file another account can
/// reach or that could not be removed.
pub fn uninstall(install: &Install) -> Result<Option<Layout>, Unmade> {
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
    stopped(&found)?;
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
    Ok(Some(layout))
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
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| Unmade(format!("the runtime a stop runs on: {error}")))?;
    match runtime.block_on(crate::control::controlled(
        found,
        crate::args::Control::Stop,
    )) {
        Ok(crate::control::Done::Said(said)) => Ok(said),
        Ok(crate::control::Done::Status(_)) => Ok(String::new()),
        Err(failure) => Err(Unmade(format!(
            "the daemon of {} did not stop [{}]: {}",
            found.layout.cli,
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
        };
        super::install_with(&plan, initialize_as_the_daemon(&layout, &kept)).expect("made");
        let removed = super::uninstall(&install)
            .expect("removed")
            .expect("there was one");
        assert_eq!(removed.install, layout.install);
        assert!(!layout.install.exists(), "`bin` outlived the uninstall");
        assert!(
            layout.state.is_dir() && layout.data.is_dir(),
            "the profile went"
        );
        assert!(super::uninstall(&install).expect("again").is_none());
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
