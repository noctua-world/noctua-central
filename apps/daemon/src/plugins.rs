//! The plugins installed in the profile, loaded at each start (`NOCTUA.md
//! §11.3`): `<data>/plugins/<name>/plugin.toml`, and beside it the entries its
//! commands are shown by — `commands.toml` and `i18n/<tag>.toml` for every
//! locale this build ships (`§20.4`) — and `component.wasm`.
//!
//! **Each judged this user's alone before it is read**: the plugins directory
//! the profile makes private, each plugin's directory, its manifest, its
//! entries and its component. A plugin is code the kernel runs with the grants
//! its manifest names, so one another account could write is one that account
//! chose.
//!
//! **In this order, each step refusing before the next is read**: the
//! directory and the manifest, so a manifest that does not parse keeps its own
//! code; the command table; the `i18n/` directory, listed, a file in it named
//! for no locale this build ships refused rather than left unread; each
//! locale's texts; and the component last. An entries file is read no further
//! than `PluginEntries::most_bytes` — its length asked of the file first, and
//! the read itself held to one byte past it — so a file of any size costs the
//! daemon that bound and no more, and it must be UTF-8. What refuses the
//! entries, a file of them missing included, carries `[NC-PLUG-0029]`; a path
//! another account can reach, and one the filesystem cannot answer for, keep
//! the profile's codes. Whether the texts are what every command's must be is
//! the kernel's, at `activate`.
//!
//! **A plugin refused is logged and shown, never fatal.** Each is loaded on
//! its own, in the order of its name: one whose directory, manifest, entries
//! or component is refused, or which the kernel does not load, is logged as
//! `daemon.plugin_refused` with the code of what refused it and kept in the
//! status with its sentence, and the daemon runs without it — the kernel and
//! every other plugin are still the person's. **What is kept of a sentence is
//! bounded**, at [`REFUSAL_BYTES_MAX`] bytes cut on a character and ended with
//! `…`, whoever refused the plugin: an entries file within its bound whose
//! every row is malformed is refused in megabytes, and kept whole it would
//! take the status past a frame — so `noctua status`, the one place the
//! refusal is shown, would be refused itself.

use std::io::Read as _;
use std::path::Path;

use nc_catalog::{ErrorCode, Locale};
use nc_kernel::Kernel;
use nc_kernel::entries::PluginEntries;
use nc_kernel::logging::LogSink;
use nc_kernel::manifest::PluginManifest;
use nc_platform::{Kind, User};

use crate::environment::Daemon;

/// The name of a plugin's manifest in its directory: the install's, which
/// an installer writes it by too.
pub const MANIFEST: &str = nc_channel::PLUGIN_MANIFEST;

/// The name of a plugin's component in its directory.
pub const COMPONENT: &str = nc_channel::PLUGIN_COMPONENT;

/// The name of a plugin's command table in its directory.
pub const COMMANDS: &str = nc_channel::PLUGIN_COMMANDS;

/// The name of the directory a plugin's texts are in, one file per locale.
pub const TEXTS: &str = nc_channel::PLUGIN_TEXTS;

/// The most bytes of one plugin's refusal the status keeps.
///
/// Far past a refusal a person reads — each problem the kernel, the manifest
/// or the entries name is a line or two — and far below a frame, so one
/// plugin's refusal, however its files were written, leaves the status room.
pub const REFUSAL_BYTES_MAX: usize = 4_096;

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
    let entries = entries(directory, user)?;
    let component = read(&directory.join(COMPONENT), user)?;
    Ok(Installed {
        manifest: manifest.described_by(entries),
        component,
    })
}

/// The entries the plugin in `directory` describes its commands with.
///
/// Its command table and its texts in every locale, each judged, read
/// within its bound, and read for its shape.
fn entries(directory: &Path, user: &User) -> Result<PluginEntries, (ErrorCode, String)> {
    let commands = bounded(&directory.join(COMMANDS), user)?;
    let texts = directory.join(TEXTS);
    nc_platform::judge(&texts, Kind::Directory, user)
        .map_err(|refused| missing(&texts, refused))?;
    let listed = std::fs::read_dir(&texts)
        .and_then(|listing| {
            listing
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| {
            (
                ErrorCode::DaemonProfileUnavailable,
                format!("`{}` could not be listed: {error}", texts.display()),
            )
        })?;
    let named: Vec<String> = Locale::ALL
        .iter()
        .map(|&locale| nc_channel::plugin_texts(locale))
        .collect();
    let mut unread: Vec<String> = listed
        .iter()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !named.contains(name))
        .collect();
    unread.sort();
    if let Some(stranger) = unread.first() {
        return Err((
            ErrorCode::PluginEntriesInvalid,
            format!(
                "`{}` holds `{}`, which names no locale this build ships: a plugin's texts are \
                 one file per locale, named `{}`",
                texts.display(),
                stranger.escape_debug(),
                named.join("`, `")
            ),
        ));
    }
    let mut read = Vec::with_capacity(Locale::ALL.len());
    for (&locale, name) in Locale::ALL.iter().zip(&named) {
        read.push((locale, bounded(&texts.join(name), user)?));
    }
    let handed: Vec<(Locale, &str)> = read
        .iter()
        .map(|(locale, text)| (*locale, text.as_str()))
        .collect();
    PluginEntries::parse(&commands, &handed).map_err(|refused| {
        (
            refused.code(),
            format!("`{}`: {refused}", directory.display()),
        )
    })
}

/// The entries file at `path`, judged this user's alone and read as UTF-8,
/// no further than one byte past what a plugin's entries may hold.
fn bounded(path: &Path, user: &User) -> Result<String, (ErrorCode, String)> {
    nc_platform::judge(path, Kind::File, user).map_err(|refused| missing(path, refused))?;
    let most = PluginEntries::most_bytes();
    let unread = |error: std::io::Error| {
        (
            ErrorCode::DaemonProfileUnavailable,
            format!("`{}` could not be read: {error}", path.display()),
        )
    };
    let past = |bytes: u64| {
        (
            ErrorCode::PluginEntriesInvalid,
            format!(
                "`{}` holds {bytes} bytes, past the {most} a plugin's entries may hold in one \
                 file, and was not read",
                path.display()
            ),
        )
    };
    let file = std::fs::File::open(path).map_err(unread)?;
    let length = file.metadata().map_err(unread)?.len();
    let bound = u64::try_from(most).unwrap_or(u64::MAX);
    if length > bound {
        return Err(past(length));
    }
    let mut bytes = Vec::new();
    file.take(bound.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(unread)?;
    if bytes.len() > most {
        // **Grown since its length was asked**: the read stopped a byte past
        // the bound, and what it read is all this says of it.
        return Err(past(u64::try_from(bytes.len()).unwrap_or(u64::MAX)));
    }
    String::from_utf8(bytes).map_err(|_| {
        (
            ErrorCode::PluginEntriesInvalid,
            format!("`{}` is not UTF-8", path.display()),
        )
    })
}

/// An entries path judged and not taken: one that is not there is the
/// entries' own refusal, and anything else keeps the profile's code.
fn missing(path: &Path, refused: nc_platform::Judged) -> (ErrorCode, String) {
    match &refused {
        nc_platform::Judged::Failed(failed)
            if failed.error().kind() == std::io::ErrorKind::NotFound =>
        {
            (
                ErrorCode::PluginEntriesInvalid,
                format!(
                    "`{}` is not there: a plugin describes every command it declares, in \
                     every locale, beside its manifest",
                    path.display()
                ),
            )
        }
        _ => judged(refused),
    }
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
                refused: Some(kept(refused)),
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
        loaded.push(Loaded {
            name,
            refused: refused.map(kept),
        });
    }
    names_refused(&loaded, log);
    loaded
}

/// `refused`, its sentence kept to [`REFUSAL_BYTES_MAX`] bytes.
///
/// Cut on a character, never inside one, and ended with `…`, so what the
/// status shows is the refusal's opening, which names the plugin's file or
/// command and its first problem, and says that more followed.
fn kept((code, sentence): (ErrorCode, String)) -> (ErrorCode, String) {
    if sentence.len() <= REFUSAL_BYTES_MAX {
        return (code, sentence);
    }
    let mut end = REFUSAL_BYTES_MAX - '…'.len_utf8();
    while !sentence.is_char_boundary(end) {
        end -= 1;
    }
    (code, format!("{}…", &sentence[..end]))
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

        /// A plugin's directory under the scratch, holding `manifest`, the
        /// reference plugin's entries and `component` as an installer writes
        /// them.
        fn plugin(&self, name: &str, manifest: &str, component: &[u8]) -> PathBuf {
            let directory = self.0.join(name);
            nc_platform::create_private_directory(&directory).expect("a plugin's directory");
            nc_platform::create_private_directory(&directory.join(super::TEXTS))
                .expect("the texts' directory");
            for (file, text) in [
                (super::MANIFEST, manifest.as_bytes()),
                (super::COMMANDS, COMMANDS.as_bytes()),
                ("i18n/en.toml", ENGLISH.as_bytes()),
                ("i18n/pt-BR.toml", PORTUGUESE.as_bytes()),
                (super::COMPONENT, component),
            ] {
                written(&directory.join(file), text);
            }
            directory
        }
    }

    /// `bytes` written to a new file at `path`, this user's alone.
    fn written(path: &std::path::Path, bytes: &[u8]) {
        use std::io::Write as _;
        nc_platform::create_private(path)
            .and_then(|mut file| file.write_all(bytes))
            .expect("a file of the plugin's");
    }

    /// The reference plugin's manifest, as the plugin ships it.
    const REFERENCE: &str = include_str!("../../../plugins/reference/plugin.toml");

    /// The reference plugin's command table, as it ships it.
    const COMMANDS: &str = include_str!("../../../plugins/reference/commands.toml");

    /// The reference plugin's English texts.
    const ENGLISH: &str = include_str!("../../../plugins/reference/i18n/en.toml");

    /// The reference plugin's Portuguese texts.
    const PORTUGUESE: &str = include_str!("../../../plugins/reference/i18n/pt-BR.toml");

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
        assert_eq!(
            read.manifest.entries(),
            &nc_kernel::entries_for_tests("reference"),
            "the manifest is described by the entries beside it"
        );
        for (name, file) in [
            ("open-dir", None),
            ("open-manifest", Some(super::MANIFEST)),
            ("open-commands", Some(super::COMMANDS)),
            ("open-texts", Some(super::TEXTS)),
            ("open-english", Some("i18n/en.toml")),
            ("open-portuguese", Some("i18n/pt-BR.toml")),
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

    /// A plugin missing a file of its entries is refused naming the file.
    ///
    /// Its texts in one locale or its command table, each with the entries'
    /// code, and before its component is read.
    #[test]
    fn a_plugin_missing_a_locales_texts_is_refused_naming_the_file() {
        let scratch = Scratch::new("missing-texts");
        let user = scratch.user();
        for file in [
            &[super::TEXTS, "pt-BR.toml"][..],
            &[super::TEXTS, "en.toml"][..],
            &[super::COMMANDS][..],
        ] {
            let directory = scratch.plugin(&file.join("-"), REFERENCE, b"bytes");
            // Joined a component at a time, as the reader joins them, so the
            // path the refusal names is spelt with this system's separator.
            let path = file
                .iter()
                .fold(directory.clone(), |path, part| path.join(part));
            std::fs::remove_file(&path).expect("removed");
            std::fs::remove_file(directory.join(super::COMPONENT)).expect("removed");
            let (code, sentence) = super::installed(&directory, &user).expect_err("missing");
            assert_eq!(
                code,
                ErrorCode::PluginEntriesInvalid,
                "{file:?}: {sentence}"
            );
            assert!(
                sentence.contains(&format!("{}` is not there", path.display())),
                "{file:?}: {sentence}"
            );
        }
    }

    /// A file in `i18n/` named for no locale this build ships is refused,
    /// a tag written in another case among them, rather than left unread.
    #[test]
    fn a_file_in_i18n_named_for_no_locale_is_refused() {
        let scratch = Scratch::new("stranger");
        let user = scratch.user();
        for (at, stranger) in ["fr.toml", "pt-br.toml", "en.toml.orig", "notes"]
            .into_iter()
            .enumerate()
        {
            let directory = scratch.plugin(&format!("p{at}"), REFERENCE, b"bytes");
            if stranger.eq_ignore_ascii_case("pt-BR.toml") {
                // **One file to a system that folds case**, which a second
                // spelling beside the first could not be created on: the
                // Portuguese texts are written under the other spelling alone.
                std::fs::remove_file(directory.join(super::TEXTS).join("pt-BR.toml"))
                    .expect("removed");
            }
            written(&directory.join(super::TEXTS).join(stranger), b"");
            let (code, sentence) = super::installed(&directory, &user).expect_err(stranger);
            assert_eq!(code, ErrorCode::PluginEntriesInvalid, "{stranger}");
            assert!(
                sentence.contains(&format!("holds `{stranger}`, which names no locale"))
                    && sentence.contains("`en.toml`, `pt-BR.toml`"),
                "{stranger}: {sentence}"
            );
        }
    }

    /// An entries file another account can reach is refused for that, before
    /// a byte of it is read — one that would not parse included.
    #[test]
    fn an_entries_file_another_account_can_reach_is_refused_before_it_is_read() {
        let scratch = Scratch::new("reachable-entries");
        let user = scratch.user();
        for file in [super::COMMANDS, "i18n/pt-BR.toml"] {
            let directory = scratch.plugin(&file.replace('/', "-"), REFERENCE, b"bytes");
            std::fs::remove_file(directory.join(file)).expect("removed");
            written(&directory.join(file), b"not = [toml");
            nc_platform::testing::open_to_another_user(&directory.join(file)).expect("opened");
            let (code, sentence) = super::installed(&directory, &user).expect_err(file);
            assert_eq!(
                code,
                ErrorCode::DaemonProfileNotPrivate,
                "{file}: {sentence}"
            );
        }
    }

    /// An entries file past what a plugin's entries may hold is refused
    /// unread, and one at it is read and judged for its shape.
    #[test]
    fn an_entries_file_past_its_bound_is_refused_unread() {
        let scratch = Scratch::new("large-entries");
        let user = scratch.user();
        let most = nc_kernel::entries::PluginEntries::most_bytes();
        // **Past it by more than a byte**, so a refusal that read the file to
        // the bound and a byte more says a size other than the file's.
        for (bytes, refused_unread) in [(most + 64, true), (most, false)] {
            let directory = scratch.plugin(&format!("b{bytes}"), REFERENCE, b"bytes");
            let file = directory.join(super::COMMANDS);
            std::fs::remove_file(&file).expect("removed");
            let mut padded = COMMANDS.to_owned();
            padded.push_str(&"#".repeat(bytes - COMMANDS.len()));
            // Not UTF-8 past the bound, so a read that went past it would
            // say so rather than how large the file is.
            let mut written_bytes = padded.into_bytes();
            if refused_unread {
                written_bytes[most] = 0xff;
            }
            written(&file, &written_bytes);
            let (code, sentence) = match super::installed(&directory, &user) {
                Ok(_) => (None, String::new()),
                Err((code, sentence)) => (Some(code), sentence),
            };
            if refused_unread {
                assert_eq!(code, Some(ErrorCode::PluginEntriesInvalid), "{sentence}");
                assert!(
                    sentence.contains(&format!("holds {bytes} bytes, past the {most}"))
                        && sentence.contains("was not read"),
                    "{sentence}"
                );
            } else {
                assert_eq!(code, None, "a file at the bound is read: {sentence}");
            }
        }
    }

    /// An entries file that is not UTF-8 is refused naming it.
    ///
    /// The command table and one locale's texts, each within its bound and
    /// holding one byte no UTF-8 text holds, in a comment that a reader
    /// taking the bytes as nearly text would parse past.
    #[test]
    fn an_entries_file_that_is_not_utf8_is_refused_naming_it() {
        let scratch = Scratch::new("not-utf8");
        let user = scratch.user();
        for (file, text) in [
            (&[super::COMMANDS][..], COMMANDS),
            (&[super::TEXTS, "pt-BR.toml"][..], PORTUGUESE),
        ] {
            let directory = scratch.plugin(&file.join("-"), REFERENCE, b"bytes");
            // Joined a component at a time, as the reader joins them.
            let path = file
                .iter()
                .fold(directory.clone(), |path, part| path.join(part));
            std::fs::remove_file(&path).expect("removed");
            let mut bytes = text.as_bytes().to_vec();
            bytes.extend_from_slice(b"\n# \xff\n");
            written(&path, &bytes);
            let (code, sentence) = super::installed(&directory, &user).expect_err("not UTF-8");
            assert_eq!(
                code,
                ErrorCode::PluginEntriesInvalid,
                "{file:?}: {sentence}"
            );
            assert_eq!(sentence, format!("`{}` is not UTF-8", path.display()));
        }
    }

    /// What the status keeps of a refusal past its bound is its opening.
    ///
    /// Cut on a character within the bound and ended with `…`; a refusal
    /// within the bound is kept whole.
    #[test]
    fn a_refusal_is_kept_to_its_bound_cut_on_a_character() {
        let most = super::REFUSAL_BYTES_MAX;
        let code = ErrorCode::PluginEntriesInvalid;
        for whole in ["a refusal".to_owned(), "a".repeat(most)] {
            assert_eq!(super::kept((code, whole.clone())), (code, whole));
        }
        // **A character across the cut**: `é` is two bytes, and one lies
        // where the cut would fall on every byte boundary the bound leaves.
        for pad in 0..4 {
            let sentence = format!("{}{}", "a".repeat(most - 4 + pad), "é".repeat(most));
            let (kept_code, kept) = super::kept((code, sentence.clone()));
            assert_eq!(kept_code, code);
            assert!(kept.len() <= most, "{pad}: {} bytes kept", kept.len());
            assert!(
                kept.len() > most - 8,
                "{pad}: only {} bytes kept",
                kept.len()
            );
            let opening = kept.strip_suffix('…').expect("ended with an ellipsis");
            assert!(sentence.starts_with(opening), "{pad}: not the opening");
        }
    }

    /// An entries file within its bound whose every row is malformed is
    /// refused in a sentence past a frame, and what the status keeps of it is
    /// within the bound.
    ///
    /// **The file a hostile plugin would write**: each malformed name is a
    /// problem the entries' refusal names, so the sentence grows with the
    /// file, while the file is one the daemon reads.
    #[test]
    fn an_entries_file_of_malformed_rows_is_refused_within_what_the_status_keeps() {
        let scratch = Scratch::new("malformed-rows");
        let user = scratch.user();
        let most = nc_kernel::entries::PluginEntries::most_bytes();
        let directory = scratch.plugin("rows", REFERENCE, b"bytes");
        let file = directory.join(super::COMMANDS);
        std::fs::remove_file(&file).expect("removed");
        let row = "{name=\"A\"},";
        let mut table = String::from("command=[");
        while table.len() + row.len() < most {
            table.push_str(row);
        }
        table.push(']');
        written(&file, table.as_bytes());
        let (code, sentence) = super::installed(&directory, &user).expect_err("malformed");
        assert_eq!(code, ErrorCode::PluginEntriesInvalid);
        let frame = usize::try_from(nc_catalog::DAEMON.ipc_frame_max_bytes).expect("fits");
        assert!(
            sentence.len() > frame,
            "the refusal holds {} bytes, which a frame of {frame} carries: the case is not \
             the one a bound is needed for",
            sentence.len()
        );
        let (_, kept) = super::kept((code, sentence.clone()));
        assert!(
            kept.len() <= super::REFUSAL_BYTES_MAX,
            "{} kept",
            kept.len()
        );
        assert!(
            sentence.starts_with(kept.trim_end_matches('…')),
            "what is kept is not the refusal's opening"
        );
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
