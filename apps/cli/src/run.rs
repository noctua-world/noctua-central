//! One run of the CLI: a command line read, the daemon asked, and how it
//! ended rendered (`NOCTUA.md §20.2`).
//!
//! **In this order, and each step refuses before the next one starts**: the
//! command line is read before anything is asked ([`crate::args::read`]);
//! help on the exit codes and the verbs is answered without a daemon, and help
//! on everything shows the grammar and the verbs when no registry can be read,
//! beside the refusal and in its exit code; for anything else the daemon is
//! reached and proven ([`crate::client`]), its registry read, the command
//! found there and its arguments read against the schema it lists and held to
//! it, and only then is the command dispatched — so a command line the CLI
//! cannot read, a command the registry does not hold and arguments the schema
//! refuses each reach no kernel.
//!
//! **The CLI acts as the profile's person and confirms nothing** (`§11.1`):
//! a command that cannot be undone is refused by the kernel with
//! `NC-KERN-0006`, which ends the run in its own exit code, because what it
//! needs is a person in the Noctua window.

use std::ffi::OsString;

use nc_catalog::{ErrorCode, Locale};
use nc_channel::found::Found;
use nc_schema::Schema;

use crate::args::{self, Asked, Read, Topic};
use crate::client::{self, Connection, Listed, Unasked};
use crate::exit::{self, Exit};
use crate::help;
use crate::render::{self, Env, Failure, Outcome};

/// Runs the command line `argv` — without the program's name — against the
/// daemon of `found`'s install, in the language `env` names.
#[must_use]
pub fn run(argv: &[OsString], found: &Found, env: &Env) -> Outcome {
    let locale = env.locale();
    let json = asks_for_json(argv);
    let words = match words_of(argv) {
        Ok(words) => words,
        Err(failure) => return render::failed(json, locale, &failure),
    };
    let line = match args::read(&words) {
        Ok(line) => line,
        Err(unread) => return render::failed(json, locale, &refused_by_the_cli(&unread)),
    };
    let ended = match line.asked {
        Asked::Help(Topic::ExitCodes) => Ok(render::shown(line.json, exit::table())),
        Asked::Help(Topic::Verbs) => Ok(render::shown(line.json, help::verbs())),
        Asked::Help(Topic::Everything) => {
            on_a_runtime(found, line.json, env, Asked::Help(Topic::Everything))
                .or_else(|failure| Ok(unlisted(found, line.json, locale, &failure)))
        }
        asked => on_a_runtime(found, line.json, env, asked),
    };
    ended.unwrap_or_else(|failure| render::failed(line.json, locale, &failure))
}

/// Help on everything with no registry to read: the grammar and the verbs, which
/// need no daemon, on standard output, and the refusal, in its code, on standard error.
fn unlisted(found: &Found, json: bool, locale: Locale, failure: &Failure) -> Outcome {
    let refused = render::failed(json, locale, failure);
    if json {
        // `--json` writes one document, and a program reads the refusal.
        return refused;
    }
    Outcome {
        stdout: help::unlisted(&found.layout.cli),
        ..refused
    }
}

/// Whether the options before the command ask for `--json`, read before the
/// rest of the line is, so a refusal of the line is rendered as it asks.
pub(crate) fn asks_for_json(argv: &[OsString]) -> bool {
    argv.iter()
        .take_while(|word| word.to_str().is_some_and(|word| word.starts_with('-')))
        .any(|word| word == "--json")
}

/// The command line as text, each word whole.
fn words_of(argv: &[OsString]) -> Result<Vec<String>, Failure> {
    argv.iter()
        .map(|word| {
            word.to_str().map(str::to_owned).ok_or_else(|| Failure {
                code: ErrorCode::CliCommandLineUnreadable,
                exit: Exit::Invalid,
                detail: format!(
                    "`{}` is not text this CLI reads: a command line is UTF-8",
                    render::escaped(&word.to_string_lossy())
                ),
            })
        })
        .collect()
}

/// A command line the CLI refused, as a failure.
fn refused_by_the_cli(unread: &args::Unread) -> Failure {
    Failure {
        code: unread.code(),
        exit: Exit::Invalid,
        detail: unread.detail().to_owned(),
    }
}

/// The failure a step that asked nothing, or read no answer, ends in.
fn unasked(unasked: &Unasked) -> Failure {
    Failure {
        code: unasked.code(),
        exit: unasked.exit(),
        detail: unasked.detail(),
    }
}

/// What `asked` comes to, on a runtime of its own for the one connection it
/// makes.
fn on_a_runtime(found: &Found, json: bool, env: &Env, asked: Asked) -> Result<Outcome, Failure> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| unasked(&Unasked::Runtime(error)))?;
    runtime.block_on(answered(found, json, env, asked))
}

/// What `asked` comes to, over one connection to the daemon.
async fn answered(found: &Found, json: bool, env: &Env, asked: Asked) -> Result<Outcome, Failure> {
    let locale = env.locale();
    let cli = found.layout.cli.as_str();
    let mut connection = client::connect(found)
        .await
        .map_err(|refused| unasked(&refused))?;
    let registry = connection
        .registry()
        .await
        .map_err(|refused| unasked(&refused))?;
    match asked {
        Asked::Help(Topic::Command(name)) => {
            let (listed, schema) = registered(&registry, &name, cli)?;
            Ok(render::shown(
                json,
                help::command(cli, listed, &schema, locale),
            ))
        }
        Asked::Command { name, words } => {
            let (listed, schema) = registered(&registry, &name, cli)?;
            match args::arguments(&schema, &name, &words) {
                Err(unread) => Err(refused_by_the_cli(&unread)),
                Ok(Read::Help) => Ok(render::shown(
                    json,
                    help::command(cli, listed, &schema, locale),
                )),
                Ok(Read::Arguments(value)) => dispatched(&mut connection, &name, value, json).await,
            }
        }
        Asked::Help(_) => Ok(render::shown(
            json,
            help::everything(cli, &registry, locale),
        )),
    }
}

/// The entry `name` has in `registry`, and its schema compiled.
fn registered<'a>(
    registry: &'a [Listed],
    name: &str,
    cli: &str,
) -> Result<(&'a Listed, Schema), Failure> {
    let listed = registry
        .iter()
        .find(|listed| listed.name == name)
        .ok_or_else(|| Failure {
            code: ErrorCode::KernelUnknownCommand,
            exit: Exit::NotFound,
            detail: format!(
                "the daemon's registry holds no command `{name}`; `{cli} help` lists every one it \
                 holds"
            ),
        })?;
    let schema = Schema::compile(&listed.args_schema).map_err(|refused| {
        unasked(&Unasked::Unread(format!(
            "the registry lists `{name}` with an argument schema this build does not read: \
             {refused}"
        )))
    })?;
    Ok((listed, schema))
}

/// The command `name` run with `value`, and its answer rendered.
async fn dispatched(
    connection: &mut Connection,
    name: &str,
    value: nc_canon::Value,
    json: bool,
) -> Result<Outcome, Failure> {
    match connection
        .dispatch(name, value)
        .await
        .map_err(|refused| unasked(&refused))?
    {
        Ok(payload) => Ok(render::answered(json, &payload)),
        Err(refusal) => Err(Failure {
            code: refusal.code,
            exit: Exit::of_refusal(&refusal),
            detail: refusal.sentence,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use nc_canon::Value;
    use nc_catalog::{ErrorCode, Locale};
    use nc_ipc::{Answer, Refusal, RefusalKind, Request, Token};

    use crate::client::scripted::{Scratch, Script, Scripted, entry};
    use crate::exit::Exit;
    use crate::render::Env;

    fn argv(line: &[&str]) -> Vec<OsString> {
        line.iter().map(OsString::from).collect()
    }

    fn english() -> Env {
        Env::of(|_| None)
    }

    const VERIFY: &str = r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0}},"additionalProperties":false}"#;

    /// The registry a scripted daemon lists: one catalog command, and one
    /// irreversible command of a plugin's taking a required note.
    fn registry() -> Answer {
        Answer::Registry(vec![
            entry("journal.verify", "command.journal.verify.title", VERIFY),
            entry(
                "reference.irreversible.do",
                "Do something that cannot be undone",
                r#"{"type":"object","additionalProperties":false,
                    "properties":{"note":{"type":"string","maxLength":128,"x-entity":"Content"}},
                    "required":["note"]}"#,
            ),
        ])
    }

    /// An install whose daemon answers each connection as `connections` says,
    /// after its registry.
    fn serving(tag: &str, connections: Vec<Vec<Answer>>) -> (Scratch, Scripted) {
        let installed = Scratch::new(tag);
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        let daemon = Scripted::serve(
            &installed,
            Script {
                token,
                welcome: installed.welcome(),
                connections: connections
                    .into_iter()
                    .map(|answers| std::iter::once(registry()).chain(answers).collect())
                    .collect(),
            },
        );
        (installed, daemon)
    }

    fn refusal(code: ErrorCode, kind: RefusalKind) -> Refusal {
        Refusal {
            code,
            kind,
            sentence: format!("refused as {}", kind.word()),
        }
    }

    /// The exit codes and the verbs are answered with no daemon to ask: no
    /// token, no listener, and still exit 0 with the table itself.
    #[test]
    fn help_on_the_exit_codes_and_the_verbs_needs_no_daemon() {
        let installed = Scratch::new("nod");
        let outcome = super::run(&argv(&["help", "exit-codes"]), &installed.found, &english());
        assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
        assert_eq!(outcome.stdout, crate::exit::table());
        let outcome = super::run(
            &argv(&["--json", "help", "exit-codes"]),
            &installed.found,
            &english(),
        );
        assert_eq!(
            outcome.stdout,
            format!(
                "{}\n",
                Value::object([("ok", Value::text(crate::exit::table()))]).to_canonical()
            )
        );
        let outcome = super::run(&argv(&["help", "help"]), &installed.found, &english());
        assert_eq!(outcome.stdout, crate::help::verbs());
    }

    /// Help on everything with no registry to read still shows the grammar
    /// and the verbs, and ends in the refusal's own code.
    ///
    /// Nothing listening is 9 and a handshake proving nobody 10, as a command
    /// would end; under `--json` the one document is the refusal. A person
    /// whose daemon is down reads the grammar this way, the way a verb that
    /// starts the daemon will be found.
    #[test]
    fn help_with_no_daemon_shows_the_grammar_and_the_verbs() {
        let installed = Scratch::new("hnd");
        installed.token(&Token::fresh().expect("a token"));
        let grammar = crate::help::unlisted(&installed.found.layout.cli);
        let unreachable = ErrorCode::IpcUnreachable.code();
        for line in [&[][..], &["help"], &["--help"]] {
            let outcome = super::run(&argv(line), &installed.found, &english());
            assert_eq!(outcome.exit, Exit::Unreachable, "{line:?}: {outcome:?}");
            assert_eq!(outcome.stdout, grammar, "{line:?}");
            assert!(
                outcome.stderr.starts_with(&format!("error[{unreachable}]")),
                "{line:?}: {}",
                outcome.stderr
            );
        }
        let outcome = super::run(&argv(&["--json", "help"]), &installed.found, &english());
        assert_eq!(outcome.exit, Exit::Unreachable, "{outcome:?}");
        let document = nc_canon::parse(outcome.stdout.trim_end()).expect("one document");
        assert_eq!(document.to_canonical(), outcome.stdout.trim_end());
        assert!(
            outcome.stdout.starts_with("{\"error\":"),
            "{}",
            outcome.stdout
        );
        assert_eq!(outcome.stderr, "");
        let daemon = Scripted::serve(
            &installed,
            Script {
                token: Token::fresh().expect("another token"),
                welcome: installed.welcome(),
                connections: vec![Vec::new()],
            },
        );
        let outcome = super::run(&argv(&["help"]), &installed.found, &english());
        assert_eq!(outcome.exit, Exit::Refused, "{outcome:?}");
        assert_eq!(outcome.stdout, grammar);
        assert!(!outcome.stderr.is_empty(), "{outcome:?}");
        assert_eq!(daemon.heard(), [] as [Request; 0]);
    }

    /// A command line the CLI cannot read is refused before anything is
    /// asked — no daemon is needed to refuse it — with exit 2.
    #[test]
    fn a_command_line_the_cli_cannot_read_reaches_no_daemon() {
        let installed = Scratch::new("bad");
        for line in [
            &["start"][..],
            &["--verbose"],
            &["--json", "Journal.Verify"],
        ] {
            let outcome = super::run(&argv(line), &installed.found, &english());
            assert_eq!(outcome.exit, Exit::Invalid, "{line:?}: {outcome:?}");
            let written = format!("{}{}", outcome.stdout, outcome.stderr);
            let code = ErrorCode::CliCommandLineUnreadable.code();
            assert!(written.contains(code), "{written}");
        }
    }

    /// Help lists every command the registry holds by its name and title, and
    /// help on one shows the arguments its schema declares.
    #[test]
    fn help_reads_the_registry() {
        let (installed, daemon) = serving("hlp", vec![Vec::new(), Vec::new(), Vec::new()]);
        let everything = super::run(&argv(&["help"]), &installed.found, &english());
        assert_eq!(everything.exit, Exit::Success, "{everything:?}");
        for said in [
            "journal.verify",
            "Verify activity history",
            "reference.irreversible.do",
        ] {
            assert!(everything.stdout.contains(said), "{}", everything.stdout);
        }
        let one = super::run(
            &argv(&["help", "journal.verify"]),
            &installed.found,
            &english(),
        );
        assert!(
            one.stdout.contains("--from <whole number, at least 0>"),
            "{}",
            one.stdout
        );
        let flag = super::run(
            &argv(&["journal.verify", "--help"]),
            &installed.found,
            &english(),
        );
        assert_eq!(flag.stdout, one.stdout);
        assert_eq!(
            daemon.heard(),
            [Request::Registry, Request::Registry, Request::Registry],
            "help dispatched something"
        );
    }

    /// A command is run with the arguments its flags give, and its answer is
    /// shown — as one canonical document under `--json`.
    #[test]
    fn a_command_runs_with_the_arguments_its_flags_give() {
        let (installed, daemon) = serving(
            "run",
            vec![
                vec![Answer::Result(Value::text("intact"))],
                vec![Answer::Result(Value::Integer(3))],
            ],
        );
        let outcome = super::run(
            &argv(&["journal.verify", "--from=7"]),
            &installed.found,
            &english(),
        );
        assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
        assert_eq!(outcome.stdout, "\"intact\"\n");
        let outcome = super::run(
            &argv(&["--json", "journal.verify", "--", r#"{"from":0}"#]),
            &installed.found,
            &english(),
        );
        assert_eq!(outcome.stdout, "{\"ok\":3}\n");
        let heard = daemon.heard();
        assert_eq!(
            heard,
            [
                Request::Registry,
                Request::Dispatch {
                    command: "journal.verify".to_owned(),
                    args: Value::object([("from", Value::Integer(7))])
                },
                Request::Registry,
                Request::Dispatch {
                    command: "journal.verify".to_owned(),
                    args: Value::object([("from", Value::Integer(0))])
                },
            ]
        );
    }

    /// Each refusal the kernel answers with ends the run in its row of the table.
    ///
    /// A command that cannot be undone ends it in exit 4, with `NC-KERN-0006`:
    /// the code saying a person must confirm it in the Noctua window, shown in
    /// the environment's language.
    #[test]
    fn each_refusal_ends_the_run_in_its_row_and_a_confirmation_in_four() {
        let rows = [
            (
                refusal(ErrorCode::PolicyDenied, RefusalKind::Denied),
                Exit::Denied,
            ),
            (
                refusal(ErrorCode::KernelUnconfirmed, RefusalKind::Denied),
                Exit::NeedsTheInterface,
            ),
            (
                refusal(ErrorCode::KernelUnknownCommand, RefusalKind::NotFound),
                Exit::NotFound,
            ),
            (
                refusal(ErrorCode::ValidationArgumentsInvalid, RefusalKind::Invalid),
                Exit::Invalid,
            ),
            (
                refusal(ErrorCode::KernelUndoConflict, RefusalKind::Conflict),
                Exit::Conflict,
            ),
            (
                refusal(ErrorCode::KernelDeadline, RefusalKind::Unavailable),
                Exit::Unavailable,
            ),
            (
                refusal(ErrorCode::PluginTrapped, RefusalKind::Internal),
                Exit::Internal,
            ),
        ];
        let (installed, daemon) = serving(
            "kinds",
            rows.iter()
                .map(|(refusal, _)| vec![Answer::Refused(refusal.clone())])
                .chain([vec![Answer::Stopping]])
                .collect(),
        );
        let portuguese = Env::of(|name| (name == "LANG").then(|| "pt_BR.UTF-8".to_owned()));
        for (refusal, exit) in &rows {
            let outcome = super::run(
                &argv(&["reference.irreversible.do", "--note", "n1"]),
                &installed.found,
                &portuguese,
            );
            assert_eq!(outcome.exit, *exit, "{refusal:?}: {outcome:?}");
            assert!(
                outcome
                    .stderr
                    .starts_with(&format!("error[{}]", refusal.code.code())),
                "{}",
                outcome.stderr
            );
            assert!(
                outcome.stderr.contains(&refusal.sentence),
                "{}",
                outcome.stderr
            );
        }
        let unconfirmed = super::run(
            &argv(&["reference.irreversible.do", "--note", "n1"]),
            &installed.found,
            &portuguese,
        );
        assert_eq!(
            unconfirmed.exit,
            Exit::Internal,
            "an answer of another kind: {unconfirmed:?}"
        );
        assert!(
            unconfirmed
                .stderr
                .contains(ErrorCode::CliAnswerUnreadable.code()),
            "{}",
            unconfirmed.stderr
        );
        let heard = daemon.heard();
        assert_eq!(heard.len(), 2 * (rows.len() + 1));
        let message =
            nc_catalog::message("error.kernel.unconfirmed", Locale::PtBR).expect("a message");
        assert!(message.contains("janela do Noctua"), "{message}");
    }

    /// Arguments the schema refuses, and a command the registry does not hold,
    /// are refused before a dispatch is sent: exit 2 and exit 5.
    #[test]
    fn what_the_registry_and_the_schema_refuse_is_never_dispatched() {
        let (installed, daemon) = serving("pre", vec![Vec::new(), Vec::new(), Vec::new()]);
        let outcome = super::run(
            &argv(&["journal.verify", "--from", "-1"]),
            &installed.found,
            &english(),
        );
        assert_eq!(outcome.exit, Exit::Invalid, "{outcome:?}");
        let code = ErrorCode::ValidationArgumentsInvalid.code();
        assert!(outcome.stderr.contains(code), "{}", outcome.stderr);
        let outcome = super::run(
            &argv(&["reference.irreversible.do"]),
            &installed.found,
            &english(),
        );
        assert_eq!(
            outcome.exit,
            Exit::Invalid,
            "a required argument left out: {outcome:?}"
        );
        let outcome = super::run(
            &argv(&["--json", "nowhere.to.go"]),
            &installed.found,
            &english(),
        );
        assert_eq!(outcome.exit, Exit::NotFound, "{outcome:?}");
        assert!(
            outcome
                .stdout
                .contains(ErrorCode::KernelUnknownCommand.code()),
            "{}",
            outcome.stdout
        );
        assert_eq!(outcome.stderr, "");
        assert!(
            daemon
                .heard()
                .iter()
                .all(|request| *request == Request::Registry),
            "something refused before it was sent was dispatched"
        );
    }

    /// Nothing listening ends the run in exit 9, and a daemon that cannot
    /// prove it holds the token in exit 10.
    #[test]
    fn nothing_listening_is_nine_and_a_refused_handshake_ten() {
        let installed = Scratch::new("dwn");
        installed.token(&Token::fresh().expect("a token"));
        let outcome = super::run(&argv(&["journal.verify"]), &installed.found, &english());
        assert_eq!(outcome.exit, Exit::Unreachable, "{outcome:?}");
        let code = ErrorCode::IpcUnreachable.code();
        assert!(outcome.stderr.contains(code), "{}", outcome.stderr);
        let daemon = Scripted::serve(
            &installed,
            Script {
                token: Token::fresh().expect("another token"),
                welcome: installed.welcome(),
                connections: vec![Vec::new()],
            },
        );
        let outcome = super::run(&argv(&["journal.verify"]), &installed.found, &english());
        assert_eq!(outcome.exit, Exit::Refused, "{outcome:?}");
        assert_eq!(daemon.heard(), [] as [Request; 0]);
    }

    /// No install beside the executable ends the run in exit 12 with the
    /// manifest's code, rendered as the command line asks.
    #[test]
    fn no_install_is_twelve_with_the_manifests_code() {
        let installed = Scratch::new("nin");
        let missing = nc_channel::found::beside(&installed.path("absent").join("noctua"))
            .expect_err("nothing is there");
        let outcome = crate::unfound(&argv(&["journal.verify"]), &missing, &english());
        assert_eq!(outcome.exit, Exit::NoInstall);
        assert_eq!(outcome.code(), 12);
        assert!(
            outcome
                .stderr
                .starts_with(&format!("error[{}]", missing.code().code()))
        );
        let outcome = crate::unfound(&argv(&["--json", "help"]), &missing, &english());
        let document = nc_canon::parse(outcome.stdout.trim_end()).expect("one document");
        assert_eq!(
            document.to_canonical(),
            outcome.stdout.trim_end(),
            "not the canonical text"
        );
        assert_eq!(outcome.stderr, "");
        assert!(outcome.stdout.contains("\"exit\":12"), "{}", outcome.stdout);
    }
}
