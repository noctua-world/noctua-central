//! The plugins installed in the profile, loaded at each start (`NOCTUA.md
//! §11.3`): `<data>/plugins/<name>/plugin.toml` and `component.wasm` beside it.
//!
//! **Each judged this user's alone before it is read**: the plugins directory
//! the profile makes private, each plugin's directory, its manifest and its
//! component. A plugin is code the kernel runs with the grants its manifest
//! names, so one another account could write is one that account chose.
//!
//! **A plugin refused is logged and shown, never fatal.** Each is loaded on
//! its own, in the order of its name: one whose directory, manifest or
//! component is refused, or which the kernel does not load, is logged as
//! `daemon.plugin_refused` with the code of what refused it and kept in the
//! status with its sentence, and the daemon runs without it — the kernel and
//! every other plugin are still the person's.

use std::path::Path;

use nc_catalog::ErrorCode;
use nc_kernel::Kernel;
use nc_kernel::logging::LogSink;
use nc_kernel::manifest::PluginManifest;
use nc_platform::{Kind, User};

use crate::environment::Daemon;

/// The name of a plugin's manifest in its directory.
pub const MANIFEST: &str = "plugin.toml";

/// The name of a plugin's component in its directory.
pub const COMPONENT: &str = "component.wasm";

/// How one installed plugin's load ended, as the status shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The name of its directory.
    pub name: String,
    /// Nothing for a plugin the kernel loaded, or the code and the sentence
    /// of what refused it.
    pub refused: Option<(ErrorCode, String)>,
}

/// A plugin read from its directory, before the kernel is asked to load it.
#[derive(Debug)]
struct Installed {
    manifest: PluginManifest,
    component: Vec<u8>,
}

/// The names in the plugins directory, in order, once it is judged this
/// user's alone.
///
/// # Errors
///
/// The code and the sentence of what refused the directory.
fn names(directory: &Path, user: &User) -> Result<Vec<String>, (ErrorCode, String)> {
    nc_platform::judge(directory, Kind::Directory, user).map_err(judged)?;
    let entries = std::fs::read_dir(directory).map_err(|error| {
        (
            ErrorCode::DaemonProfileUnavailable,
            format!("`{}` could not be listed: {error}", directory.display()),
        )
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            (
                ErrorCode::DaemonProfileUnavailable,
                format!("`{}` could not be listed: {error}", directory.display()),
            )
        })?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// The plugin in `directory`, each of its files judged and read.
fn installed(directory: &Path, user: &User) -> Result<Installed, (ErrorCode, String)> {
    nc_platform::judge(directory, Kind::Directory, user).map_err(judged)?;
    let manifest = read(&directory.join(MANIFEST), user)?;
    let manifest = String::from_utf8(manifest).map_err(|_| {
        (
            ErrorCode::PluginManifestInvalid,
            format!("`{}` is not UTF-8", directory.join(MANIFEST).display()),
        )
    })?;
    let manifest = PluginManifest::parse(&manifest)
        .map_err(|refused| (refused.code(), refused.to_string()))?;
    let component = read(&directory.join(COMPONENT), user)?;
    Ok(Installed {
        manifest,
        component,
    })
}

/// The file at `path`, judged this user's alone and read.
fn read(path: &Path, user: &User) -> Result<Vec<u8>, (ErrorCode, String)> {
    nc_platform::judge(path, Kind::File, user).map_err(judged)?;
    std::fs::read(path).map_err(|error| {
        (
            ErrorCode::DaemonProfileUnavailable,
            format!("`{}` could not be read: {error}", path.display()),
        )
    })
}

/// A path judged and not taken, as a plugin's refusal.
fn judged(judged: nc_platform::Judged) -> (ErrorCode, String) {
    let refused = crate::Refused::judged(judged);
    (refused.code(), refused.to_string())
}

/// Loads every plugin installed under `directory` into `kernel`, logging each
/// one refused to `log`, and answers how each load ended.
///
/// **Never fatal**: a directory that cannot be listed is one refusal, under
/// the directory's own name, and every plugin in it is one load of its own.
pub async fn load(
    kernel: &Kernel<Daemon>,
    directory: &Path,
    user: &User,
    log: &impl LogSink,
) -> Vec<Loaded> {
    let mut loaded = Vec::new();
    let names = match names(directory, user) {
        Ok(names) => names,
        Err(refused) => {
            let name = directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            loaded.push(Loaded {
                name,
                refused: Some(refused),
            });
            names_refused(&loaded, log);
            return loaded;
        }
    };
    for name in names {
        let refused = match installed(&directory.join(&name), user) {
            Err(refused) => Some(refused),
            Ok(Installed {
                manifest,
                component,
            }) => match kernel.load(manifest, &component).await {
                Ok(_) => None,
                Err(refused) => Some(first_problem(&refused)),
            },
        };
        loaded.push(Loaded { name, refused });
    }
    names_refused(&loaded, log);
    loaded
}

/// The code and the sentence of the first problem a load was refused with.
fn first_problem(refused: &nc_kernel::LoadError) -> (ErrorCode, String) {
    refused.problems().first().map_or_else(
        || (ErrorCode::PluginComponentInvalid, refused.to_string()),
        |problem| (problem.code(), problem.sentence().to_owned()),
    )
}

/// Logs each refused load as `daemon.plugin_refused`, with its code.
fn names_refused(loaded: &[Loaded], log: &impl LogSink) {
    for (code, _) in loaded.iter().filter_map(|plugin| plugin.refused.as_ref()) {
        log.record(&nc_log::events::DaemonPluginRefused { code: *code });
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nc_catalog::ErrorCode;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("nc-daemon-plugins-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        fn user(&self) -> nc_platform::User {
            nc_platform::user_in(&self.0).expect("the scratch is private")
        }

        /// A plugin's directory under the scratch, holding `manifest` and
        /// `component` as an installer writes them.
        fn plugin(&self, name: &str, manifest: &str, component: &[u8]) -> PathBuf {
            use std::io::Write as _;
            let directory = self.0.join(name);
            nc_platform::create_private_directory(&directory).expect("a plugin's directory");
            nc_platform::create_private(&directory.join(super::MANIFEST))
                .and_then(|mut file| file.write_all(manifest.as_bytes()))
                .expect("a manifest");
            nc_platform::create_private(&directory.join(super::COMPONENT))
                .and_then(|mut file| file.write_all(component))
                .expect("a component");
            directory
        }
    }

    /// The reference plugin's manifest, as the plugin ships it.
    const REFERENCE: &str = include_str!("../../../plugins/reference/plugin.toml");

    /// The names in the plugins directory are read in order, and a directory
    /// another account can reach is refused before anything in it is read.
    #[test]
    fn the_plugins_are_read_in_order_from_a_directory_of_this_users() {
        let scratch = Scratch::new("names");
        let user = scratch.user();
        for name in ["b", "a", "c"] {
            scratch.plugin(name, REFERENCE, b"");
        }
        assert_eq!(
            super::names(&scratch.0, &user).expect("listed"),
            ["a", "b", "c"]
        );
        nc_platform::testing::open_to_another_user(&scratch.0).expect("opened");
        let (code, _) = super::names(&scratch.0, &user).expect_err("reachable");
        assert_eq!(code, ErrorCode::DaemonProfileNotPrivate);
    }

    /// A plugin is read only once its directory, its manifest and its
    /// component are each this user's alone, and a manifest that does not
    /// parse is refused with the manifest's code.
    #[test]
    fn a_plugin_is_read_only_from_files_of_this_users() {
        let scratch = Scratch::new("installed");
        let user = scratch.user();
        let good = scratch.plugin("good", REFERENCE, b"bytes");
        let read = super::installed(&good, &user).expect("read");
        assert_eq!(read.component, b"bytes");
        assert_eq!(read.manifest.id(), "noctua:reference");
        for (name, file) in [
            ("open-dir", None),
            ("open-manifest", Some(super::MANIFEST)),
            ("open-component", Some(super::COMPONENT)),
        ] {
            let directory = scratch.plugin(name, REFERENCE, b"bytes");
            let reached = file.map_or_else(|| directory.clone(), |file| directory.join(file));
            nc_platform::testing::open_to_another_user(&reached).expect("opened");
            let (code, _) = super::installed(&directory, &user).expect_err(name);
            assert_eq!(code, ErrorCode::DaemonProfileNotPrivate, "{name}");
        }
        let unparsed = scratch.plugin("unparsed", "id = 7", b"bytes");
        let (code, _) = super::installed(&unparsed, &user).expect_err("unparsed");
        assert_eq!(code, ErrorCode::PluginManifestInvalid);
        let missing = scratch.plugin("missing", REFERENCE, b"bytes");
        std::fs::remove_file(missing.join(super::COMPONENT)).expect("removed");
        let (code, _) = super::installed(&missing, &user).expect_err("missing");
        assert_eq!(code, ErrorCode::DaemonProfileUnavailable);
    }

    /// Each refused load is logged with its code, and nothing is logged for
    /// one that loaded.
    #[test]
    fn each_refused_plugin_is_logged_with_its_code() {
        let log = nc_kernel::logging::Captured::default();
        super::names_refused(
            &[
                super::Loaded {
                    name: "a".to_owned(),
                    refused: None,
                },
                super::Loaded {
                    name: "b".to_owned(),
                    refused: Some((ErrorCode::PluginManifestInvalid, "no".to_owned())),
                },
            ],
            &log,
        );
        let lines = log.named("daemon.plugin_refused");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(
            lines[0].text("code").as_deref(),
            Some(ErrorCode::PluginManifestInvalid.code())
        );
    }
}
