//! The `checkouts` verb: the development checkouts installed beside this
//! install, listed, and one chosen — `noctua-dev` pointed at its CLI and that
//! CLI's path copied to the clipboard (`NOCTUA.md §17.1`, `§20.2`).
//!
//! **With no daemon**: the checkouts are found on disk, by the one listing the
//! Leader settlement reads too (`nc_channel::checkouts`), under this install's
//! bases and its namespace.
//!
//! - **At a terminal, with no `--json`**, the picker ([`crate::picker`]) opens
//!   in [`crate::terminal`], the checkout installed most recently first: Enter
//!   chooses, and Esc or Ctrl-C leaves, which ends in an exit code of its own
//!   with nothing changed.
//! - **With `--json`, or with no terminal**, the list is one JSON document,
//!   and nothing opens.
//! - **With a digest**, that checkout is chosen with no picker, for a script
//!   or an agent; one not installed here is refused, naming it.
//!
//! **Choosing** points `noctua-dev` at the checkout's CLI ([`crate::chosen`]),
//! where the checkout installed most recently records it lives, copies that
//! CLI's path to the clipboard through the system's own tool
//! (`nc_platform::clipboard`), and prints the path on standard output. A copy
//! that fails undoes nothing: the path is printed all the same, and why it was
//! not copied is said beside it. **Every text from disk is escaped for the
//! terminal** ([`crate::render`]).

use std::path::{Path, PathBuf};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_channel::Checkout;
use nc_channel::checkouts::{Listed, Recorded, pointed_at};
use nc_channel::found::Found;

use crate::exit::Exit;
use crate::picker::{Entry, Picker};
use crate::render::{self, Env, Failure, Outcome, escaped};

/// What a checkout that records no directory says in its place.
pub const UNRECORDED: &str = "records no directory: it was installed before checkouts recorded one, and installing it \
     again records it";

/// What `checkouts` comes to for `found`'s install: the list, the picker, or
/// the checkout `digest` names chosen.
#[must_use]
pub fn answered(found: &Found, json: bool, env: &Env, digest: Option<&str>) -> Outcome {
    answer(found, json, env, digest, crate::terminal::Crossterm::new)
        .unwrap_or_else(|failure| render::failed(json, env.locale(), &failure))
}

/// What [`answered`] answers, or the failure it ends in, with the picker
/// opened in the terminal `terminal` makes.
fn answer<T: crate::terminal::Terminal>(
    found: &Found,
    json: bool,
    env: &Env,
    digest: Option<&str>,
    terminal: impl FnOnce() -> T,
) -> Result<Outcome, Failure> {
    let cli = found.layout.cli.as_str();
    let listed =
        nc_channel::checkouts::listed(&found.install, &found.user).map_err(|unlisted| Failure {
            code: ErrorCode::CliCheckoutsUnlisted,
            exit: Exit::Failed,
            detail: format!("the development checkouts were not listed: {unlisted}"),
        })?;
    let link = crate::install::newest_link(&listed);
    if let Some(digest) = digest {
        return chosen(
            found,
            json,
            env,
            named(&listed, digest, cli)?,
            link.as_deref(),
        );
    }
    let now = link
        .as_deref()
        .and_then(|link| crate::chosen::pointed(link, nc_platform::REACH, &found.user).ok())
        .flatten()
        .and_then(|target| pointed_at(&target, &listed));
    if json || !env.terminal() {
        return Ok(render::answered(
            json,
            &listing(&listed, link.as_deref(), now),
        ));
    }
    let ordered = newest_first(&listed);
    let mut picker = Picker::new(entries(&ordered, now));
    match crate::terminal::picked(&mut terminal(), &mut picker) {
        Ok(Some(index)) => chosen(found, json, env, ordered[index], link.as_deref()),
        Ok(None) => Ok(Outcome {
            exit: Exit::Left,
            stdout: String::new(),
            stderr: "nothing was chosen, and nothing changed\n".to_owned(),
        }),
        Err(error) => Err(Failure {
            code: ErrorCode::CliTerminalUnanswered,
            exit: Exit::Failed,
            detail: format!(
                "the terminal did not answer: {error}; nothing was chosen, and `{cli} checkouts \
                 <checkout>` chooses one without it"
            ),
        }),
    }
}

/// The checkout of `listed` whose digest `digest` writes.
fn named<'a>(listed: &'a [Listed], digest: &str, cli: &str) -> Result<&'a Listed, Failure> {
    let checkout = Checkout::parse(digest).map_err(|_| Failure {
        code: ErrorCode::CliCommandLineUnreadable,
        exit: Exit::Invalid,
        detail: format!(
            "`{}` is not a checkout's digest, which is eight lower-case hexadecimal digits; \
             `{cli} checkouts` lists them",
            escaped(digest)
        ),
    })?;
    listed
        .iter()
        .find(|one| one.checkout == checkout)
        .ok_or_else(|| Failure {
            code: ErrorCode::CliCheckoutUnknown,
            exit: Exit::NotFound,
            detail: format!(
                "no development checkout `{checkout}` is installed beside this install, so nothing \
                 was chosen; `{cli} checkouts` lists those that are"
            ),
        })
}

/// `listed`, the checkout installed most recently first.
fn newest_first(listed: &[Listed]) -> Vec<&Listed> {
    let mut ordered: Vec<&Listed> = listed.iter().collect();
    ordered.sort_by(|left, right| {
        right
            .installed
            .cmp(&left.installed)
            .then(left.checkout.cmp(&right.checkout))
    });
    ordered
}

/// The picker's entries for `ordered`, the one `noctua-dev` reaches `now`
/// marked.
fn entries(ordered: &[&Listed], now: Option<Checkout>) -> Vec<Entry> {
    ordered
        .iter()
        .map(|one| Entry {
            digest: one.checkout.to_string(),
            directory: directory_of(one).map(|directory| directory.to_string_lossy().into_owned()),
            note: note_of(one),
            chosen: now == Some(one.checkout),
        })
        .collect()
}

/// The directory `one` records, when it records one.
fn directory_of(one: &Listed) -> Option<&Path> {
    match &one.recorded {
        Recorded::Read(record) => Some(&record.directory),
        Recorded::Absent | Recorded::Refused(_) => None,
    }
}

/// What is said of `one` where its directory would be.
fn note_of(one: &Listed) -> String {
    match &one.recorded {
        Recorded::Read(_) => String::new(),
        Recorded::Absent => UNRECORDED.to_owned(),
        Recorded::Refused(why) => format!("its record was not read: {why}"),
    }
}

/// The list as one JSON value: each checkout's digest, directory, CLI, when
/// it was installed and whether `noctua-dev` reaches it `now`, and where it lives.
#[must_use]
pub fn listing(listed: &[Listed], link: Option<&Path>, now: Option<Checkout>) -> Value {
    let text_or_null =
        |path: Option<&Path>| path.map_or(Value::Null, |path| Value::text(path.to_string_lossy()));
    Value::object([
        (
            "checkouts",
            Value::Array(
                listed
                    .iter()
                    .map(|one| {
                        let note = note_of(one);
                        Value::object([
                            ("checkout", Value::text(one.checkout.to_string())),
                            ("chosen", Value::Bool(now == Some(one.checkout))),
                            ("cli", Value::text(one.cli.to_string_lossy())),
                            ("directory", text_or_null(directory_of(one))),
                            (
                                "installed",
                                one.installed.map_or(Value::Null, Value::Integer),
                            ),
                            (
                                "note",
                                if note.is_empty() {
                                    Value::Null
                                } else {
                                    Value::text(note)
                                },
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("noctua_dev", text_or_null(link)),
    ])
}

/// `one` chosen for `found`'s install: `noctua-dev` at `link` pointed at the
/// checkout's CLI, the path copied, and the path printed.
fn chosen(
    found: &Found,
    json: bool,
    env: &Env,
    one: &Listed,
    link: Option<&Path>,
) -> Result<Outcome, Failure> {
    let not_pointed = |detail: String| Failure {
        code: ErrorCode::CliChosenNotPointed,
        exit: Exit::Failed,
        detail,
    };
    let link = link.ok_or_else(|| {
        not_pointed(
            "no installed checkout records where `noctua-dev` lives, so it was not pointed: \
             install one again with `cargo xtask dev --install`"
                .to_owned(),
        )
    })?;
    // **The launcher a missing one is copied from is the CLI that runs
    // now**, which knows how to launch; a checkout installed before the
    // launcher existed would not.
    let launcher = running().unwrap_or_else(|| one.cli.clone());
    crate::chosen::point(
        link,
        &one.cli,
        &found.install,
        nc_platform::REACH,
        &launcher,
        &found.user,
    )
    .map_err(|unpointed| not_pointed(unpointed.to_string()))?;
    let path = one.cli.to_string_lossy();
    let copied = nc_platform::clipboard::copy(&path, env.clipboard());
    Ok(said_chosen(json, one, link, &copied))
}

/// The executable running now, canonicalized.
fn running() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    std::fs::canonicalize(&executable).ok()
}

/// What a choice of `one` says: its CLI's path on standard output, and on
/// standard error what `noctua-dev` at `link` reaches now and whether the path
/// was `copied` — or, with `--json`, all of it as one document.
fn said_chosen(
    json: bool,
    one: &Listed,
    link: &Path,
    copied: &Result<String, nc_platform::clipboard::Uncopied>,
) -> Outcome {
    let path = one.cli.to_string_lossy();
    if json {
        let (tool, why) = match copied {
            Ok(tool) => (Value::text(tool.as_str()), Value::Null),
            Err(uncopied) => (Value::Null, Value::text(uncopied.to_string())),
        };
        return render::answered(
            true,
            &Value::object([
                ("checkout", Value::text(one.checkout.to_string())),
                ("cli", Value::text(path)),
                ("copied_by", tool),
                ("noctua_dev", Value::text(link.to_string_lossy())),
                ("not_copied", why),
            ]),
        );
    }
    let copy = match copied {
        Ok(tool) => format!(
            "the path is copied to the clipboard, by `{}`",
            escaped(tool)
        ),
        Err(uncopied) => format!(
            "the path was not copied: {}",
            escaped(&uncopied.to_string())
        ),
    };
    Outcome {
        exit: Exit::Success,
        stdout: format!("{}\n", escaped(&path)),
        stderr: format!(
            "`{}` now reaches the development checkout {}; {copy}\n",
            escaped(&link.to_string_lossy()),
            one.checkout
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use nc_canon::Value;
    use nc_catalog::ErrorCode;

    use crate::exit::Exit;
    use crate::install::{Chosen, Installed, Plan};
    use crate::picker::{Entry, Key};
    use crate::render::Env;

    /// A scratch home for one test, removed however the test leaves.
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
                std::env::temp_dir().join(format!("nc-checkouts-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        fn file(&self, name: &str, text: &str) -> PathBuf {
            use std::io::Write as _;
            let path = self.0.join(name);
            nc_platform::create_private(&path)
                .and_then(|mut file| file.write_all(text.as_bytes()))
                .expect("a file");
            path
        }

        fn bin(&self) -> PathBuf {
            self.0.join("home").join(".local").join("bin")
        }

        /// The checkout `digest` installed under `namespace` at `at`, with its
        /// name in this scratch's bin directory.
        fn installed(&self, digest: &str, namespace: &str, at: i64) -> Installed {
            let base = |name: &str| self.0.join("home").join(name);
            let install = nc_channel::Install {
                channel: nc_channel::Channel::Development(
                    nc_channel::Checkout::parse(digest).expect("a digest"),
                ),
                bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                    .expect("plain"),
                namespace: Some(nc_channel::Namespace::parse(namespace).expect("a namespace")),
                log_endpoint: None,
            };
            let (cli, daemon) = (self.0.join("cli"), self.0.join("daemon"));
            if !cli.exists() {
                self.file("cli", "c");
                self.file("daemon", "d");
            }
            let plan = Plan {
                install,
                cli: &cli,
                daemon: &daemon,
                world: "{}",
                plugins: Vec::new(),
                restorer: false,
                chosen: Some(Chosen {
                    bin: self.bin(),
                    directory: self.0.join(format!("checkout-{digest}")),
                    installed: at,
                }),
            };
            crate::install::install_with(&plan, |_: &Path, _: &Path| Ok(())).expect("installed")
        }
    }

    /// An environment at no terminal, with no clipboard: what a suite runs in.
    fn quiet() -> Env {
        Env::of(|_| None)
    }

    fn document(text: &str) -> Value {
        nc_canon::parse(text.trim_end()).expect("one JSON document")
    }

    /// With no terminal, or with `--json`, the list is one JSON document.
    ///
    /// Each checkout's digest, directory, CLI, install time and whether
    /// `noctua-dev` reaches it, where `noctua-dev` lives — and a checkout that
    /// records no directory is listed, saying how to record one.
    #[test]
    fn the_listing_is_one_document_and_a_checkout_with_no_record_is_listed() {
        let scratch = Scratch::new("listed");
        let one = scratch.installed("0000000a", "tl2", 1_000);
        let other = scratch.installed("0000000b", "tl2", 2_000);
        let record = one.found.layout.checkout_record.clone().expect("a record");
        std::fs::remove_file(&record).expect("an install from before records");
        let link = other.link.clone().expect("pointed");
        for json in [true, false] {
            let outcome = super::answered(&other.found, json, &quiet(), None);
            assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
            let listed = if json {
                document(&outcome.stdout)
                    .into_object()
                    .and_then(|mut fields| fields.remove("ok"))
                    .expect("an answer")
            } else {
                document(&outcome.stdout)
            };
            let text = |path: &Path| Value::text(path.to_string_lossy());
            assert_eq!(
                listed,
                Value::object([
                    (
                        "checkouts",
                        Value::Array(vec![
                            Value::object([
                                ("checkout", Value::text("0000000a")),
                                ("chosen", Value::Bool(false)),
                                ("cli", text(&one.cli)),
                                ("directory", Value::Null),
                                ("installed", listed_time(&listed, 0)),
                                ("note", Value::text(super::UNRECORDED)),
                            ]),
                            Value::object([
                                ("checkout", Value::text("0000000b")),
                                ("chosen", Value::Bool(true)),
                                ("cli", text(&other.cli)),
                                ("directory", text(&scratch.0.join("checkout-0000000b"))),
                                ("installed", Value::Integer(2_000)),
                                ("note", Value::Null),
                            ]),
                        ]),
                    ),
                    ("noctua_dev", text(&link)),
                ])
            );
            assert!(
                matches!(listed_time(&listed, 0), Value::Integer(at) if at > 2_000),
                "a checkout with no record is dated by its manifest"
            );
        }
    }

    /// The picker lists the checkout installed most recently first, and marks
    /// the one `noctua-dev` reaches and no other.
    ///
    /// **A checkout that records nothing is placed by its manifest's last
    /// write**, among the others by their records, and says how to record one.
    #[test]
    fn the_picker_lists_the_checkout_installed_last_first_and_marks_the_one_reached() {
        let scratch = Scratch::new("ordered");
        let newest = scratch.installed("0000000a", "tl5", 3_000);
        scratch.installed("0000000b", "tl5", 1_000);
        let unrecorded = scratch.installed("0000000c", "tl5", 2_000);
        let layout = &unrecorded.found.layout;
        std::fs::remove_file(layout.checkout_record.as_ref().expect("a record"))
            .expect("an install from before records");
        let between = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_500);
        nc_platform::set_modified(&layout.manifest, between).expect("dated");
        let user = &newest.found.user;
        let listed = nc_channel::checkouts::listed(&newest.found.install, user).expect("listed");
        let link = crate::install::newest_link(&listed).expect("where the name lives");
        let now = crate::chosen::pointed(&link, nc_platform::REACH, user)
            .expect("read")
            .and_then(|target| nc_channel::checkouts::pointed_at(&target, &listed));
        assert_eq!(
            now,
            nc_channel::Checkout::parse("0000000c").ok(),
            "the name reaches the checkout installed last"
        );
        let directory = |digest: &str| {
            Some(
                scratch
                    .0
                    .join(format!("checkout-{digest}"))
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        let entry = |digest: &str, directory: Option<String>, note: &str, chosen: bool| Entry {
            digest: digest.to_owned(),
            directory,
            note: note.to_owned(),
            chosen,
        };
        assert_eq!(
            super::entries(&super::newest_first(&listed), now),
            [
                entry("0000000a", directory("0000000a"), "", false),
                entry("0000000c", None, super::UNRECORDED, true),
                entry("0000000b", directory("0000000b"), "", false),
            ]
        );
    }

    /// The install time listed for the checkout at `at`.
    fn listed_time(listed: &Value, at: usize) -> Value {
        match listed {
            Value::Object(fields) => match fields.get("checkouts") {
                Some(Value::Array(all)) => match &all[at] {
                    Value::Object(one) => one.get("installed").cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                },
                _ => Value::Null,
            },
            _ => Value::Null,
        }
    }

    /// Choosing by digest points `noctua-dev` at that checkout's CLI and
    /// prints its path; a copy that fails undoes nothing, and says why.
    #[test]
    fn choosing_by_digest_points_noctua_dev_and_prints_the_path() {
        let scratch = Scratch::new("chosen");
        let one = scratch.installed("0000000a", "tl3", 1_000);
        let other = scratch.installed("0000000b", "tl3", 2_000);
        let link = other.link.clone().expect("pointed");
        let user = &one.found.user;
        let no_tool = quiet().with_the_clipboard(vec![nc_platform::clipboard::Tool {
            program: "nc-no-such-clipboard-tool".to_owned(),
            arguments: Vec::new(),
            environment: Vec::new(),
        }]);
        let outcome = super::answered(&other.found, false, &no_tool, Some("0000000a"));
        assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
        assert_eq!(outcome.stdout, format!("{}\n", one.cli.display()));
        assert!(
            outcome.stderr.contains("the path was not copied")
                && outcome.stderr.contains("nc-no-such-clipboard-tool"),
            "{}",
            outcome.stderr
        );
        assert_eq!(
            crate::chosen::pointed(&link, nc_platform::REACH, user),
            Ok(Some(one.cli.clone())),
            "a copy that failed undid the choice"
        );
        let json = super::answered(&one.found, true, &quiet(), Some("0000000b"));
        assert_eq!(json.exit, Exit::Success, "{json:?}");
        let answer = document(&json.stdout)
            .into_object()
            .and_then(|mut fields| fields.remove("ok"))
            .and_then(Value::into_object)
            .expect("an answer");
        assert_eq!(
            answer.get("cli"),
            Some(&Value::text(other.cli.to_string_lossy()))
        );
        assert_eq!(answer.get("copied_by"), Some(&Value::Null));
        assert_eq!(
            crate::chosen::pointed(&link, nc_platform::REACH, user),
            Ok(Some(other.cli.clone()))
        );
    }

    /// A digest no installed checkout has is refused, naming it, and one that
    /// is not a digest is a command line not read; neither changes anything.
    #[test]
    fn an_unknown_digest_is_refused_naming_it() {
        let scratch = Scratch::new("unknown");
        let one = scratch.installed("0000000a", "tl4", 1_000);
        let link = one.link.clone().expect("pointed");
        let unknown = super::answered(&one.found, false, &quiet(), Some("0000000f"));
        assert_eq!(unknown.exit, Exit::NotFound);
        assert!(
            unknown
                .stderr
                .contains(ErrorCode::CliCheckoutUnknown.code())
                && unknown.stderr.contains("0000000f"),
            "{}",
            unknown.stderr
        );
        let malformed = super::answered(&one.found, false, &quiet(), Some("\u{1b}[2J"));
        assert_eq!(malformed.exit, Exit::Invalid);
        assert!(!malformed.stderr.contains('\u{1b}'), "{}", malformed.stderr);
        assert_eq!(
            crate::chosen::pointed(&link, nc_platform::REACH, &one.found.user),
            Ok(Some(one.cli.clone()))
        );
    }

    /// A terminal that hands out the keys it holds in order, draws nowhere,
    /// and goes away past the last.
    struct Pressed(std::vec::IntoIter<Key>);

    impl crate::terminal::Terminal for Pressed {
        fn enter(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn leave(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn size(&mut self) -> std::io::Result<(usize, usize)> {
            Ok((80, 24))
        }

        fn key(&mut self) -> std::io::Result<Key> {
            self.0
                .next()
                .ok_or_else(|| std::io::Error::other("the terminal went away"))
        }

        fn draw(&mut self, _: &[String]) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// At a terminal the picker's ending is the verb's: Esc and Ctrl-C leave
    /// in an exit code of their own, with `noctua-dev` where it was.
    ///
    /// Down then Enter chooses the checkout installed before the last, prints
    /// its CLI's path and points `noctua-dev` at it; a terminal that goes away
    /// chooses nothing, and says so in its own code.
    #[test]
    fn at_a_terminal_the_picker_leaves_in_its_own_code_or_chooses_the_checkout_selected() {
        let scratch = Scratch::new("picked");
        let older = scratch.installed("0000000a", "tl6", 1_000);
        let newest = scratch.installed("0000000b", "tl6", 2_000);
        let link = newest.link.clone().expect("pointed");
        let user = &newest.found.user;
        let reached = || crate::chosen::pointed(&link, nc_platform::REACH, user);
        let at_a_terminal = quiet().at_a_terminal(true);
        let pressed = |keys: &[Key]| {
            let keys = keys.to_vec();
            move || Pressed(keys.into_iter())
        };
        let picked =
            |keys: &[Key]| super::answer(&newest.found, false, &at_a_terminal, None, pressed(keys));
        for leave in [Key::Escape, Key::Interrupt] {
            let left = picked(&[Key::Down, leave]).expect("left");
            assert_eq!((left.exit, left.code()), (Exit::Left, 13), "{left:?}");
            assert_eq!(left.stdout, "", "{left:?}");
            assert_eq!(
                reached(),
                Ok(Some(newest.cli.clone())),
                "leaving the picker changed `noctua-dev`"
            );
        }
        let chose = picked(&[Key::Down, Key::Enter]).expect("chosen");
        assert_eq!(chose.exit, Exit::Success, "{chose:?}");
        assert_eq!(chose.stdout, format!("{}\n", older.cli.display()));
        assert_eq!(reached(), Ok(Some(older.cli.clone())));
        let gone = picked(&[]).expect_err("a terminal that went away");
        assert_eq!(
            (gone.code, gone.exit),
            (ErrorCode::CliTerminalUnanswered, Exit::Failed)
        );
        assert_eq!(
            reached(),
            Ok(Some(older.cli.clone())),
            "a terminal that went away changed `noctua-dev`"
        );
    }
}
