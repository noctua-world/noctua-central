//! Brings the local development stack up, or takes it down.
//!
//! One command to a working PostgreSQL and a working central log store —
//! Stage 10's local development stack, and the tenth of its acceptance criteria.
//!
//! **From a clean machine, which is what CI's Linux runner is.** That is the
//! criterion's own wording, and it runs in the `linux` job on every push and
//! pull request, and in `release.yml`'s gate job, each on a fresh runner that
//! pulls both pinned images layer by layer before the stack comes up healthy
//! and migrated — measured, with no layer already there. This header is the
//! half rustdoc publishes, which is why it is written here: a claim made only
//! in a body comment is one the published documentation can still contradict.
//!
//! **The two words mean the same thing everywhere in this file.** What `--down`
//! empties is the database, and the image store is untouched — so a local
//! `dev --down` → `dev` is not the clean-machine run, and the comment beside
//! `--down` in [`run`] says *an empty database*, not a clean machine.
//!
//! **It passes no `.env`.** Docker Compose would read one from beside the
//! compose file, which is inside the repository, and `cargo xtask check` fails
//! on a `.env` at any depth (`NOCTUA.md §15.5`). Every credential the compose
//! file reads has a default there, and its two ports are handed to it by this
//! file — the checkout's, from `nc_channel` ([`Port`]) — so the stack comes up
//! with none set; a developer who wants different values exports them, and the
//! example environment, `.env.example`, says how.

use std::path::Path;
use std::process::Command;

use crate::ui;

/// How long to wait for both services to report healthy.
///
/// PostgreSQL initialises a fresh data directory on its first run, which is the
/// slow case this has to cover — on this machine it took about six seconds, so
/// the ceiling is generous rather than tight. A stack that is not up by then is
/// broken rather than slow, and a wait that never ends is worse than a message.
const HEALTH_TIMEOUT_SECONDS: u64 = 120;

/// One setting of the local stack: the variable that names it, and what it is
/// when nothing does.
///
/// **Written once, and held to the compose file.** `docker-compose.yml`
/// declares each as `${NAME:-value}`, and `dev.rs` and `database.rs::default_url`
/// both read them; a default copied into each reader and held to nothing
/// drifts. The two ports are not settings: they are the checkout's ([`Port`]).
/// Measured without
/// `the_settings_this_tool_reads_are_the_ones_the_stack_declares`, `POSTGRES_DB`'s
/// default in this file alone changed to `"central"`: `check passed` with no warn
/// line, and `cargo xtask dev` then failed outright —
/// `the stack is up and PostgreSQL refused: database "central" does not exist`.
/// That is the defect [`stack_url`]'s own paragraph is written about, one
/// argument along.
///
/// `.env.example` is held to the compose file by
/// `the_example_environment_agrees_with_the_compose_defaults`; this holds the
/// code to it, in both directions.
type Setting = (&'static str, &'static str);

/// Who connects.
pub(crate) const POSTGRES_USER: Setting = ("POSTGRES_USER", "noctua");
/// What they connect with.
pub(crate) const POSTGRES_PASSWORD: Setting = ("POSTGRES_PASSWORD", "noctua");
/// Which database.
pub(crate) const POSTGRES_DB: Setting = ("POSTGRES_DB", "noctua");

/// Every one of them, so a test can read the set rather than three names.
///
/// A reverse loop over `docker-compose.yml` is what makes this a set rather than
/// a sample: a fourth `${NAME:-value}` added to the stack and not added here is a
/// setting the tools do not read, which is the same stack-with-two-configurations
/// the paragraph above measures.
#[cfg(test)]
pub(crate) const THE_STACK_SETTINGS: [Setting; 3] = [POSTGRES_USER, POSTGRES_PASSWORD, POSTGRES_DB];

/// What the environment says, or what the stack is without it.
pub(crate) fn setting((variable, default): Setting) -> String {
    as_compose_reads(std::env::var(variable).ok()).unwrap_or_else(|| default.to_owned())
}

/// One of the stack's two ports: which of the checkout's pair it is, the
/// variable that moves it, and the service that publishes it.
///
/// **The checkout's, and not a setting with a default.** Two checkouts at one
/// default bind one port, so the second `dev` cannot come up beside the first.
/// The pair is derived from the checkout's path by `nc_channel::Checkout::stack`
/// — the one function every name an install gives the system comes from, and
/// the one that names this stack's compose project — and `docker-compose.yml`
/// writes each port as `${NAME:?…}`, with nothing to fall back to: a
/// `docker compose up` that was not handed the pair refuses, rather than
/// binding a port another checkout's stack may hold. Either variable,
/// `POSTGRES_PORT` or `LOGS_PORT`, moves its port, and the one reader of
/// both is [`port`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Port {
    /// Where PostgreSQL listens, on loopback.
    Postgres,
    /// Where the log store listens, on loopback.
    Logs,
}

/// What each port is called, which of the pair it is, and who publishes it.
impl Port {
    /// Both, so every reader hands compose the pair and a test reads the set.
    pub(crate) const ALL: [Self; 2] = [Self::Postgres, Self::Logs];

    /// The variable that moves it.
    pub(crate) const fn variable(self) -> &'static str {
        match self {
            Self::Postgres => "POSTGRES_PORT",
            Self::Logs => "LOGS_PORT",
        }
    }

    /// Which of the checkout's pair it is when its variable says nothing.
    const fn of(self, stack: &nc_channel::Stack) -> u16 {
        match self {
            Self::Postgres => stack.postgres,
            Self::Logs => stack.logs,
        }
    }

    /// The service that publishes it, and the port inside that service's
    /// container it publishes, as `docker-compose.yml` writes both.
    const fn published_by(self) -> (&'static str, &'static str) {
        // **What `dev` asks `docker compose port` with**, to tell its own stack
        // from another holder of a port it cannot bind, so a wrong answer here
        // refuses every `dev` over a running stack. Measured with the two arms
        // swapped: the suites that judge this file passed, 138 of 138, and
        // `cargo xtask dev` over this checkout's running stack refused its own
        // PostgreSQL port as *held by something this checkout's stack does not
        // run*. `each_port_is_asked_of_the_service_that_publishes_it` holds both
        // halves to the compose file.
        match self {
            Self::Postgres => ("postgres", "5432"),
            Self::Logs => ("logs", "9428"),
        }
    }
}

/// The port this checkout's stack binds for `which`: what its variable says,
/// and the checkout's when it says nothing.
///
/// **The one reader**, for `dev`, `database.rs::default_url` and the log
/// store's bounds in `check.rs` alike, and for every compose invocation this
/// file makes: a port read one way where the stack comes up and another where
/// the tools look for it is the stack with two configurations the paragraph on
/// [`Setting`] measures.
pub(crate) fn port(root: &Path, which: Port) -> String {
    port_of(root, which, std::env::var(which.variable()).ok())
}

/// The port [`port`] reads, with what the variable says handed in, so a test
/// can hand it either answer without setting this process's environment.
fn port_of(root: &Path, which: Port, said: Option<String>) -> String {
    as_compose_reads(said).unwrap_or_else(|| {
        which
            .of(&nc_channel::Checkout::of(root).stack())
            .to_string()
    })
}

/// What a variable says, read as compose reads it: one set empty says nothing.
///
/// **One rule for the tools and the stack.** Compose reads `${NAME:-value}`
/// as its default and `${NAME:?…}` as missing when `NAME` is set empty, and
/// the example environment writes the two ports empty — so a copy of it
/// sourced whole leaves every port the checkout's, here as there. Read as a
/// value, an empty `POSTGRES_PASSWORD` would be the empty password here and the
/// default, `noctua`, in the stack: two configurations of one stack.
fn as_compose_reads(said: Option<String>) -> Option<String> {
    said.filter(|said| !said.is_empty())
}

/// The url of the stack `docker-compose.yml` brings up.
///
/// **Read from the same four variables compose reads, with the same defaults.**
/// `migrate` does not read `DATABASE_URL`, which can point anywhere, and it does
/// not hardcode the credentials either: hardcoded, three of the four variables
/// `.env.example` documents break `dev` outright — `POSTGRES_PASSWORD=hunter2`
/// gives *password authentication failed*, and `POSTGRES_DB=central` gives
/// *database "noctua" does not exist*. A stack whose
/// operator variables the tool that brings it up does not read is a stack with
/// two configurations.
fn stack_url(port: &str) -> String {
    let user = setting(POSTGRES_USER);
    let password = setting(POSTGRES_PASSWORD);
    let database = setting(POSTGRES_DB);
    format!("postgres://{user}:{password}@127.0.0.1:{port}/{database}")
}

/// The `SET search_path` this run pins, as text.
///
/// **Split out because the whole rule is in this one string.** No test runs
/// `migrate`, which needs a live database, so without a test of its own a
/// single token added here puts the schema second:
///
/// ```text
/// SET search_path = "$user", "noctua", pg_catalog, pg_temp
/// ```
///
/// With a role-named schema present — the `CREATE SCHEMA … AUTHORIZATION`
/// arrangement the twelfth migration calls ordinary — `dev` then builds a second
/// complete journal there, append-only triggers and all, reports
/// `12 migration(s) were already applied, and this run applied none`, and the
/// whole gate passes over it, measured. `check` pins to the schema the URL names,
/// so the two agree by both being right — which is why
/// `the_search_path_puts_the_named_schema_first` holds this string rather than
/// leaving it to a comparison of the two.
///
/// The schema is first among the schemas a name may resolve in, and `pg_temp`
/// is last so a temporary table cannot shadow a real one.
///
/// **`pg_catalog` is not named, because naming it is a defect.** PostgreSQL puts
/// the system catalogue implicitly first *unless the path names it*, so writing
/// `<schema>, pg_catalog, pg_temp` moves the catalogue **behind** a schema this
/// run may migrate — and the forward-only guard's own read of `pg_class` then
/// resolves there. Measured: with a `public.pg_class` of three columns present
/// and no source edited, the guard's query answered no record, `highest_applied`
/// answered `None`, the guard was skipped, and a migration numbered 0 was
/// applied *after* version 12 with `ok 13 migration(s) applied` and
/// `cargo xtask check` byte-identical to a clean run. The control, with the
/// shadow dropped, refuses it by name.
///
/// `nc-journal`'s pinned path is `<schema>, pg_temp` for this reason. It needs
/// `CREATE` on the schema, which its owner has, so it is a narrowing rather than
/// the zero-privilege door the eleventh migration closed.
pub(crate) fn path_statement(schema: &str) -> String {
    format!(
        "SET search_path = {}, pg_temp",
        crate::database::quoted_identifier(schema)
    )
}
/// How many migrations this run applied, from the two counts it read.
///
/// **`saturating_sub` on a signed count saturates at `i64::MIN`.** Taken
/// inline, a difference with `now` smaller than `already` — which only a wrong
/// read produces — prints
/// `ok -12 migration(s) applied, and the database now holds 0 of 12 on disk`
/// under a green run, measured. A count that went backwards is not a negative number of
/// migrations; it is a read this run cannot report from, and saying so is the
/// only answer that is not a wrong one.
fn what_this_run_applied(already: i64, now: i64) -> Result<i64, String> {
    if now < already {
        return Err(format!(
            "the migrations ran and the count went backwards: {already} were applied \
             before this run and {now} are recorded now. Something is reading a \
             different table from the one that was written — check `search_path` and \
             the schema the URL names"
        ));
    }
    // A plain subtraction, because the guard above is what makes it safe: both
    // counts are non-negative and `now` is not below `already`, so this cannot
    // wrap and cannot saturate. Written as the arithmetic it is rather than as a
    // `saturating_sub` that reads like a bound — the two are the same value
    // here, and only one of them says why.
    Ok(now - already)
}

/// Pins the connection to the schema the URL names, and answers which it is.
///
/// Its own function, so `migrate` stays under clippy's line limit. The path is
/// set once for the whole pool, so the forward-only guard's read, the count
/// beside it and `Migrator::run` all resolve `_sqlx_migrations` in the same
/// place, rather than each wherever its own search path reaches first.
async fn pin_the_path(pool: &sqlx::PgPool, url: &str) -> Result<String, String> {
    let schema = crate::database::configured_schema(url);
    let pinned = path_statement(&schema);
    sqlx::query(sqlx::AssertSqlSafe(&*pinned))
        .execute(pool)
        .await
        .map_err(|error| format!("the search path could not be pinned: {error}"))?;
    Ok(schema)
}

/// The highest migration this database has already applied, if any.
///
/// **The guard's *argument* is outside every mechanism that covers its
/// predicate.** [`is_backdated`] is decided from two directions — its own unit
/// test and `corpus::the_forward_only_predicate_decides_every_clause` — and both
/// hand it `highest` written out as `12`, so the expression that computes it is
/// read by neither and has rows of its own,
/// `corpus::the_highest_applied_is_the_highest_and_not_the_last_row_read`.
/// Measured without those rows, `applied.last()` replaced by `applied.first()`:
/// `cargo run -p xtask -- check` gave `check passed` with no warn line, while
/// against a database holding versions
/// 1..12 and 20, a migration numbered 13 was applied and recorded *after* 20 —
/// `ok 1 migration(s) applied`, out of order, permanently.
///
/// **The maximum, not the last.** The rows arrive `ORDER BY version` today and
/// `.last()` is only correct because of it, which puts the guard's correctness
/// in a `SELECT` twenty lines away — the same defect one line up. This asks the
/// question the rule asks.
///
/// `None` for a database with nothing applied, which is the ordinary first run.
pub(crate) fn highest_applied(applied: &[i64]) -> Option<i64> {
    applied.iter().copied().max()
}

/// Whether a migration on disk is new and numbered below one already applied.
///
/// **A clause no test in this workspace decides when it is written inline.**
/// Inline in [`migrate`], which no test runs, nothing reaches it from either
/// direction. Measured, the condition
/// replaced by `false` and `cargo test --workspace --locked` run whole:
/// **exit 0**, every target `ok`. With the same neutering and
/// a backdated migration numbered 0 on disk, `cargo xtask dev` printed
/// `ok 1 migration(s) applied, and the database now holds 13 of 13 on disk`,
/// and `_sqlx_migrations` then held version 0 recorded *after* version 12:
/// applied out of order and reported `ok`.
///
/// A function so it can be driven from written lists, and an operator in the
/// mutation corpus as well — a clause reachable from neither is a clause with
/// no coverage at all.
pub(crate) fn is_backdated(version: i64, highest: i64, applied: &[i64]) -> bool {
    version < highest && !applied.contains(&version)
}

/// Applies the migrations, forward-only and in numeric order.
///
/// **Through `sqlx`'s own migrator**, not through `psql`: the services will use
/// it, and two mechanisms recording what has been applied is two answers to
/// "what state is this database in".
///
/// **Forward-only is not one of the migrator's rules.**
/// The migrator refuses an applied migration that was *edited*, and applies a
/// newly-added lower-numbered one without comment — so the check below reads
/// what the database has already applied and refuses that case by name. The
/// numbering itself (one file per number, no gaps) is `cargo xtask check`'s, in
/// `limits::every_migration_is_numbered_once`, because a broken set should fail
/// the build rather than a deployment.
///
/// A stack that is up with no schema is a stack a developer then has to finish
/// by hand, and `cargo xtask check` would refuse it — which reads as a defect in
/// the code rather than as a missing step.
///
/// # Errors
///
/// Returns why a migration could not be applied, or which applied migration the
/// files no longer name.
fn migrate(root: &Path, port: &str) -> Result<(), String> {
    // **The stack's own database, and not `DATABASE_URL`.** Reading the
    // environment first gives a developer with `DATABASE_URL` pointing anywhere
    // else — which `.env.example` tells them to export — a run that migrates
    // *that* database and then prints the compose url as though it were ready:
    // "one command to a working platform" producing a stack whose advertised
    // database has no schema, and saying nothing.
    let url = stack_url(port);
    let migrations = root.join("migrations");
    if !migrations.is_dir() {
        return Err("migrations/ is missing, so the stack would come up with no schema".to_owned());
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("could not start a runtime: {error}"))?;
    runtime.block_on(async {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .map_err(|error| format!("the stack is up and PostgreSQL refused: {error}"))?;
        let migrator = sqlx::migrate::Migrator::new(migrations.as_path())
            .await
            .map_err(|error| format!("the migrations could not be read: {error}"))?;
        let count = migrator.iter().count();

        // **Forward-only is checked here, because the migrator does not check
        // it.** A migration numbered *below* one that has already run is applied
        // in silence: `_sqlx_migrations` ends up holding 0, 1, 2 with 0 recorded
        // last, and the schema is whatever that order happened to produce. The
        // migrator refuses an applied migration that was *edited*, which is a
        // different rule.
        // **The schema the connection string names, not whichever one the
        // search path reaches first.** Read unqualified, `_sqlx_migrations` is
        // wrong for the reason `database::applied` — the *reader*, in
        // `database.rs` — gives in its own paragraph, with the measurement `app`
        // recording six and `public` twelve; this writer is pinned the same way.
        //
        // Measured unqualified: with a second `_sqlx_migrations` in a role-named schema
        // ahead of `public` on the path, a migration numbered *below* the
        // twelve already applied ran, was recorded first, and `dev` printed
        // `13 migration(s) applied, and the database now holds 13 of 13 on
        // disk` — while `check`, which *is* pinned, read `public` and printed
        // `12 migration(s), numbered once each and in order`. Two parallel
        // schemas, and the guard's own sentence about applying out of order
        // "as though it had always been there" describes exactly what happens.
        let schema = pin_the_path(&pool, &url).await?;
        // **A read that failed is not a database with nothing applied.** With
        // `unwrap_or_default()`, a permission error, a wrong path or a dropped
        // connection all read as "nothing has been applied" and turn the
        // forward-only guard off. Only a table that is genuinely absent is
        // that, and it is asked for by name.
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_class c \
             WHERE c.relname = '_sqlx_migrations' AND c.relkind = 'r' \
               AND c.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = $1))",
        )
        .bind(&schema)
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("the applied migrations could not be read: {error}"))?;
        let applied: Vec<i64> = if present {
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&pool)
                .await
                .map_err(|error| format!("the applied migrations could not be read: {error}"))?
        } else {
            Vec::new()
        };
        if let Some(highest) = highest_applied(&applied) {
            for migration in migrator.iter() {
                let version = migration.version;
                if is_backdated(version, highest, &applied) {
                    return Err(format!(
                        "migration {version} ({}) is new and numbered below {highest}, \
                         which this database has already applied. Running it now would \
                         apply it out of order and record it as though it had always \
                         been there. Renumber it above {highest}, or start from a fresh \
                         database with `cargo xtask dev --down`",
                        migration.description
                    ));
                }
            }
        }

        // **What was already there, so the line can say what this run did.**
        // Printing `migrator.iter().count()` — the files on disk — makes a second
        // `dev` against an already-migrated database report
        // `12 migration(s) applied` having applied none. That is `§19.3`'s
        // shape: a step reporting on something it did not do, and the sentence
        // a reader uses to conclude the stack is at the right version.
        // Resolved through the pinned path above, and a failed read is a
        // failure rather than a zero — the same two rules as the guard's own
        // read beside it.
        let already: i64 = if present {
            sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
                .fetch_one(&pool)
                .await
                .map_err(|error| format!("the applied migrations could not be counted: {error}"))?
        } else {
            0
        };
        // **And what landed before it failed.** `Migrator::run` wraps each
        // migration on its own, so a failure half-way through leaves every one
        // before it committed and recorded — permanently, since a migration
        // only goes forwards. Saying only which migration failed hides that. Measured, a valid `0013`
        // followed by a `0014` that cannot apply:
        //
        // ```text
        // error a migration failed: while executing migration 14: relation
        //       "r49b_probe_two" already exists
        //
        // _sqlx_migrations:  13 | r49b probe one | success = t
        // ```
        //
        // and the stack is up, with no URL printed, because the line that prints
        // it is past this point. The success path two screens down reports
        // exactly what *this run* did, and so does this failure path.
        if let Err(error) = migrator.run(&pool).await {
            // **No fallback at all, because every one of them is a claim.**
            // With `unwrap_or(already)` and a difference taken by
            // `saturating_sub` — the pair the read of `now` below refuses
            // too — a read that failed would report *"None landed"*, which is a
            // statement about the database this path cannot make. A count that
            // could not be read is its own
            // third answer.
            let landed: Option<i64> = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
                .fetch_one(&pool)
                .await
                .ok();
            let applied = landed.and_then(|landed| what_this_run_applied(already, landed).ok());
            pool.close().await;
            return Err(format!(
                "a migration failed: {error}. {} before it, and each of those is \
                 committed — the stack is up on 127.0.0.1:{}. Fix the file and run \
                 `cargo xtask dev` again, or `cargo xtask dev --down` for a database \
                 with nothing in it",
                match applied {
                    None => "How many landed could not be read".to_owned(),
                    Some(0) => "None landed".to_owned(),
                    Some(applied) => format!("{applied} migration(s) landed"),
                },
                port
            ));
        }
        // **A read that failed is not a database holding nothing.** Not
        // `unwrap_or(0)`, the shape refused above for `present` and
        // `already` — *a permission error, a wrong path or a dropped
        // connection all read as "nothing has been applied"* — because this is
        // the read that decides what this run **reports**. It is also the one
        // number a developer reads to know the stack is ready.
        let now: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .map_err(|error| format!("the migrations ran and could not be counted: {error}"))?;
        pool.close().await;
        // **And `saturating_sub` on a signed count saturates at `i64::MIN`, not
        // at zero.** With `now` smaller than `already` — which only a wrong read
        // can produce — the arm below would print
        // `ok -12 migration(s) applied, and the database now holds 0 of 12 on
        // disk` under a green run, measured.
        let applied = what_this_run_applied(already, now)?;
        if applied == 0 {
            ui::ok(&format!(
                // `now`, the rows, not `count`, the files on disk — the
                // substitution this function's own paragraph records as the
                // defect, on this arm too.
                "{now} migration(s) were already applied, and this run applied none"
            ));
        } else {
            ui::ok(&format!(
                "{applied} migration(s) applied, and the database now holds {now} of \
                 {count} on disk"
            ));
        }
        Ok::<(), String>(())
    })
}

/// Whether this system runs no stack, in which case this verb says so and
/// does nothing.
///
/// **Not applicable rather than an error, and nothing touched.** The stack
/// runs on Linux, where CI brings it up (`NOCTUA.md §22.2`); on macOS and
/// Windows the gate's steps that need it are not applicable, and so is the
/// verb that brings it up or takes it down. Its own function so a test can hand
/// it either answer on any system.
fn stands_aside(runs_the_stack: bool) -> bool {
    if runs_the_stack {
        return false;
    }
    ui::not_applicable("the container stack, which this verb brings up and takes down");
    true
}

/// Installs this checkout as its development channel, or removes that
/// install — on every system, with or without the stack.
///
/// **The environment is read here and nowhere after**: the base directories
/// this system keeps a user's profiles under, and `NC_INSTALL_NAMESPACE`,
/// which relocates the install as a test's, are written into the manifest, so
/// the binaries never read them (`NOCTUA.md §17.1`). The CLI and the daemon are
/// built, and the plugins, then copied out of `target/` by `nc_cli::install`,
/// with the test world and its grant (`policy/world.json`); a daemon an earlier
/// install runs is stopped first. **The daemon is registered with this
/// system's service manager** as `noctua-daemon --restored`, restored when it
/// dies and started at each login, and started through it unless the profile
/// carried the `stopped` mark — `NOCTUA.md §17.3`. **On Linux, lingering** is
/// what starts it at boot rather than at the first login: kept where logind
/// keeps it, enabled where it lets this user, and otherwise the command and
/// the NixOS option that enable it are printed. `PATH` is not changed: the
/// CLI is at the path this prints.
///
/// # Errors
///
/// Returns what did not finish: the environment, the build, the install,
/// the registration or the start.
pub(crate) fn install(root: &Path, uninstall: bool) -> Result<(), String> {
    if uninstall {
        ui::heading("dev --uninstall");
    } else {
        ui::heading("dev --install");
    }
    let log_store = format!(
        "http://127.0.0.1:{}/insert/jsonline",
        port(root, Port::Logs)
    );
    let install =
        nc_cli::install::development(root, &|name: &str| std::env::var_os(name), Some(log_store))
            .map_err(|unmade| unmade.to_string())?;
    if uninstall {
        return uninstalled(&install);
    }
    crate::check::cargo(
        root,
        &[
            "build",
            "--locked",
            "--package",
            "nc-cli",
            "--package",
            "nc-daemon",
            "--bins",
        ],
        false,
    )
    .map_err(|said| format!("the CLI and the daemon did not build: {said}"))?;
    ui::ok("the CLI and the daemon are built");
    crate::plugins::run(root)?;
    let built = root.join("target").join("debug");
    let suffix = nc_platform::EXECUTABLE_SUFFIX;
    let (cli, daemon) = (
        built.join(format!("noctua{suffix}")),
        built.join(format!("noctua-daemon{suffix}")),
    );
    let world = std::fs::read_to_string(root.join("policy/world.json"))
        .map_err(|error| format!("`policy/world.json` could not be read: {error}"))?;
    let world = nc_cli::install::development_world(&world).map_err(|unmade| unmade.to_string())?;
    let mut sources = Vec::new();
    for member in crate::members::members(root)? {
        if let Some(name) = member.path.strip_prefix("plugins/") {
            sources.push((
                name.to_owned(),
                root.join(&member.path).join(nc_channel::PLUGIN_MANIFEST),
                root.join(format!("target/plugins/components/{name}.wasm")),
            ));
        }
    }
    let plan = nc_cli::install::development_plan(
        install,
        &cli,
        &daemon,
        &world,
        sources
            .iter()
            .map(|(name, manifest, component)| nc_cli::install::Plugin {
                name,
                manifest,
                component,
            })
            .collect(),
    );
    let installed = nc_cli::install::install(&plan).map_err(|unmade| unmade.to_string())?;
    said_installed(&installed)
}

/// Removes this checkout's development install, `install`, and says what
/// was removed: `dev --uninstall`.
///
/// # Errors
///
/// Returns what did not finish: the stop, the restorer, or removing the
/// files.
fn uninstalled(install: &nc_channel::Install) -> Result<(), String> {
    match nc_cli::install::uninstall(install).map_err(|unmade| unmade.to_string())? {
        None => ui::ok("nothing was installed for this checkout, so nothing was removed"),
        Some(nc_cli::install::Uninstalled {
            layout,
            unregistered,
        }) => {
            ui::ok(&format!("{} is uninstalled", layout.cli));
            if let Some(name) = unregistered {
                ui::detail(&format!(
                    "its restorer, `{name}`, is unregistered from this system's service \
                     manager"
                ));
            }
            ui::detail(&format!(
                "its profile is kept under `{}`, `{}` and `{}`, so an install made again \
                 is the same device with the same data",
                layout.data.display(),
                layout.config.display(),
                layout.state.display()
            ));
        }
    }
    Ok(())
}

/// What `dev --install` says of the install it made, `installed`: its CLI,
/// its restorer and its start, and — for a real install on Linux — lingering,
/// which it enables where logind lets it.
///
/// # Errors
///
/// Returns what did not finish: finding the service manager, or asking
/// logind.
fn said_installed(installed: &nc_cli::install::Installed) -> Result<(), String> {
    ui::gap();
    if let Some(stopped) = &installed.stopped {
        ui::detail(&format!(
            "the daemon the earlier install ran was stopped first: {}",
            stopped.trim_end()
        ));
    }
    ui::ok(&format!("{} is installed", installed.found.layout.cli));
    ui::detail(&format!(
        "the CLI is `{}`; `PATH` is not changed, so it is run by that path or a link you make",
        installed.cli.display()
    ));
    let cli = installed.cli.display();
    if let Some(registered) = &installed.registered {
        ui::ok(&format!(
            "{} restores its daemon, `{}`, kept in `{}`, and starts it at each login",
            registered.manager,
            registered.name,
            registered.kept_in.display()
        ));
        match &registered.started {
            Some(started) => ui::detail(started.trim_end()),
            None => ui::detail(&format!(
                "its profile carried the `stopped` mark before this install, so the daemon \
                 stays down: `{cli} start` starts it"
            )),
        }
    }
    let lingering = lingering_asked(&installed.found.install, || {
        nc_platform::service::Registrar::for_user(&installed.found.user)
            .and_then(|registrar| registrar.linger())
            .map_err(|error| error.to_string())
    })?;
    if let Some(lingering) = lingering {
        let name = std::env::var("USER").unwrap_or_else(|_| "<your user name>".to_owned());
        match lingering_said(&lingering, &name) {
            Some(Said::Done(said)) => ui::ok(&said),
            Some(Said::ToDo(said)) => ui::warn(&said),
            None => {}
        }
    }
    ui::detail(&format!(
        "`{cli} stop` stops the daemon and keeps it down, whatever restores it, until \
         `{cli} start`; `{cli} help` lists every command"
    ));
    Ok(())
}

/// What logind answers of lingering for `install`, asked through `ask`.
///
/// **Asked for a real install alone**: a relocated install is a test's, whose
/// restorer lasts this boot or this login alone, and lingering is the
/// machine's, so `ask` is never called for one.
fn lingering_asked<E>(
    install: &nc_channel::Install,
    ask: impl FnOnce() -> Result<nc_platform::service::Lingering, E>,
) -> Result<Option<nc_platform::service::Lingering>, E> {
    if install.namespace.is_none() {
        ask().map(Some)
    } else {
        Ok(None)
    }
}

/// What `dev --install` says of lingering: done, or for its user to do.
#[derive(Debug, PartialEq, Eq)]
enum Said {
    /// Nothing is left to do.
    Done(String),
    /// What its user runs, or configures, to have it.
    ToDo(String),
}

/// What `dev --install` says once logind answered `lingering`, for the
/// user `name`: nothing on a system with no lingering, that it is kept or
/// was enabled, or **the exact command and the NixOS option** that enable it.
///
/// **Never enabled by a test**: lingering is the machine's, and this is the
/// one thing that asks for it, after the install it is for.
fn lingering_said(lingering: &nc_platform::service::Lingering, name: &str) -> Option<Said> {
    use nc_platform::service::Lingering;
    let option = format!("`users.users.{name}.linger = true;`");
    match lingering {
        Lingering::NotApplicable => None,
        Lingering::Already => Some(Said::Done(
            "logind keeps your user manager from boot on (lingering), so the daemon starts \
             with the computer"
                .to_owned(),
        )),
        Lingering::Enabled => Some(Said::Done(format!(
            "lingering is enabled for {name}, so the daemon starts with the computer; a NixOS \
             configuration that sets `users.users.{name}.linger = false;` takes it away at each \
             switch, and {option} declares it"
        ))),
        Lingering::Refused(said) => Some(Said::ToDo(format!(
            "lingering is not enabled, so the daemon starts at your first login and not at \
             boot: run `loginctl enable-linger {name}`, or `sudo loginctl enable-linger {name}` \
             where logind refuses you, or on NixOS set {option} — logind said {said}"
        ))),
    }
}

/// Brings the stack up, or takes it down with its volumes — on Linux. Elsewhere
/// it says the stack is not applicable there and changes nothing.
///
/// # Errors
///
/// Returns a message naming what to do when Docker is absent, when the compose
/// file is missing, or when a service does not become healthy.
pub(crate) fn run(root: &Path, down: bool) -> Result<(), String> {
    ui::heading("dev");
    if stands_aside(crate::check::RUNS_THE_STACK) {
        return Ok(());
    }
    let compose = root.join("docker-compose.yml");
    if !compose.is_file() {
        return Err("docker-compose.yml is missing, so there is no stack to bring up".to_owned());
    }

    if down {
        // `-v` removes the volumes too. That is the point of the flag: `dev
        // --down` is what a developer runs to get back to an empty *database*,
        // and leaving the data behind would make the next `dev` a different
        // test from the first one.
        //
        // **Not a clean machine.** `down -v` takes containers, networks and
        // volumes and leaves the image store alone, so the tree a `--down`
        // leaves behind still holds both pinned images — the sense this file's
        // header gives the words too.
        // **And it says whether there was anything to take down.** Saying
        // `the stack is down and its volumes are gone` over compose's own
        // `Warning: No resource found to remove for project "…"` is the shape
        // of a second `migrate` reporting `12 migration(s) applied` having
        // applied none, in the function above. It matters
        // when `--down` is run from the wrong checkout: the developer is told
        // the volumes are gone while another project's stack still holds the
        // ports, and the next `dev` fails on a port bind with no hint.
        let said = what_compose_reported(root, &["down", "-v"])?;
        if nothing_was_running(&said) {
            ui::ok("nothing was running under this project, so nothing was removed");
            ui::detail(&format!(
                "a compose project is named for the checkout this `xtask` was built in, \
                 not for the directory you are standing in — this one is `{}`, and a \
                 stack started from another checkout is not it; `docker compose ls` \
                 lists them",
                project_for(root)
            ));
        } else {
            ui::ok("the stack is down and its volumes are gone");
        }
        return Ok(());
    }

    // **`--remove-orphans`, because compose only warns.** A service taken out of
    // `docker-compose.yml` leaves its container running under this project, and
    // `up -d` prints `Found orphan containers … --remove-orphans` *above* the
    // `ok` line where nobody reads it. The wait below then reports that orphan
    // as an unhealthy service of this stack and spends the whole timeout on it.
    //
    // **And its ports are free, or already its own, before compose is asked.**
    // A pair another stack holds would fail `up` in Docker's words, about a
    // port the developer never chose; asked here, it fails naming the variable
    // that moves it.
    for which in Port::ALL {
        let (service, inside) = which.published_by();
        free_or_this_stacks(which, &port(root, which), || {
            compose_output(root, &["port", service, inside]).ok()
        })?;
    }
    compose_command(root, &["up", "-d", "--remove-orphans"])?;
    ui::ok("the stack is starting");
    wait_for_health(root)?;

    let logs_port = port(root, Port::Logs);
    // **Through the one reader, like its neighbour.** Reading the variable
    // itself with an inline port would be another copy of what the stack is
    // handed — and this is the port `migrate` connects to *and* the port the
    // success line prints. `database.rs::default_url` reads it through `port`
    // too; this reader, the one that decides where the migrations land, does
    // the same. Measured, with a setting read through its own `env::var` and
    // an inline `"5433"`, and without
    // `every_setting_is_read_through_the_table_that_declares_it`: `check passed`,
    // byte-identical to a clean run, while `cargo xtask dev` brought the stack up
    // on one port and tried to migrate another.
    let postgres_port = port(root, Port::Postgres);
    the_log_store_is_serving(&logs_port)?;
    migrate(root, &postgres_port)?;
    ui::gap();
    ui::ok("the stack is up");
    ui::detail(&format!("postgres    {}", stack_url(&postgres_port)));
    ui::detail(&format!(
        "log store   http://127.0.0.1:{logs_port}/select/vmui/  (interactive, live tailing)"
    ));
    ui::detail("both bind to loopback only; `cargo xtask dev --down` removes them and their data");
    // **What a tool outside `cargo xtask` has to be told.** The ports are this
    // checkout's, so no file states them: a hand-run `docker compose up`
    // refuses without them, and the PostgreSQL suites and `xtask/mutants.py`
    // read `DATABASE_URL`. So the line says what to export, whole.
    ui::detail(&format!(
        "the ports are this checkout's, and `POSTGRES_PORT` and `LOGS_PORT` move them; a \
         tool outside `cargo xtask` is told them by `export POSTGRES_PORT={postgres_port} \
         LOGS_PORT={logs_port} DATABASE_URL={}`",
        stack_url(&postgres_port)
    ));
    // **Which stack, said out loud.** The project name carries this checkout's
    // path (see `project_for`), so a machine with two checkouts has two of these
    // and `docker compose ls` is where a developer goes looking. A name nothing
    // ever prints is a name nobody can use.
    ui::detail(&format!(
        "compose project {} — `docker compose -p {} logs -f` follows them",
        project_for(root),
        project_for(root)
    ));
    Ok(())
}

/// A port this checkout's stack is about to bind is free, or already its own.
///
/// **Asked before compose, so the refusal names what moves the port.** Two
/// checkouts whose digests fall on one pair of the block
/// (`nc_channel::PORT_PAIRS`), or anything else listening on one of the pair,
/// fail `up` in Docker's words — `port is already allocated` — about a port
/// the developer never chose. Asked here, the same collision is a sentence
/// naming the port, the service, and the variable that moves it.
///
/// **A port held by this project is this stack's.** A second `dev` over a
/// running stack is the ordinary case, and refusing it would refuse every run
/// after the first, so a port that cannot be bound is asked of compose —
/// `published`, whose answer is what `docker compose port` prints for this
/// project — and only then, so a free port costs no docker call. What it does
/// not tell apart is a listener that is gone by the time compose binds, or
/// one that arrives in between: compose then fails as it always did, and its
/// message names both variables.
///
/// # Errors
///
/// Returns why: the variable does not hold a port, or the port cannot be
/// bound and this project does not publish it there.
fn free_or_this_stacks(
    which: Port,
    at: &str,
    published: impl FnOnce() -> Option<String>,
) -> Result<(), String> {
    let variable = which.variable();
    let Some(number) = at.parse::<u16>().ok().filter(|number| *number != 0) else {
        return Err(format!(
            "`{variable}` is {at:?}, which is not a port. Unset it for this checkout's own, \
             or set it to a port from 1 to 65535"
        ));
    };
    let bound = std::net::TcpListener::bind(("127.0.0.1", number)).map(drop);
    bound_or_this_stacks(which, number, bound, published)
}

/// What binding one port of the pair answered, judged: free, this stack's
/// own, or refused with the reason the bind gave.
///
/// **Held only when the bind says the port is in use.** A bind fails for more
/// reasons than a listener: a port below 1024 is refused to a user without
/// the privilege, and an address this system does not have is refused as
/// unavailable, with nothing listening and a rootful Docker still able to
/// publish the port. Read as held, each was refused as *held by something
/// this checkout's stack does not run*. Measured with `POSTGRES_PORT=543`
/// and nothing listening there: that sentence, about a holder `docker ps`
/// cannot show. So the refusal carries the bind's own error, and names a
/// holder only for `AddrInUse`.
///
/// **The bind's answer is handed in**, as what compose publishes is, so a
/// test hands it each answer, a free port included, which a test cannot
/// otherwise make without releasing one, and asks no docker.
///
/// # Errors
///
/// Returns why the port cannot be this stack's: something holds it, or the
/// bind was refused for the reason it gives.
fn bound_or_this_stacks(
    which: Port,
    number: u16,
    bound: std::io::Result<()>,
    published: impl FnOnce() -> Option<String>,
) -> Result<(), String> {
    let Err(error) = bound else {
        return Ok(());
    };
    let this_stacks = format!("127.0.0.1:{number}");
    if published().is_some_and(|said| said.lines().any(|line| line.trim() == this_stacks)) {
        return Ok(());
    }
    let (service, _) = which.published_by();
    let variable = which.variable();
    if error.kind() == std::io::ErrorKind::AddrInUse {
        return Err(format!(
            "127.0.0.1:{number}, where this checkout's {service} listens, is held by \
             something this checkout's stack does not run — another checkout's stack on the \
             same pair, or anything else listening there. Export `{variable}` with a free \
             port and run `cargo xtask dev` again; `docker ps` says whether a stack holds it"
        ));
    }
    Err(format!(
        "127.0.0.1:{number}, where this checkout's {service} listens, could not be bound to \
         ask whether it is free ({error}), and this checkout's stack does not publish it. \
         Export `{variable}` with a port this user can bind and run `cargo xtask dev` again"
    ))
}

/// The log store answers `GET /health`, asked from outside the container.
///
/// **The container's own healthcheck cannot fail.** The image has no shell, no
/// `curl` and no `wget`, so the only thing a compose `CMD` can run is the binary
/// — and `--help` parses arguments and exits 0 whether or not the HTTP server
/// is serving. Measured: it exits 0 and the container reports `healthy` with the
/// server never asked anything. `dev` would then print the store's URL, which is a
/// gate reporting success by not looking (`§19.3`), in the one place a developer
/// takes the answer on trust.
///
/// So this asks from here, where a socket exists. `/health` is the store's own
/// path and answers 200 when it is serving; `--help` answers 0 when the process
/// is alive. The two are different questions and only one of them is the one a
/// developer is about to rely on.
///
/// # Errors
///
/// Returns why the store did not answer.
fn the_log_store_is_serving(port: &str) -> Result<(), String> {
    use std::io::{Read as _, Write as _};

    let address = format!("127.0.0.1:{port}");
    let mut socket = std::net::TcpStream::connect(&address).map_err(|error| {
        format!("the log store is not accepting connections at {address}: {error}")
    })?;
    let deadline = std::time::Duration::from_secs(5);
    let _ = socket.set_read_timeout(Some(deadline));
    let _ = socket.set_write_timeout(Some(deadline));
    socket
        .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .map_err(|error| format!("the log store closed the connection: {error}"))?;
    // **Until the status line ends, not once.** `read` returns what has arrived,
    // and a status line split across TCP segments is ordinary: one call can hand
    // back `HTTP/1.1 ` with the code still in flight, which `served` reads as a
    // refusal. The failure is a false one — `dev` would report a healthy store
    // as "running and not serving" — and it would be intermittent, which is the
    // worst kind to chase. `store.rs`'s own reader has always used
    // `read_to_end`; this is the same rule, bounded, because only the first line
    // is being judged.
    let mut answer = Vec::new();
    let mut chunk = [0_u8; 64];
    while !answer.contains(&b'\n') && answer.len() < 512 {
        let read = socket
            .read(&mut chunk)
            .map_err(|error| format!("the log store did not answer: {error}"))?;
        if read == 0 {
            break;
        }
        answer.extend_from_slice(&chunk[..read]);
    }
    let status = String::from_utf8_lossy(&answer);
    // **One statement of the rule, in `store.rs`.** Copies drift: with one copy
    // requiring the three characters to be digits and another not,
    // `HTTP/1.1 2ab Invented` is a refusal in one place and
    // `the log store answers /health` in the other.
    if crate::store::served(&status) {
        ui::ok("the log store answers /health");
        Ok(())
    } else {
        Err(format!(
            "the log store is running and not serving: `GET /health` answered `{}`. The \
             container's healthcheck cannot see this — it runs `--help`, which exits 0 \
             while the process is alive",
            status.lines().next().unwrap_or("nothing").trim()
        ))
    }
}

/// What one line of `docker compose ps -a` says about one service.
///
/// **Acceptance criterion 10's one real decision.** The `dev::` tests cover
/// the log store's status line, the project name, `--down`, the search path
/// and the forward-only rule; the loop that decides *whether the stack is up*
/// is reached by none of them. Measured with the loop inline,
/// `health != "healthy"` replaced by `health == "unhealthy"` so that a container
/// reporting `starting` counts as healthy: `check passed` with no warn line, and
/// a cold `cargo xtask dev` printed `ok 2 service(s) healthy` against a
/// PostgreSQL still initialising its data directory, then failed with
/// `Connection reset by peer`.
///
/// Every arm of that loop is in the same position, and each is a defect this
/// file already carries a paragraph about. A verdict over one line is a pure
/// function, so it is one — driven from a written table in `corpus.rs`, one of
/// the suites `xtask/mutants.py` runs for this file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Running, and its healthcheck says so.
    Healthy,
    /// Not yet. What to say about it has been pushed into the list handed in.
    ///
    /// **The reason is pushed rather than returned, and that is not a style
    /// choice.** `check.rs`'s `WHAT_THE_GATE_CAN_SAY` reads every sentence this
    /// tool can say by walking forward from a `.push(`, and its own paragraph
    /// names the four shapes it cannot follow — one of which is *a constructor
    /// that is no needle*. Returned as `NotYet(format!(…))` these three
    /// sentences would leave that record, which is a verdict a reader sees and no list
    /// holds. Handed the list, the words stay where the record can see them and
    /// the table in `corpus.rs` still drives the clause.
    NotYet,
    /// Running and declaring no healthcheck, which never changes. Names it.
    NoHealthcheck(String),
}

/// Reads one `{{.Service}} {{.State}} {{.Health}}` line.
///
/// **An empty Health column is not "starting", it is "never".** `docker compose
/// ps` leaves it empty for a container that declares no healthcheck, which is a
/// permanent state — read as `starting` it cost the full 120 s and a diagnosis
/// blaming a PostgreSQL data directory. Measured twice, at `2:00.48 total`.
///
/// A service that is not running is not healthy whatever its health column says:
/// an exited container reports no health at all, and `-a` is what makes it
/// appear here rather than vanish from the list.
pub(crate) fn standing_of(line: &str, unhealthy: &mut Vec<String>) -> Standing {
    let mut parts = line.split_whitespace();
    let service = parts.next().unwrap_or("?");
    let state = parts.next().unwrap_or("");
    let health = parts.next().unwrap_or("");
    if state != "running" {
        unhealthy.push(format!(
            "{service} is {}",
            if state.is_empty() {
                "not running"
            } else {
                state
            }
        ));
        return Standing::NotYet;
    }
    if health.is_empty() {
        return Standing::NoHealthcheck(service.to_owned());
    }
    if health == "healthy" {
        return Standing::Healthy;
    }
    unhealthy.push(format!("{service} is {health}"));
    Standing::NotYet
}

/// Waits until every service reports healthy.
fn wait_for_health(root: &Path) -> Result<(), String> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(HEALTH_TIMEOUT_SECONDS);
    loop {
        // **`-a`, because `docker compose ps` omits what is not running.**
        // Without it, a service that exited on start simply disappears from this
        // list instead of appearing as unhealthy, so the loop below finds
        // nothing wrong and the stack is reported healthy with a declared
        // service dead. The
        // `seen == 0` guard only fires when *every* service is gone.
        let states = compose_output(
            root,
            &[
                "ps",
                "-a",
                "--format",
                "{{.Service}} {{.State}} {{.Health}}",
            ],
        )?;
        let mut unhealthy = Vec::new();
        let mut seen = 0_usize;
        for line in states.lines().filter(|line| !line.trim().is_empty()) {
            seen += 1;
            match standing_of(line, &mut unhealthy) {
                Standing::Healthy | Standing::NotYet => {}
                Standing::NoHealthcheck(service) => {
                    return Err(format!(
                        "`{service}` is running and declares no healthcheck, so `dev` \
                         cannot tell when it is ready. Every service in \
                         `docker-compose.yml` carries one; give this one a \
                         `healthcheck:` block, or take it out of the file"
                    ));
                }
            }
        }
        // Zero services is not "everything is healthy". Without this the wait
        // would return immediately against a compose file that started nothing.
        if seen == 0 {
            return Err(
                // **Named with its project.** `project_for` calls this stack
                // `noctua-central-<digest of the path>` on purpose, and a bare
                // `docker compose ps` resolves to the *directory basename* —
                // which is by construction never that name. Measured: the two
                // commands these messages named printed an empty table and no
                // output at all, while `docker compose -p noctua-central-44f26d14
                // ps -a` showed the unhealthy container. A developer reads "no
                // containers" and goes looking at the daemon.
                format!(
                    "docker compose reports no services at all, so nothing came up. Check \
                     `docker compose -p {} ps -a` and the daemon's logs",
                    project_for(root)
                ),
            );
        }
        if unhealthy.is_empty() {
            ui::ok(&format!("{seen} service(s) healthy"));
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "the stack did not become healthy within {HEALTH_TIMEOUT_SECONDS}s: {}. \
                 `docker compose -p {} logs` says why; a fresh PostgreSQL volume takes \
                 the longest, and a restart loop there usually means the data directory \
                 was written by a different major version",
                unhealthy.join(", "),
                project_for(root)
            ));
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

/// The Docker Compose project these services belong to, one per checkout.
///
/// **Named, because both of the alternatives share a stack between checkouts.**
/// Compose's default is the base name of the directory holding the compose file,
/// and two checkouts of this repository are two directories called
/// `noctua-central` — so the default collides in exactly the case it needs not
/// to, and `cargo xtask dev --down` from either removes the other's containers
/// *and its volumes*. Measured from a copy called `noctua-central`: the live
/// stack's containers carried `com.docker.compose.project=noctua-central` with
/// a working directory in the original tree.
///
/// A written-out constant is worse: it makes the collision unconditional, from
/// any directory under any name. Measured from a directory sharing nothing with
/// this repository but the flag:
///
/// ```text
/// docker compose ps -a                       ->  (nothing)
/// docker compose -p noctua-central ps -a     ->  noctua-central-postgres-1  running
/// ```
///
/// So the name carries the checkout: the repository's name, which is what a
/// person reads in `docker compose ls`, and the path's digest, which is what
/// makes it this checkout's. Canonicalized first, so a symlinked path and its
/// target are one stack rather than two.
///
/// **Derived by `nc_channel`, which names everything an install gives the
/// system** (`NOCTUA.md §17.1`): the stack is the checkout's, so it is
/// `Checkout::stack`, the one place the digest and its rendering are written,
/// and the one a development install's layout is handed too.
///
/// **And so are its ports**, from the same digest ([`Port`]), so two
/// checkouts' stacks run at once. Two whose digests fall on one pair, or a
/// port anything else holds, fail the second `dev` before compose runs,
/// naming the variable that moves the port, instead of quietly migrating the
/// first's database.
fn project_for(root: &Path) -> String {
    nc_channel::Checkout::of(root).stack().project
}

/// The compose invocation that was made, written so it can be run again.
///
/// **The `-p` is not optional in a printed command either.** Every invocation
/// here passes `["-p", &project_for(root)]`, because a bare `docker compose`
/// resolves the project from the *directory basename* — which by construction
/// is never that name. A printed command without it addresses a different
/// project, and a developer who runs it sees, measured:
///
/// ```text
///   error `docker compose up -d --remove-orphans` failed
///
/// $ docker compose ps -a
/// NAME  IMAGE  COMMAND  SERVICE  CREATED  STATUS  PORTS
/// $ docker compose -p noctua-central-ee54a959 ps -a
/// noctua-central-ee54a959-postgres-1   …   Up (healthy)
/// ```
///
/// — nothing at all, and running the printed command creates a *second* stack,
/// which is the collision `project_for` exists to prevent. `wait_for_health`'s
/// messages carry the project for the same reason, with the measurement written
/// above them.
fn ran(root: &Path, args: &[&str]) -> String {
    format!(
        "`docker compose -p {} {}`",
        project_for(root),
        args.join(" ")
    )
}

/// What a caller wants back from a `docker compose` run.
///
/// **Three copies of one builder chain are three places the isolation can be
/// lost.** With each reader writing `Command::new("docker")`, the `-p`, the
/// arguments and `.current_dir(root)` out in full and differing only in which
/// stream it reads, each of three planted defects leaves the suite green,
/// measured: an `--project-name` appended after the `-p`, which docker resolves
/// to the **later** flag — `docker compose -p a --project-name b config`
/// answers `name: b`; a `root` shadowed by `Path::new("/")`, which points every
/// checkout on the machine at one project while the printed command names
/// another; and `.current_dir(root)` deleted, after which compose walks up from
/// wherever the developer is standing and labels *that* tree's services with
/// this tree's project. A count of tokens in this file's text holds none of
/// them; the command that gets built does.
///
/// So the chain is written once, and the three readers say only what they read.
enum Compose {
    /// Docker's own output reaches the terminal, and nothing comes back.
    Through,
    /// The run is captured, and one of its streams is returned.
    Captured(Stream),
}

/// Which captured stream a reader takes.
///
/// `down` writes the one line that says it removed nothing to stderr, and the
/// readers that parse `ps` take stdout. The two are kept apart because stderr
/// noise in the middle of `ps` output is read as a service.
enum Stream {
    /// What compose wrote to standard output.
    Stdout,
    /// What compose wrote to standard error.
    Stderr,
}

/// The one place this file runs `docker compose`.
///
/// **It runs the process rather than handing one back.** A function that
/// returned a configured `Command` would leave its caller free to append
/// `.args(["--project-name", …])` before running it, which is the first of the
/// three defects this collapse exists to make impossible.
///
/// Everything from this signature to the `match` below is held whole by
/// `the_block_that_decides_isolation_is_the_one_written_down` — **against this
/// file's own text, comments and all**, because a comment inside the block is
/// where the next reader is told what it holds. The lines that decide what each
/// arm does are held beside it, and so are the three readers' whole bodies: the
/// variant each one passes is the whole of what tells output-through from
/// captured.
///
/// That refuses an ordinary edit as loudly as it refuses a defect: an
/// `--ansi never`, a `--progress plain`, a `.stdin(Stdio::null())` or an
/// `env_clear` of the `COMPOSE_*` variables is a red test naming the block. It
/// is the price, and it is paid on purpose — a checkout's stack is not worth an
/// edit nobody read.
///
/// **What a stack that cannot come up says, measured rather than described.**
/// Taken in a scratch copy with its own compose project name, the log store's
/// image replaced by `example.invalid/noctua-does-not-exist:1`:
///
/// ```text
/// dev
///  Image example.invalid/noctua-does-not-exist:1 Pulling
///  Image example.invalid/noctua-does-not-exist:1 Error failed to resolve
///    reference "example.invalid/noctua-does-not-exist:1": failed to do
///    request: Head "https://example.invalid/v2/.../manifests/1": dial tcp:
///    lookup example.invalid: no such host
/// Error response from daemon: failed to resolve reference ...
///   error `docker compose -p noctua-central-<hash> up -d --remove-orphans`
///         <the sentence written at the call site below, verbatim>
/// ```
///
/// The sentence is not quoted here on purpose: it is a needle
/// `a_stack_that_did_not_come_up_says_what_to_look_at` anchors on, and a test
/// that took the first copy would read a quotation here instead of the shipped
/// line. The test refuses a second copy rather than silently preferring the
/// first.
///
/// `cargo xtask dev` exited 1. So docker's own words reach the reader ahead of
/// this tool's sentence, which is what letting the output through is for, and
/// the sentence names the right three causes with the real one first.
fn compose(root: &Path, args: &[&str], want: Compose) -> Result<String, String> {
    // **And a caller cannot name a project either.** The block below is held
    // whole, and `.args(args)` sits inside it — so the one channel the pin
    // leaves open is the slice itself: `compose_command(root, &["--project-name",
    // "noctua-central", "up", "-d", …])` brings the stack up under the shared
    // name with the whole suite green without this refusal, measured, because
    // the walk that reads this file discards everything
    // above the first `Command::new("docker")` and the call site is four hundred
    // lines above it. Docker takes the later of two, so `--down` would then
    // address a project that never ran.
    //
    // Refused here rather than pinned at three call sites, because this line is
    // inside the held block and a text rule over call sites is one more thing
    // that reads text.
    // **Among the top-level flags, and every spelling of them.** Docker takes
    // `-p x`, `-px`, `-p=x`, `--project-name x` and `--project-name=x` alike and
    // resolves the flag to the *later* of two — so asking for the exact token
    // `-p` and the long form only lets `-pnoctua-central` at a call site bring
    // every checkout up under one project with the suite green.
    //
    // **And only the top-level ones, because `run`'s `-p` is `--publish`.** A
    // rule over the whole slice refuses `compose(root, &["run", "-p",
    // "8080:80", …])` with a message about project names, which is the
    // too-strong half. The distinction is
    // positional: docker's own flags come before the subcommand, so the scan
    // stops at the first argument that is not a flag. That is the cheaper way
    // to draw the line: it needs no second parameter.
    //
    // **And a top-level flag may take a separated value.** A `take_while` over
    // flags stops at the first token that is not one — so `--ansi never -p
    // shared up` slips past it, because `never` is the *value* of `--ansi` and
    // stands before the `-p`, measured. These are the flags that take one:
    // `--ansi`, `-f`, `--profile`, `--progress`, `--env-file`,
    // `--project-directory` and `--parallel`.
    let takes_a_value = |it: &str| {
        matches!(
            it,
            "-f" | "--file"
                | "--ansi"
                | "--profile"
                | "--progress"
                | "--env-file"
                | "--project-directory"
                | "--parallel"
        )
    };
    let mut before_the_verb = Vec::new();
    let mut its_value = false;
    for it in args {
        if its_value {
            its_value = false;
            continue;
        }
        if !it.starts_with('-') {
            break;
        }
        before_the_verb.push(*it);
        its_value = takes_a_value(it) && !it.contains('=');
    }
    if let Some(named) = before_the_verb.iter().find(|it| {
        it.starts_with("--project-name") || (it.starts_with("-p") && !it.starts_with("--"))
    }) {
        return Err(format!(
            "`{named}` was passed to `docker compose` among its arguments. The project \
             is this checkout's and is set above; docker resolves the flag to the later \
             of two, so the stack would come up under a name `--down` does not address"
        ));
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let mut command = Command::new("docker");
    command
        .arg("compose")
        .args(["-p", &project_for(root)])
        .args(args)
        // **The checkout's ports, handed to every invocation.** The file
        // writes no port of its own, so `up` and `config` refuse without them,
        // and so does `ps` of a project that is not running, while `down` and
        // `logs` of a running one do not ask, measured. Handed to all, no
        // invocation depends on which verbs compose resolves the file for.
        .envs(Port::ALL.map(|which| (which.variable(), port(root, which))))
        .current_dir(root);
    match want {
        Compose::Through => {
            let status = command.status().map_err(|error| unavailable(&error))?;
            if status.success() {
                Ok(String::new())
            } else {
                Err(format!(
                    "{} failed. Docker's own words are above this line and they are the \
             diagnosis; the three this fails for are an image that could not be fetched \
             — a first run on a machine that has never had them pulls both — a daemon \
             that is not running or that your user may not reach, and a port already \
             taken, which `POSTGRES_PORT` and `LOGS_PORT` are for",
                    ran(root, args)
                ))
            }
        }
        Compose::Captured(stream) => {
            let output = command.output().map_err(|error| unavailable(&error))?;
            if !output.status.success() {
                return Err(format!(
                    "{} failed: {}",
                    ran(root, args),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(String::from_utf8_lossy(match stream {
                Stream::Stdout => &output.stdout,
                Stream::Stderr => &output.stderr,
            })
            .into_owned())
        }
    }
}

/// Runs `docker compose`, letting its output through.
///
/// What a stack that cannot come up says is written on [`compose`], which is
/// where the sentence this returns lives.
fn compose_command(root: &Path, args: &[&str]) -> Result<(), String> {
    compose(root, args, Compose::Through)?;
    Ok(())
}

/// Whether `docker compose down` found anything of this project's to remove.
///
/// The needle is compose's own sentence, and it is pinned by a test holding the
/// line verbatim — a message this reads for meaning is a message that changes
/// under it.
fn nothing_was_running(said: &str) -> bool {
    said.contains("No resource found to remove")
}

/// What compose *said*, which for `down` is on stderr.
///
/// `compose_output` returns stdout, and compose writes `Warning: No resource
/// found to remove for project "…"` — the one line that says `down` removed
/// nothing — to stderr. Kept apart from `compose_output` rather than folded into
/// it, because that function's callers parse `ps` output and stderr noise in the
/// middle of it would be read as a service.
fn what_compose_reported(root: &Path, args: &[&str]) -> Result<String, String> {
    compose(root, args, Compose::Captured(Stream::Stderr))
}

/// Where the running `logs` container actually mounts its data volume.
///
/// **The compose file is what the stack *would* be; the container is what it
/// is.** A rule that compares the log store's `-storageDataPath` against a
/// mount read out of `docker-compose.yml` is satisfied by a container still
/// running from before the file was edited — or one started by hand with no
/// volume at all: measured, the compose `logs` container stopped
/// and the same digest-pinned image run on the same port with no mount, the
/// file untouched, left `check passed` and the whole transcript byte-identical,
/// printing *"the running log store keeps 30 day(s) and 8589934592 byte(s), as
/// declared"* about a directory that goes with the container.
///
/// `docker compose ps -q logs` names the container this project is running and
/// `docker inspect` says where it mounts. `None` means the question could not
/// be asked — no docker, no container — which the caller reports as its own
/// finding rather than as a mismatch.
///
/// # Errors
///
/// Returns why docker could not be asked.
pub(crate) fn where_the_logs_container_mounts(root: &Path) -> Result<Option<String>, String> {
    // **The whole body, not four lines somewhere in it.** A spelling, a count
    // of `let`, a name in the callee, and these four lines held as a block each
    // say what is *written*, not that it is all the function does: three lines
    // above them returning another container's mount leave every such pin
    // green, measured. Every statement of the body is compared, so any
    // statement added here is a failure that says to read the function.
    match compose_output(root, &["ps", "-q", "logs"])?.trim() {
        "" => Ok(None),
        id => where_that_container_mounts(id),
    }
}

/// Where the container with this id mounts, by asking docker about that id.
///
/// **Split from its caller so the id cannot be rebound on the way.** The gate
/// that exempts `docker inspect` from naming a project rests on the id having
/// come from a project-scoped `compose ps -q`, and neither a spelling nor a
/// count of the text between the two holds that: a spelling (`let id = `) is
/// kept by `let id: &str = "…";`, a count of `let` is kept by
/// `let mut id = …; id = "…";`, and the same count refuses an ordinary hoist of
/// the format string, which is the other way to be wrong.
///
/// Here there is nothing between the signature and the command, so the rule the
/// gate needs is about a name rather than about a count: `id` does not appear
/// in this function before the command that is handed it. A hoist is legal; a
/// rebinding is not, however it is spelled.
fn where_that_container_mounts(id: &str) -> Result<Option<String>, String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let output = Command::new("docker")
        .args([
            "inspect",
            id,
            "--format",
            "{{range .Mounts}}{{.Destination}}\n{{end}}",
        ])
        .output()
        .map_err(|error| unavailable(&error))?;
    if !output.status.success() {
        return Ok(None);
    }
    let said = String::from_utf8_lossy(&output.stdout);
    Ok(said
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned))
}

/// Runs `docker compose`, capturing what it printed.
fn compose_output(root: &Path, args: &[&str]) -> Result<String, String> {
    compose(root, args, Compose::Captured(Stream::Stdout))
}

fn unavailable(error: &std::io::Error) -> String {
    format!(
        "could not run docker: {error}. The local stack is PostgreSQL and the central \
         log store, and both come from Docker — install it, and make sure the daemon is \
         running and your user may reach it"
    )
}

#[cfg(test)]
mod tests {
    /// Lingering is said as done where logind keeps it or enabled it, and
    /// otherwise as the exact command and the NixOS option that enable it;
    /// a system with no lingering says nothing of it.
    ///
    /// No test enables it: these are logind's answers, as `dev --install`
    /// reads them.
    #[test]
    fn lingering_is_said_as_done_or_as_the_command_and_the_option_that_do_it() {
        use nc_platform::service::Lingering;
        assert_eq!(
            super::lingering_said(&Lingering::NotApplicable, "ana"),
            None
        );
        for done in [Lingering::Already, Lingering::Enabled] {
            assert!(
                matches!(
                    super::lingering_said(&done, "ana"),
                    Some(super::Said::Done(_))
                ),
                "{done:?}"
            );
        }
        let Some(super::Said::Done(enabled)) = super::lingering_said(&Lingering::Enabled, "ana")
        else {
            panic!("enabled lingering is not done");
        };
        for named in [
            "`users.users.ana.linger = true;`",
            "`users.users.ana.linger = false;`",
        ] {
            assert!(enabled.contains(named), "{named} is not said: {enabled}");
        }
        let refused = Lingering::Refused("`loginctl enable-linger 1000` refused".to_owned());
        let Some(super::Said::ToDo(said)) = super::lingering_said(&refused, "ana") else {
            panic!("refused lingering is not left to its user");
        };
        for named in [
            "`loginctl enable-linger ana`",
            "`sudo loginctl enable-linger ana`",
            "`users.users.ana.linger = true;`",
            "refused",
        ] {
            assert!(said.contains(named), "{named} is not said: {said}");
        }
    }

    /// Lingering is asked for a real install, and never for a relocated one.
    ///
    /// A relocated install is a test's, and lingering is the machine's: logind
    /// is not asked, so nothing a test installs can enable it.
    #[test]
    fn lingering_is_asked_for_a_real_install_and_never_for_a_relocated_one() {
        use nc_platform::service::Lingering;
        // Nothing is written: the bases are only named.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let base = |name: &str| root.join("home").join(name);
        let relocated = nc_channel::Install {
            channel: nc_channel::Channel::Development(nc_channel::Checkout::of(root)),
            bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                .expect("plain absolute bases"),
            namespace: Some(nc_channel::Namespace::parse("tl1").expect("a namespace")),
            log_endpoint: None,
        };
        let real = nc_channel::Install {
            namespace: None,
            ..relocated.clone()
        };
        let asked = std::cell::Cell::new(0_u8);
        assert_eq!(
            super::lingering_asked(&real, || {
                asked.set(asked.get() + 1);
                Ok::<_, String>(Lingering::Already)
            }),
            Ok(Some(Lingering::Already))
        );
        assert_eq!(asked.get(), 1, "a real install did not ask logind once");
        assert_eq!(
            super::lingering_asked(&relocated, || {
                asked.set(asked.get() + 1);
                Ok::<_, String>(Lingering::Enabled)
            }),
            Ok(None)
        );
        assert_eq!(
            asked.get(),
            1,
            "a test's install asked logind for lingering"
        );
        assert_eq!(
            super::lingering_asked(&real, || Err::<Lingering, _>("no `loginctl`".to_owned())),
            Err("no `loginctl`".to_owned()),
            "logind's refusal to answer was not the install's"
        );
    }

    /// Where no stack runs, the verb says so and goes no further; where one
    /// does, it says nothing and carries on.
    #[test]
    fn the_verb_is_not_applicable_where_no_stack_runs() {
        let _printing = crate::ui::printing();
        crate::ui::forget_counts();
        assert!(
            !super::stands_aside(true),
            "a system that runs the stack was told it does not"
        );
        assert!(
            crate::ui::not_applicable_parts().is_empty(),
            "a system that runs the stack printed a not-applicable line"
        );
        assert!(
            super::stands_aside(false),
            "a system that runs no stack would go on to ask docker"
        );
        assert_eq!(
            crate::ui::not_applicable_parts(),
            ["the container stack, which this verb brings up and takes down"],
            "the verb stood aside without saying so"
        );
        crate::ui::forget_counts();
    }

    /// The verb itself asks this system, through its own entry point.
    ///
    /// **The test above holds the helper and not the call.** It hands
    /// `stands_aside` each answer, so `run` handing it `true`, or not asking at
    /// all, leaves it green, and nothing else drives `run` off Linux: CI's
    /// macOS and Windows jobs run `cargo xtask check` and never `dev`. So this
    /// drives `run` over a root with no `docker-compose.yml`, which stops it
    /// before docker on every system. Where no stack runs it must stand aside
    /// and say so, which the macOS and Windows jobs hold. Where one does it
    /// must go on and meet the missing file, which the Linux job holds.
    #[test]
    fn the_verb_asks_this_system_before_it_reaches_docker() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(
            !root.join("docker-compose.yml").exists(),
            "the root this drives holds a compose file, so the verb would go on to docker"
        );
        let _printing = crate::ui::printing();
        crate::ui::forget_counts();
        let answer = super::run(root, false);
        let parts = crate::ui::not_applicable_parts();
        crate::ui::forget_counts();
        if crate::check::RUNS_THE_STACK {
            assert_eq!(
                answer,
                Err("docker-compose.yml is missing, so there is no stack to bring up".to_owned()),
                "on a system that runs the stack the verb stood aside, or went past the \
                 compose file"
            );
            assert!(
                parts.is_empty(),
                "on a system that runs the stack the verb said it does not apply: {parts:?}"
            );
        } else {
            assert_eq!(
                answer,
                Ok(()),
                "on a system that runs no stack the verb went on toward docker"
            );
            assert_eq!(
                parts,
                ["the container stack, which this verb brings up and takes down"],
                "on a system that runs no stack the verb did not say it does not apply"
            );
        }
    }

    /// A stack that did not come up says what to look at.
    ///
    /// **Acceptance criterion 10's failure half, which is the half that can be
    /// measured here.** The pass half — `cargo xtask dev` on a machine that has
    /// never had the images — is CI's: the `linux` job and `release.yml`'s gate
    /// job run it on a fresh runner that pulls both. What this closes is the
    /// sentence a reader meets when it goes wrong, which has to say more than
    /// the command with the word *failed* after it.
    ///
    /// Written against the message rather than against docker, because
    /// producing the real failure means pointing a service at a reference that
    /// cannot resolve — measured, and the transcript is on [`super::compose`] —
    /// and a test that needs a daemon is a test `--offline` cannot run.
    ///
    /// **The transcript lives on [`super::compose`]**, and every reference to it
    /// here names that item. Nothing in this tree reads a rustdoc link's target
    /// for relevance, so a link naming an item that does not hold the
    /// transcript — or does not exist — stays green while it sends a reader
    /// nowhere.
    #[test]
    fn a_stack_that_did_not_come_up_says_what_to_look_at() {
        // **A source assertion, and it says why.** Producing the real failure
        // means a daemon and a compose project — measured, and the transcript
        // is on `compose`, which is the one place this file runs compose — and a
        // test that needs one is a test `--offline`
        // cannot run. The sentence itself has to live at the call site, inside
        // the `Err(` the verdict census reads, so there is no helper to call:
        // what is asserted is the four things the sentence must name.
        // **The shipped sentence, not this file.** `include_str!` reads the
        // whole file including this module, and both the needle below and the
        // five words it then looks for are string literals *this test
        // contains*. Read whole, the file lets `find` land on the test's own
        // text once the live sentence moves, and the words are then supplied by
        // the list that is looking for them: measured with that list four lines
        // below the needle, inside the six-hundred-byte window, the sentence
        // reworded to name none of the five left `check passed`. So this cuts
        // at `mod tests`, as `the_over_deletion_guard_runs_before_the_commit`
        // does.
        let source = include_str!("dev.rs")
            .split("mod tests")
            .next()
            .expect("this file has a test module");
        // **Exactly one**, not the first of however many. Cutting at `mod tests`
        // stops the test reading its own literals and leaves the other half of
        // that defect: any earlier copy anywhere in the file — a doc block
        // quoting the line, say — is what `find` returns, and the window then
        // measures the copy rather than the shipped sentence. Measured with a
        // transcript in the runner's documentation quoting it: the window lands
        // on the quotation and the test fails against a sentence that has not
        // changed. A needle that must be unique is asserted unique.
        let needle = "failed. Docker's own words are above this line";
        assert_eq!(
            source.matches(needle).count(),
            1,
            "`{needle}` appears more than once outside the test module, so what this \
             test measures is whichever copy comes first rather than the sentence the \
             tool ships"
        );
        let at = source
            .find(needle)
            .expect("a failed stack says more than the command it ran");
        let said = &source[at..at + 600.min(source.len() - at)];
        for named in ["image", "daemon", "port", "POSTGRES_PORT", "LOGS_PORT"] {
            assert!(
                said.contains(named),
                "a stack that did not come up says nothing about `{named}`"
            );
        }
    }

    /// `dev`'s health check answers the shared table, whatever code it holds.
    ///
    /// **A needle list cannot win this.** `the_two_xx_rule_is_written_twice_and_no_more`
    /// scans for the *spelling* of a 2xx test and keeps a whitelist of the two
    /// files allowed to hold one, and a third classifier written here — whose
    /// protocol test is `version.len() > 5 && version.contains('/')` and whose
    /// value test is `code.as_bytes().first().copied() == Some(50)`, `50` being
    /// ASCII `2` — slips past every needle: measured, both scans report the same
    /// two files, and `cargo xtask dev` prints `the log store answers /health` for
    /// `HTTP/1.1 2ab Invented`, the one row `xtask/canaries/status-lines.tsv`
    /// calls `not-2xx`.
    ///
    /// So this asks the *behaviour* rather than the text. A listener answers
    /// each row of the shared table and `the_log_store_is_serving` has to agree
    /// with it. A third classifier is then free to exist and cannot disagree,
    /// which is the property the whitelist was standing in for.
    #[test]
    fn the_health_check_answers_the_shared_table() {
        use std::io::{Read as _, Write as _};

        let table = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/canaries/status-lines.tsv"
        ));
        let mut rows = 0_usize;
        for line in table.lines() {
            if line.starts_with('#') || line.trim().is_empty() || line.starts_with("line\t") {
                continue;
            }
            let mut column = line.split('\t');
            let status = column.next().expect("a status line");
            let terminator = column.next().expect("a terminator");
            let served = column.next().expect("a `served` column");
            // The rows this table writes for `Delivery`'s own reader carry a
            // body; a health check reads the status line and stops.
            let answer = format!(
                "{}{}",
                status.replace("\\r", "\r").replace("\\n", "\n"),
                match terminator {
                    "crlf" => "\r\n\r\n",
                    "lf" => "\n\n",
                    _ => "",
                }
            );
            let listener =
                std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
            let port = listener.local_addr().expect("a bound address").port();
            let body = answer.clone();
            let serving = std::thread::spawn(move || {
                let Ok((mut socket, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0_u8; 128];
                let _ = socket.read(&mut request);
                let _ = socket.write_all(body.as_bytes());
            });
            let verdict = super::the_log_store_is_serving(&port.to_string());
            let _ = serving.join();
            assert_eq!(
                verdict.is_ok(),
                served == "2xx",
                "`{status}` ({terminator}) is `{served}` in the shared table and \
                 `cargo xtask dev` says {}. The table is the one statement of what a \
                 2xx answer is; a health check that disagrees with it prints \
                 `the log store answers /health` over a store answering nonsense",
                if verdict.is_ok() { "yes" } else { "no" }
            );
            rows += 1;
        }
        assert!(
            rows > 15,
            "{rows} row(s) of the shared table were answered, and it holds more — a \
             loop that reads nothing agrees with everything"
        );
    }

    /// `--down` tells a nothing-to-remove run from a real one.
    ///
    /// **Saying `the stack is down and its volumes are gone` either way**, over
    /// compose's own warning, is the shape of a second `migrate` reporting
    /// `12 migration(s) applied` having applied none. It matters when `--down`
    /// is run from the wrong checkout: the
    /// developer is told the volumes are gone while another project's stack
    /// still holds the ports, and the next `dev` fails on a port bind.
    ///
    /// The sample is verbatim from `docker compose -p … down -v` against a
    /// project that had never been up, on the version this stack pins.
    #[test]
    fn a_down_that_removed_nothing_is_not_a_stack_coming_down() {
        let quiet = "time=\"2026-08-31T01:34:09+01:00\" level=warning msg=\"Warning: No \
                     resource found to remove for project \\\"nc-downprobe-2749760\\\".\"\n";
        assert!(super::nothing_was_running(quiet), "{quiet}");
        // A real teardown says what it removed, on stderr, and never that.
        let real = " Container noctua-central-9f-postgres-1  Removing\n \
                    Container noctua-central-9f-postgres-1  Removed\n \
                    Volume noctua-central-9f_postgres-data  Removing\n";
        assert!(!super::nothing_was_running(real), "{real}");
    }

    /// Every `docker` invocation in `code` that names no project, by ordinal.
    ///
    /// **Split out so the hole can be tested.** Inside the test, reading one
    /// file, the exemption — `docker inspect` is scoped by the id it is handed
    /// rather than by `-p` — could only ever be exercised by the tree's own
    /// call. Asking which subcommand ran is not enough, because the property
    /// its justification rests on is about the argument: an `inspect` pointed
    /// at a container *name* reaches whatever project named it, which for a
    /// copy of this checkout is the developer's own.
    ///
    /// So the exemption asks for the binding as well. `id` and nothing else:
    /// the value a project-scoped `compose ps -q` returned, which is what makes
    /// the project already decided before the command runs.
    fn the_invocations_that_name_no_project(code: &str) -> Vec<usize> {
        let mut chunks = code.split("Command::new(\"docker\")");
        chunks.next();
        let mut unscoped = Vec::new();
        for (nth, chunk) in chunks.enumerate() {
            let head = chunk.split_once(')').map_or(chunk, |(head, _)| head);
            let by_id = head.split(',').any(|piece| piece.trim() == "id");
            if head.contains("\"inspect\"") && by_id {
                continue;
            }
            if !names_its_project(chunk, code) {
                unscoped.push(nth + 1);
            }
        }
        unscoped
    }

    /// Every name-introducing construct in `spent`, as `(head, value)`.
    ///
    /// The *head* is the text a construct binds its names in; the *value* is
    /// what a `let` is given, and is empty for everything else. Rust introduces
    /// a name in five places and this walks all five: a `let` head, a `for`
    /// head, a function's parameter list, a closure's parameter list, and a
    /// `match` arm's pattern. Enumerating **the grammar** is what a list of
    /// spellings cannot do: a rule repaired one spelling at a time leaves the
    /// next spelling open.
    ///
    /// `spent` must have its strings blanked: a `=>` inside a message is not an
    /// arm, and a `|` inside one is not a closure.
    fn the_names_a_binder_introduces(spent: &str) -> Vec<(&str, &str)> {
        let bytes = spent.as_bytes();
        let opens_a_word = |at: usize| {
            at == 0 || {
                let before = bytes[at - 1];
                !before.is_ascii_alphanumeric() && before != b'_'
            }
        };
        let mut found: Vec<(&str, &str)> = Vec::new();
        // A `let`: the head runs to the `=` that gives it a value, and the
        // value from there to the `;`. `let x;` has no `=` and no value.
        for (at, _) in spent.match_indices("let ") {
            if !opens_a_word(at) {
                continue;
            }
            let rest = &spent[at + "let ".len()..];
            let ends = rest.find(['=', ';', '\n']).unwrap_or(rest.len());
            let value = if rest.as_bytes().get(ends) == Some(&b'=') {
                let tail = &rest[ends + 1..];
                &tail[..tail.find(';').unwrap_or(tail.len())]
            } else {
                ""
            };
            found.push((&rest[..ends], value));
        }
        // A `for`: the head runs to its ` in `. A `for` without one on its line
        // is not a loop header, and reading further would take a *use* —
        // `for entry in project.chars()` — for a binding.
        for (at, _) in spent.match_indices("for ") {
            if !opens_a_word(at) {
                continue;
            }
            let rest = &spent[at + "for ".len()..];
            let line = rest.split_once('\n').map_or(rest, |(line, _)| line);
            if let Some((head, _)) = line.split_once(" in ") {
                found.push((head, ""));
            }
        }
        // A function's parameters, between its own parentheses.
        for (at, _) in spent.match_indices("fn ") {
            if !opens_a_word(at) {
                continue;
            }
            let rest = &spent[at + "fn ".len()..];
            let Some(opens) = rest.find('(') else {
                continue;
            };
            let mut depth = 0_usize;
            for (index, character) in rest[opens..].char_indices() {
                match character {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            found.push((&rest[opens + 1..opens + index], ""));
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        // A closure's parameters. An opening `|` follows what can precede an
        // expression; `||` is an empty parameter list or a disjunction, and
        // neither introduces a name.
        let characters: Vec<(usize, char)> = spent.char_indices().collect();
        for (index, &(at, character)) in characters.iter().enumerate() {
            // **The second half is belt and braces, and that is written down
            // rather than left looking undecided.** Deleting it survives the
            // suite, measured, because the
            // preceding-character guard below refuses every `||` this tree can
            // produce: an or-expression's operands end in a name or a `)`, and
            // none of those is in that guard's list. It stays because `= || b`
            // and `( || b)` really are closures in Rust and the two rules read
            // opposite halves of the same two characters; a reader who deletes
            // it should know it was measured redundant, not that it decides
            // nothing.
            if character != '|' || characters.get(index + 1).is_some_and(|(_, c)| *c == '|') {
                continue;
            }
            let before = spent[..at].trim_end();
            if !(before.ends_with(['=', '(', ',', '{', ';']) || before.ends_with("move")) {
                continue;
            }
            let rest = &spent[at + 1..];
            let line = rest.split_once('\n').map_or(rest, |(line, _)| line);
            if let Some(closes) = line.find('|') {
                found.push((&line[..closes], ""));
            }
        }
        // A `match` arm's pattern: from the line it opens on to its `=>`, cut
        // at a guard, because a guard is a use and not a binding.
        for (at, _) in spent.match_indices("=>") {
            let opened = spent[..at].rfind('\n').map_or(0, |line| line + 1);
            let pattern = &spent[opened..at];
            found.push((pattern.split(" if ").next().unwrap_or(pattern), ""));
        }
        found
    }

    /// Whether one invocation passes `-p` with the project this tree computes.
    ///
    /// **The flag, not one spelling of its argument.** Asking for the literal
    /// `["-p", &project_for(root)]` exactly once makes hoisting the call into a
    /// local — `let project = project_for(root);` above three invocations, which
    /// is byte-identical behaviour and which `rustfmt` and clippy are both
    /// silent about — report all three as belonging to whatever the directory
    /// is called. Measured against a live stack: same project, same containers,
    /// three false findings. That is the too-strong half, and this file also records the
    /// same shape one rule along for the docker-id pin.
    ///
    /// So the flag is asked for once, and what follows it must be the project
    /// this tree computes: the call itself, or a binding whose only initialiser
    /// in this file is that call. A binding that comes from anywhere else is
    /// the defect the rule exists for.
    fn names_its_project(chunk: &str, code: &str) -> bool {
        let opens = "[\"-p\", ";
        if chunk.matches(opens).count() != 1 {
            return false;
        }
        let Some((_, after)) = chunk.split_once(opens) else {
            return false;
        };
        let Some((argument, _)) = after.split_once(']') else {
            return false;
        };
        let argument = argument.trim();
        let computed = format!("&{}(root)", "project_for");
        if argument == computed {
            return true;
        }
        let Some(named) = argument.strip_prefix('&') else {
            return false;
        };
        if named.is_empty() || !named.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return false;
        }
        // The binding, and the only one: a second `let` of the same name
        // between it and the invocation is where the project would change.
        // **The whole statement, because a prefix is not a value.**
        // `let project = project_for(root).replace("noctua-central", "…");`
        // begins with the pinned call and ends somewhere else: measured, with
        // the suite and clippy green, `compose up` under one project while
        // `--down -v`, `ps` and the printed command addressed another.
        //
        // **Every binding of that name, not exactly one of them.** Three
        // invocations in three functions hoisting the same call is ordinary —
        // and refusing it prints the most alarming message this tool has,
        // claiming three live invocations took the project from the directory's
        // name, over code that is `fmt`- and clippy-clean with behaviour
        // identical, measured. What matters is that *no* binding of that name
        // comes from anywhere else, which is what this compares.
        //
        // **And every way of writing a binding, not one of them.** A count of
        // bindings as `code.matches("let project = ")` walks straight past a
        // type annotation: `let project: String = "whatever".to_owned();` is a
        // binding of that name it never sees, so an invocation could take its
        // project from anywhere while the gate reported every invocation naming
        // the one this tree computes. Measured with the plant in
        // `compose_command` — the function that runs `up -d --remove-orphans`
        // and `down -v` — and `check passed`, fmt and clippy clean. `--down -v`
        // then removes a *fixed* project's volumes, which is the accident
        // `project_for` exists to prevent, and `ran()` still prints the computed
        // name, so the message names a project that is not the one that ran.
        //
        // `let mut` is counted as a binding — it has to be, or one pinned
        // binding beside a `let mut project = …` would balance — and never as
        // a *pinned* one, for the reason written beside the comparison below.
        // **Every place Rust introduces a name, because every one of them is a
        // binding.** Counting `let` and `for` heads misses a *parameter*, which
        // is neither: measured, moving `compose_command`'s invocation into a
        // closure `|project: String|` — the function that runs
        // `up -d --remove-orphans` and `down -v` — and hoisting the call once
        // elsewhere left `check passed`, with both compose verbs running under
        // the *fixed* project every checkout's directory is called while `ran()`
        // still printed the computed one. A rule repaired one spelling at a time
        // is closed by the grammar, not by the next spelling.
        //
        // The binders are: a `let` head, a `for` head, a function's parameters,
        // a closure's parameters, and a `match` arm's pattern. Strings are
        // spent first, because a `=>` inside a message is not an arm.
        //
        // **And a binding is *pinned* by its value, not by its spelling.** A
        // pinned form of the literal `let <name> = <call>;` refuses the same
        // hoist written `let project: String = project_for(root);` — a binding
        // the rule *sees* and does not *accept* — and an ordinary line then
        // prints the most alarming message this tool has: the other way of
        // being wrong.
        let spent = crate::gates::without_string_contents(code);
        let mut bindings = 0_usize;
        let mut pinned = 0_usize;
        for (head, value) in the_names_a_binder_introduces(&spent) {
            if !head
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == named)
            {
                continue;
            }
            bindings += 1;
            // **And a binding that can be changed is not a pin.** The value
            // compared here is the *initialiser*; a statement after it is not a
            // binder, so `let mut project = project_for(root);` followed by
            // `project.push_str("-planted")` — or `project.truncate(…)`, which
            // yields the directory's own name and restores the collision this
            // whole rule exists to prevent — count as one binding, one pinned,
            // unless `mut` is refused, and the gate then reports the invocation
            // as naming what this tree computes. Measured: `check passed` with
            // only line counts moving. The doc above records the same defect
            // written as *one* statement, closed by "the whole statement,
            // because a prefix is not a value"; two statements walk past that,
            // and so would three.
            //
            // What closes it is not reading the statements after the binding —
            // that is the next spelling again — but refusing the thing that
            // makes them possible: a pinned binding is immutable. Reassignment,
            // a mutating method and a `&mut` borrow all require `mut`, so one
            // word refuses the family. No binding of this name in this file
            // wants to be mutable.
            if head.split_whitespace().any(|word| word == "mut") {
                continue;
            }
            if value.trim().trim_start_matches('&') == computed.trim_start_matches('&') {
                pinned += 1;
            }
        }
        pinned > 0 && pinned == bindings
    }

    /// Every `docker compose` this verb runs names its project.
    ///
    /// **A source assertion, and it says so.** What these two functions do is
    /// start a process; driving them means starting Docker, and the thing being
    /// asserted is one flag. Without it the project name is the *directory's*
    /// name, so two checkouts of this repository — both called `noctua-central`
    /// — share one project and `cargo xtask dev --down` from either removes the
    /// other's containers and volumes. Measured from a copy of this checkout: the live
    /// stack's containers carried `com.docker.compose.project=noctua-central`
    /// with a working directory in the original tree.
    #[test]
    fn every_compose_invocation_names_its_project() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is readable");
        // **The test module, not the first `#[cfg(test)]`** — a file gains one
        // above its own tests sooner or later, and then the scan stops early
        // without saying so: `ui.rs` has several, and this file has one on
        // `THE_STACK_SETTINGS`.
        let body = source
            .split_once("\nmod tests {")
            .expect("the test module is below")
            .0;
        // **And code, not prose.** The doc on `failed` quotes the flag it is
        // about, and counting raw text counts the quotation: more `-p`s than
        // invocations, a red test for a comment.
        // **Every comment, not the one spelling.** `line_without_a_comment`
        // reads a line at a time and knows `//`, so a `/* … */` holding
        // `.args(["-p", &project_for(root)])` beside a live invocation makes the
        // chunk's flag count 2 — and one occurrence is all the rule below asks
        // for, measured. `without_comments` is a state machine
        // over the whole file: a block comment nests, a raw string ends at its
        // own hash count, and neither opens inside a string.
        let code = crate::gates::without_comments(body, "//");
        // **Every process this file starts, and exactly two of them.** A floor
        // such as `>= 2` counts one *spelling*: `let program = "docker";` a line
        // above, then `Command::new(program)` with its `-p` dropped, leaves the
        // literal invocations clearing the floor, and is invisible to the chunk
        // walk below — because the text before the first literal is what `split`
        // throws away. Measured on the invocation that runs
        // `compose up -d --remove-orphans`: `check passed`, with the stack
        // coming up under the project named after the directory while `--down`
        // still addressed the hashed one.
        //
        // So the number is the number, and every process started in this file
        // has to be the literal `docker`. A third invocation is a line to change
        // here after reading it, not a floor to clear.
        //
        // **Two: `compose`, and the `docker inspect` the rule below exempts.** A
        // builder chain written out once per reader is where each of three
        // planted defects — a second project flag, a shadowed `root`, a deleted
        // `.current_dir` — leaves this suite green, so the chain is written
        // once.
        let invocations = code.matches("Command::new(\"docker\")").count();
        let started = code.matches("Command::new(").count();
        assert_eq!(
            (started, invocations),
            (2, 2),
            "this file starts {started} process(es) and {invocations} of them spell \
             `docker` in the call. One that does not is a process the chunk walk below \
             cannot see, and the flag that walk looks for is the whole of what keeps two \
             checkouts' stacks apart"
        );
        // **Each invocation, not the total.** Two counts over one file are equal
        // whenever the flag is written as often as `docker` is invoked, wherever
        // it is written — so moving it off the one invocation that runs
        // `docker compose ps -a`, and writing it twice on another, leaves a
        // total green: measured, `check passed`, with the call that decides
        // whether the stack is up addressing the project named after the
        // directory, which for a copy of this checkout is the developer's own.
        //
        // Each `Command::new("docker")` opens a chunk that ends where the next
        // one begins, and the flag has to be in that chunk exactly once.
        let unscoped = the_invocations_that_name_no_project(&code);
        assert!(
            unscoped.is_empty(),
            "`docker` invocation(s) {unscoped:?} in this file do not name their project \
             exactly once, so they belong to whatever the directory is called — and \
             `--down` from a second checkout removes the first's volumes"
        );
    }

    /// The block that decides isolation is the one written down.
    ///
    /// **A token count over this file's text is not a statement about the
    /// command that gets built.** Each of three defects planted in the runner
    /// leaves a token count green, clippy silent and `fmt` clean, measured:
    ///
    /// - `.args(["--project-name", "noctua-central"])` appended after the `-p`.
    ///   A rule counting `["-p", ` sees no prefix it shares with the long form,
    ///   so the count stays one — and docker resolves the flag to its **later**
    ///   occurrence, measured: `-p a --project-name b` answers `name: b`. Every
    ///   checkout on the machine then addresses one project.
    /// - `root` shadowed by `Path::new("/")` as the function's first statement,
    ///   with the working directory left correct so the tool still works. The
    ///   argument is still the literal `&project_for(root)`, which a count
    ///   accepts on sight; the stack comes up under the digest of `/` while the
    ///   printed command names the real one.
    /// - `.current_dir(root)` deleted. Compose then walks up from wherever the
    ///   developer is standing, so `up` labels *that* tree's services with this
    ///   tree's project and `down -v` takes them from inside it.
    ///
    /// None of the three is reachable by widening the count, because none of
    /// them is about a count. So the block from the signature to the `match` is
    /// compared whole, comments and all, and `command` is named exactly twice
    /// below it — each time to run it. An arm that appended a flag before
    /// running would sit under neither clause otherwise.
    ///
    /// **The too-strong half, named rather than discovered.** This refuses an
    /// ordinary edit as loudly as a defect: `--ansi never`, `--progress plain`,
    /// a `.stdin(Stdio::null())`, an `env_clear` of the `COMPOSE_*` variables.
    /// Each is a red test whose message quotes the block. That is the price this
    /// file also pays for the lines that hand a container id to
    /// `docker inspect`.
    #[test]
    fn the_block_that_decides_isolation_is_the_one_written_down() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is readable");
        let body = source
            .split_once("\nmod tests {")
            .expect("the test module is below")
            .0;
        let opens = format!(
            "fn {}(root: &Path, args: &[&str], want: Compose) -> Result<String, String> {{\n",
            "compose"
        );
        // **This file's own text, not a comment-stripped copy of it.** Against
        // `without_comments`, which turns every comment line into a blank one
        // that the filter below then drops, a comment inside the pinned block is
        // free while the doc on both sides says "comments and all": planted, a
        // sentence added between `command` and `.arg("compose")` leaves the
        // suite green. A
        // comment inside this block is where the next reader is told what it
        // holds, which is exactly what must not change unread.
        let at = body.find(&opens).expect("the one runner is here") + opens.len();
        // **The whole body, not the lines a needle finds in it.** A pin ending
        // at the `match`, with a second clause collecting the lines naming
        // `command`, `output.stdout` or `output.stderr`, is a filter standing
        // between the region and the judgement — this tree's own catalogue
        // entry for a counter above the accounting. Two lines written into the
        // `Captured` arm naming none of the three —
        // `let stream = match stream { Stream::Stdout | Stream::Stderr =>
        // Stream::Stdout };` — leave the suite, clippy and `fmt` all silent
        // against it while `what_compose_reported` asks for stderr and gets
        // stdout, so `dev --down` says the volumes are gone having removed
        // nothing, measured.
        //
        // There is no needle. Everything from the signature to the closing
        // brace is compared, so the price is the messages too: rewording the
        // diagnosis is a red test quoting the body. That is the price, and it is
        // paid on purpose — a filter leaves a gap, and a defect walks through
        // it, measured.
        let ends = body
            .get(at..)
            .and_then(|rest| rest.find("\n}\n"))
            .expect("and the runner closes")
            + at;
        // **Compared trimmed, and reported one line at a time.** The `held`
        // vector is long; an `assert_eq!` over it renders two large `Debug`
        // dumps with the one differing element buried in the middle, and a
        // developer rewording the diagnosis meets that rather than a sentence.
        // And the wrapped literal's continuation lines sit at a column rustfmt
        // will not touch while it re-indents the code around them — so,
        // untrimmed, interior indentation would be load-bearing in the pin, and a
        // no-op nesting change would turn it red. Neither is what this holds: it holds which
        // statements are there and in what order.
        let body_of_the_runner: Vec<&str> = body[at..ends]
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let held = THE_RUNNER_AS_WRITTEN;
        // **And nothing else in this file writes a project flag at all.** The
        // block above is held, and the slice it spends is the caller's — so a
        // call site four hundred lines up could name a project the pin never
        // sees. `compose` refuses one at run time, and this says the refusal
        // is never reached, because a `cargo xtask dev` that fails on its own
        // arguments is a defect that shipped rather than one that was caught.
        let elsewhere: Vec<&str> = body[..at]
            .lines()
            .chain(body[ends..].lines())
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| line.contains("\"--project-name") || line.contains("\"-p"))
            // **And the two halves agree about `run -p`.** The rule above lets a
            // published port after the subcommand through — `run`'s `-p` is
            // `--publish` — and a needle reading that same slice as a project
            // flag would refuse what the other half blesses: planted, the legal
            // slice goes red without this filter.
            .filter(|line| {
                let after_the_verb = line
                    .find("\"run\"")
                    .zip(line.find("\"-p"))
                    .is_some_and(|(verb, flag)| verb < flag);
                !after_the_verb
            })
            .map(str::trim)
            .collect();
        assert!(
            elsewhere.is_empty(),
            "this file writes a project flag somewhere other than the one place that \
             names the project: {elsewhere:?}. Docker resolves the flag to the later of \
             two, so the stack would come up under a name `--down` does not address"
        );
        let said = "the one place this file runs `docker compose` is not what is written \
                    down. Read it whole: which project the stack belongs to, which \
                    directory compose reads its file from, whether the output goes \
                    through or is captured, and which stream a captured reader is \
                    handed are all decided here";
        if let Some((line, (found, want))) = body_of_the_runner
            .iter()
            .zip(held.iter())
            .enumerate()
            .find(|(_, (found, want))| found.trim() != want.trim())
        {
            panic!("{said}\n  line {line} is `{found}`\n  and is written down as `{want}`");
        }
        assert_eq!(
            body_of_the_runner.len(),
            held.len(),
            "{said}\n  it holds {} statement(s) and {} are written down",
            body_of_the_runner.len(),
            held.len()
        );
    }

    /// The one place this file runs `docker compose`, written down.
    ///
    /// Lifted out of [`the_block_that_decides_isolation_is_the_one_written_down`]
    /// at clippy's hundred-line bound. It is the runner's whole body, trimmed
    /// line by line, because a filter between the region and the judgement is a
    /// gap a defect walks through — the one measured beside the comparison in
    /// that test.
    #[cfg(test)]
    const THE_RUNNER_AS_WRITTEN: [&str; 115] = [
        "// **And a caller cannot name a project either.** The block below is held",
        "// whole, and `.args(args)` sits inside it — so the one channel the pin",
        "// leaves open is the slice itself: `compose_command(root, &[\"--project-name\",",
        "// \"noctua-central\", \"up\", \"-d\", …])` brings the stack up under the shared",
        "// name with the whole suite green without this refusal, measured, because",
        "// the walk that reads this file discards everything",
        "// above the first `Command::new(\"docker\")` and the call site is four hundred",
        "// lines above it. Docker takes the later of two, so `--down` would then",
        "// address a project that never ran.",
        "//",
        "// Refused here rather than pinned at three call sites, because this line is",
        "// inside the held block and a text rule over call sites is one more thing",
        "// that reads text.",
        "// **Among the top-level flags, and every spelling of them.** Docker takes",
        "// `-p x`, `-px`, `-p=x`, `--project-name x` and `--project-name=x` alike and",
        "// resolves the flag to the *later* of two — so asking for the exact token",
        "// `-p` and the long form only lets `-pnoctua-central` at a call site bring",
        "// every checkout up under one project with the suite green.",
        "//",
        "// **And only the top-level ones, because `run`'s `-p` is `--publish`.** A",
        "// rule over the whole slice refuses `compose(root, &[\"run\", \"-p\",",
        "// \"8080:80\", …])` with a message about project names, which is the",
        "// too-strong half. The distinction is",
        "// positional: docker's own flags come before the subcommand, so the scan",
        "// stops at the first argument that is not a flag. That is the cheaper way",
        "// to draw the line: it needs no second parameter.",
        "//",
        "// **And a top-level flag may take a separated value.** A `take_while` over",
        "// flags stops at the first token that is not one — so `--ansi never -p",
        "// shared up` slips past it, because `never` is the *value* of `--ansi` and",
        "// stands before the `-p`, measured. These are the flags that take one:",
        "// `--ansi`, `-f`, `--profile`, `--progress`, `--env-file`,",
        "// `--project-directory` and `--parallel`.",
        "let takes_a_value = |it: &str| {",
        "matches!(",
        "it,",
        "\"-f\" | \"--file\"",
        "| \"--ansi\"",
        "| \"--profile\"",
        "| \"--progress\"",
        "| \"--env-file\"",
        "| \"--project-directory\"",
        "| \"--parallel\"",
        ")",
        "};",
        "let mut before_the_verb = Vec::new();",
        "let mut its_value = false;",
        "for it in args {",
        "if its_value {",
        "its_value = false;",
        "continue;",
        "}",
        "if !it.starts_with('-') {",
        "break;",
        "}",
        "before_the_verb.push(*it);",
        "its_value = takes_a_value(it) && !it.contains('=');",
        "}",
        "if let Some(named) = before_the_verb.iter().find(|it| {",
        "it.starts_with(\"--project-name\") || (it.starts_with(\"-p\") && !it.starts_with(\"--\"))",
        "}) {",
        "return Err(format!(",
        "\"`{named}` was passed to `docker compose` among its arguments. The project \\",
        "is this checkout's and is set above; docker resolves the flag to the later \\",
        "of two, so the stack would come up under a name `--down` does not address\"",
        "));",
        "}",
        "#[expect(",
        "clippy::disallowed_methods,",
        "reason = \"a tool this gate drives; its output is read, not printed\"",
        ")]",
        "let mut command = Command::new(\"docker\");",
        "command",
        ".arg(\"compose\")",
        ".args([\"-p\", &project_for(root)])",
        ".args(args)",
        "// **The checkout's ports, handed to every invocation.** The file",
        "// writes no port of its own, so `up` and `config` refuse without them,",
        "// and so does `ps` of a project that is not running, while `down` and",
        "// `logs` of a running one do not ask, measured. Handed to all, no",
        "// invocation depends on which verbs compose resolves the file for.",
        ".envs(Port::ALL.map(|which| (which.variable(), port(root, which))))",
        ".current_dir(root);",
        "match want {",
        "Compose::Through => {",
        "let status = command.status().map_err(|error| unavailable(&error))?;",
        "if status.success() {",
        "Ok(String::new())",
        "} else {",
        "Err(format!(",
        "\"{} failed. Docker's own words are above this line and they are the \\",
        "diagnosis; the three this fails for are an image that could not be fetched \\",
        "— a first run on a machine that has never had them pulls both — a daemon \\",
        "that is not running or that your user may not reach, and a port already \\",
        "taken, which `POSTGRES_PORT` and `LOGS_PORT` are for\",",
        "ran(root, args)",
        "))",
        "}",
        "}",
        "Compose::Captured(stream) => {",
        "let output = command.output().map_err(|error| unavailable(&error))?;",
        "if !output.status.success() {",
        "return Err(format!(",
        "\"{} failed: {}\",",
        "ran(root, args),",
        "String::from_utf8_lossy(&output.stderr)",
        "));",
        "}",
        "Ok(String::from_utf8_lossy(match stream {",
        "Stream::Stdout => &output.stdout,",
        "Stream::Stderr => &output.stderr,",
        "})",
        ".into_owned())",
        "}",
        "}",
    ];

    /// An argument that names a project of its own is refused.
    ///
    /// **The one channel the body pin leaves open.** `.args(args)` is inside the
    /// held block, and `args` is whatever a caller passes: without this refusal,
    /// `--project-name noctua-central` in `run`'s slice brings the stack up under
    /// the shared name with the whole suite green, measured, because the walk
    /// that reads this file discards everything above the first
    /// `Command::new("docker")` and the call site is four hundred lines above
    /// it. Docker resolves the flag to the later of two.
    ///
    /// This needs no docker: the refusal returns before anything is spawned.
    #[test]
    fn an_argument_that_names_a_project_of_its_own_is_refused() {
        // Every spelling docker accepts, including the two that carry the name
        // on the flag itself — and `up` after a flag, which the scan reaches.
        for named in [
            vec!["--ansi", "never", "-p", "noctua-central", "up"],
            vec!["-f", "docker-compose.yml", "-p", "noctua-central", "up"],
            vec![
                "--project-directory",
                ".",
                "--project-name",
                "noctua-central",
                "up",
            ],
            vec!["--project-name", "noctua-central", "up"],
            vec!["--project-name=noctua-central", "up"],
            vec!["-p", "noctua-central", "up"],
            vec!["-pnoctua-central", "up"],
            vec!["-p=noctua-central", "up"],
        ] {
            let refused = super::compose(
                std::path::Path::new("/nonexistent/noctua-central"),
                &named,
                super::Compose::Through,
            )
            .expect_err("a second project flag cannot be run");
            assert!(
                refused.contains("among its arguments"),
                "`{named:?}` was not refused for naming a project: {refused}"
            );
        }
        // **And the too-strong half.** `run`'s
        // `-p` is `--publish`, so a rule over the whole slice refuses a
        // legitimate published port with a message about project names. The
        // scan stops at the subcommand; this reaches the spawn and fails on
        // docker being absent or refusing, not on its own arguments.
        let published = super::compose(
            std::path::Path::new("/nonexistent/noctua-central"),
            &["run", "-p", "8080:80", "--rm", "postgres", "true"],
            super::Compose::Captured(super::Stream::Stdout),
        );
        assert!(
            published
                .as_ref()
                .err()
                .is_none_or(|said| !said.contains("among its arguments")),
            "a published port after the subcommand was refused as a project name: \
             {published:?}"
        );
    }

    /// Each reader asks for what it is written to ask for.
    ///
    /// Split from [`the_block_that_decides_isolation_is_the_one_written_down`]
    /// at clippy's hundred-line bound, and separate on its own merits: that
    /// rule holds the command that gets built, and this one holds what each
    /// caller asks of it.
    #[test]
    fn each_reader_asks_for_the_stream_it_is_written_for() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is readable");
        let body = source
            .split_once("\nmod tests {")
            .expect("the test module is below")
            .0;
        let code = crate::gates::without_comments(body, "//");
        // **And the variant each reader passes.** With the builder written once,
        // a whole function body is one token at each call site: planted,
        // `compose_command` asking for `Captured(Stdout)` instead of `Through`
        // leaves the rest of the suite green while `cargo xtask dev` swallows
        // docker's entire transcript during a two-image pull and the long
        // diagnosis above becomes unreachable from every caller. Their bodies
        // are one line each, so they are held whole.
        for (named, returns, spends) in [
            (
                "compose_command",
                "Result<(), String>",
                vec!["compose(root, args, Compose::Through)?;", "Ok(())"],
            ),
            (
                "what_compose_reported",
                "Result<String, String>",
                vec!["compose(root, args, Compose::Captured(Stream::Stderr))"],
            ),
            (
                "compose_output",
                "Result<String, String>",
                vec!["compose(root, args, Compose::Captured(Stream::Stdout))"],
            ),
        ] {
            let takes = format!("fn {named}(root: &Path, args: &[&str]) -> {returns} {{\n");
            let from = code
                .find(&takes)
                .unwrap_or_else(|| panic!("`{named}` reads what compose printed"))
                + takes.len();
            let to = code[from..].find("\n}\n").expect("and it closes") + from;
            let reads: Vec<&str> = code[from..to]
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect();
            assert_eq!(
                reads, spends,
                "`{named}` does not ask for what it is written to ask for. The variant \
                 is the whole of what tells output-through from captured, and which \
                 stream a captured reader is handed"
            );
        }
    }

    /// A binding of the project is counted however it is written.
    ///
    /// Split from [`every_compose_invocation_names_its_project`] at clippy's
    /// hundred-line bound. The two are one
    /// question asked of two things: which invocations in this file name a
    /// project, and what the predicate counts as a binding of the name.
    #[test]
    fn a_binding_of_the_project_is_counted_however_it_is_written() {
        // **A hoist in every function is still a hoist.** Asking for exactly one
        // binding of the name in the whole file makes hoisting the call in each
        // of the three functions that start a compose command — ordinary,
        // `fmt`-clean, identical behaviour — report all three as taking the
        // project from the directory's name, measured. What is refused is a
        // binding that comes from somewhere else, and that
        // is what these two cases say.
        let hoisted_everywhere = concat!(
            "let project = project_for(root);\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n",
            "let project = project_for(root);\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n"
        );
        assert!(
            names_its_project(
                "Command::new(\"docker\").args([\"-p\", &project]);",
                hoisted_everywhere
            ),
            "the same call hoisted in two functions was read as two different projects"
        );
        let rewritten = concat!(
            "let project = project_for(root);\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n",
            "let project = project_for(root).replace(\"a\", \"b\");\n"
        );
        assert!(
            !names_its_project(
                "Command::new(\"docker\").args([\"-p\", &project]);",
                rewritten
            ),
            "a binding of the same name taken from somewhere else was accepted"
        );
        // **A type annotation is a binding.** `let project: String = …` holds no
        // `let project = ` at all, so a count of bindings does not see it and
        // one pinned binding elsewhere in the file pays for it. Planted, it sits
        // in the function that runs `down -v`.
        let annotated = concat!(
            "let project = project_for(root);\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n",
            "let project: String = \"nc-fixed\".to_owned();\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n"
        );
        assert!(
            !names_its_project(
                "Command::new(\"docker\").args([\"-p\", &project]);",
                annotated
            ),
            "a binding written with a type annotation was invisible, so an invocation \
             took its project from somewhere else and this said every one names it"
        );
        // And `let mut`, for the same reason.
        let mutable = concat!(
            "let project = project_for(root);\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n",
            "let mut project = String::new();\n"
        );
        assert!(
            !names_its_project(
                "Command::new(\"docker\").args([\"-p\", &project]);",
                mutable
            ),
            "a `let mut` binding of the same name was invisible"
        );

        // **And a `let mut` is never a *pin*, however it is initialised.** The
        // value this rule compares is the initialiser, and a statement after it
        // is not a binder — so a hoist followed by an ordinary mutation counts
        // as one binding, one pinned, and the gate reports the invocation as
        // naming what this tree computes. `truncate` is the cheap one: it
        // yields `noctua-central`, the directory's own name, which is the
        // collision `project_for` exists to prevent. Measured against
        // `compose_command`, the function that runs `up -d --remove-orphans`
        // and `down -v`.
        for mutated in [
            "let mut project = project_for(root);\nproject.push_str(\"-planted\");\n",
            "let mut project = project_for(root);\nproject.truncate(4);\n",
            "let mut project = project_for(root);\nproject = elsewhere();\n",
        ] {
            let code = format!("{mutated}Command::new(\"docker\").args([\"-p\", &project]);\n");
            assert!(
                !names_its_project("Command::new(\"docker\").args([\"-p\", &project]);", &code),
                "a binding that can be changed after it is made was read as pinning the \
                 project: {mutated}"
            );
        }
    }

    /// Every clause of the binder walk decides something.
    ///
    /// **Eight clauses, and the tables above reach none of them.** The two
    /// tables above drive the five *arms* of [`the_names_a_binder_introduces`]
    /// — a `let` head, a `for` head, a function's parameters, a closure's
    /// parameters, a `match` arm — and not the guards, cuts and boundary tests
    /// inside them: without this test each of those deleted alone leaves the
    /// suite green, measured, and `xtask/mutants.py` carries no operator over
    /// this function at all.
    ///
    /// Two of them protect *correct* code and are otherwise undecided in both
    /// directions: the word-boundary guards, without which `outlet ` and
    /// `effn ` are binders, and the `||` exclusion, without which an ordinary
    /// or-expression is a closure head.
    #[test]
    fn every_clause_of_the_binder_walk_decides_something() {
        // **A count, not a predicate over the heads.** `let` and `fn` inside a
        // longer word introduce nothing, and without the boundary guards each
        // of these yields a *second*, spurious binder — so the rule counts a
        // binding that is not in the file and a real pin can be balanced out by
        // it. Asserted as the whole answer, because a test that only asks
        // whether some head mentions a name is answered the same way either
        // way, which is the defect itself written into its own fixture.
        for (not_a_binder, whole) in [
            ("let outlet = elsewhere();\n", [("outlet ", " elsewhere()")]),
            ("let effn = elsewhere();\n", [("effn ", " elsewhere()")]),
        ] {
            assert_eq!(
                the_names_a_binder_introduces(not_a_binder),
                whole.to_vec(),
                "`{not_a_binder}` holds one binder, and the walk found something else"
            );
        }

        // `||` is an or-expression, not a closure with no parameters.
        assert_eq!(
            the_names_a_binder_introduces("let ok = here() || project_for(root);\n"),
            [("ok ", " here() || project_for(root)")].to_vec(),
            "an or-expression was read as a closure head"
        );
        assert_eq!(
            the_names_a_binder_introduces("let f = move |project: String| { drop(project) };\n"),
            [
                ("f ", " move |project: String| { drop(project) }"),
                ("project: String", ""),
            ]
            .to_vec(),
            "a `move` closure's parameter is a binding, and the walk missed it"
        );

        // A closure head ends at its closing `|`, and a match arm's pattern
        // ends before its guard — otherwise each swallows the expression after
        // it and reads names out of a body.
        let closure =
            the_names_a_binder_introduces("let f = |project: String| { let elsewhere = 1; };\n");
        assert!(
            closure
                .iter()
                .any(|(head, _)| head.contains("project") && !head.contains("elsewhere")),
            "a closure head ran past its own parameters: {closure:?}"
        );
        let arm = the_names_a_binder_introduces(
            "        Some(project) if elsewhere(project) => here(),\n",
        );
        assert!(
            arm.iter()
                .any(|(head, _)| head.contains("project") && !head.contains("elsewhere")),
            "a match arm's pattern ran into its guard: {arm:?}"
        );
    }

    /// A binding written as a pattern or a parameter is counted too.
    ///
    /// Kept apart from [`a_binding_of_the_project_is_counted_however_it_is_written`]
    /// at clippy's hundred-line bound, and separate on its own merits: every
    /// spelling there writes `let <name>` as two adjacent words
    /// and every spelling here does not, which is the difference the walk over
    /// the grammar exists for.
    #[test]
    fn a_binding_written_as_a_pattern_or_a_parameter_is_counted_too() {
        // **A pattern is a binding.** Three spellings, none of which holds
        // `let project` as adjacent words: a tuple, a refutable pattern, and a
        // loop's own binding. Planted in `compose_command`, the function that
        // runs `up -d --remove-orphans` and `down -v`, each leaves a rule that
        // reads `let` heads green.
        for pattern in [
            "let (project,) = (String::new(),);\n",
            "let Some(project) = maybe else { return };\n",
            "for project in [\"nc-fixed\"] {}\n",
        ] {
            let patterned = format!(
                "{}{}{pattern}",
                "let project = project_for(root);\n",
                "Command::new(\"docker\").args([\"-p\", &project]);\n"
            );
            assert!(
                !names_its_project(
                    "Command::new(\"docker\").args([\"-p\", &project]);",
                    &patterned
                ),
                "a binding written as a pattern was invisible, so an invocation took its \
                 project from somewhere else and this said every one names it: {pattern}"
            );
        }
        // **And a parameter is a binding.** A closure's, a function's and a
        // `match` arm's: none of the three has a `let` or a `for` head, so a
        // rule reading heads alone cannot see them. The closure case is
        // measured — `compose_command`'s invocation moved into
        // `|project: String|`, with one legitimate hoist elsewhere paying for
        // it — and under such a rule it leaves the whole gate green while both
        // compose verbs run under the fixed project every checkout is called.
        for introduced in [
            "let under = |project: String| { drop(project) };\n",
            "fn under(project: String) { drop(project) }\n",
            "match it { Some(project) => drop(project), None => () }\n",
        ] {
            let elsewhere = format!(
                "{}{}{introduced}",
                "let project = project_for(root);\n",
                "Command::new(\"docker\").args([\"-p\", &project]);\n"
            );
            assert!(
                !names_its_project(
                    "Command::new(\"docker\").args([\"-p\", &project]);",
                    &elsewhere
                ),
                "a parameter is a binding and this did not see it: {introduced}"
            );
        }
        // **And a hoist written with a type annotation is a hoist.** A rule
        // that *sees* that spelling and pins only the bare literal refuses it,
        // and an ordinary line prints the most alarming message this tool has.
        // A binding is pinned by its **value**.
        let annotated_hoist = concat!(
            "let project: String = project_for(root);\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n"
        );
        assert!(
            names_its_project(
                "Command::new(\"docker\").args([\"-p\", &project]);",
                annotated_hoist
            ),
            "a hoist written with a type annotation was read as a binding from somewhere else"
        );
        // **And a use is not a binding**, which is the direction that would
        // refuse correct code: a loop over something the name merely appears in,
        // and a `let` of another name holding it, are neither of them bindings
        // of it.
        for legitimate in [
            "for entry in project.chars() {}\n",
            "let flag = format!(\"-p {project}\");\n",
            "let projects = vec![project];\n",
        ] {
            let beside = format!(
                "{}{}{legitimate}",
                "let project = project_for(root);\n",
                "Command::new(\"docker\").args([\"-p\", &project]);\n"
            );
            assert!(
                names_its_project(
                    "Command::new(\"docker\").args([\"-p\", &project]);",
                    &beside
                ),
                "a line that merely uses the name was read as a second binding: {legitimate}"
            );
        }
        // **And a longer name that merely opens with this one is not this one**,
        // which is the direction that would refuse correct code.
        let neighbour = concat!(
            "let project = project_for(root);\n",
            "Command::new(\"docker\").args([\"-p\", &project]);\n",
            "let project_root = root.to_owned();\n"
        );
        assert!(
            names_its_project(
                "Command::new(\"docker\").args([\"-p\", &project]);",
                neighbour
            ),
            "a binding whose name merely opens with this one was read as a second binding"
        );
    }

    /// The id the `docker inspect` exemption rests on came from a
    /// project-scoped `compose ps -q`, and nothing touches it on the way.
    ///
    /// Split from the invocation rule at clippy's hundred-line bound. The two
    /// are one question asked of two things: which invocations name a project,
    /// and where the one exempted invocation's argument comes from.
    #[test]
    fn the_inspected_id_comes_from_a_project_scoped_read() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is readable");
        let body = source
            .split_once("\nmod tests {")
            .expect("the test module is below")
            .0;
        // **Every comment, not the one spelling.** `line_without_a_comment`
        // reads a line at a time and knows `//`, so a `/* … */` holding
        // `.args(["-p", &project_for(root)])` beside a live invocation makes the
        // chunk's flag count 2 — and one occurrence is all the rule below asks
        // for, measured. `without_comments` is a state machine
        // over the whole file: a block comment nests, a raw string ends at its
        // own hash count, and neither opens inside a string.
        let code = crate::gates::without_comments(body, "//");

        // **And where that `id` came from.** The exemption asks for the binding
        // by name, and a name is not a provenance: one line added inside
        // `where_the_logs_container_mounts` — `let id = "noctua-central-logs-1";`
        // after the project-scoped read — leaves a name-only rule green and
        // points the command at the container a developer's own checkout
        // produces, measured. So the file has to bind `id` from
        // `compose ps -q` and must not bind it again between there and the
        // `inspect`.
        // **And the lines that hand the id over, held whole.** A spelling
        // (`let id = `), a count of `let`, and a name in the callee's body are
        // each a rule about the text between the project-scoped read and the
        // command, and each leaves somewhere open: a scan of the callee leaves
        // the caller, so `let id = "noctua-central-logs-1";` on the line above
        // the hand-over passes it.
        //
        // There is nowhere to put that line: the caller is a `match` whose
        // binding's only use is the call, and the whole block is compared. A
        // statement changed there is a failure that says to read four lines
        // rather than a number to move.
        // **The whole body**, from the signature to its closing brace. Four
        // lines pinned say they are written somewhere in the function rather
        // than that they are the function: three lines above them, returning
        // the mount of a container named by the directory's own project, leave
        // four such pins green and `check passed`. Every statement is compared;
        // comments are stripped first, so the comment in the body is free to say
        // what it holds.
        let opens = format!(
            "pub(crate) fn {}(root: &Path) -> Result<Option<String>, String> {{\n",
            "where_the_logs_container_mounts"
        );
        let at = code
            .find(&opens)
            .expect("the reader that takes the project-scoped id is here")
            + opens.len();
        let ends = code[at..].find("\n}\n").expect("and it closes") + at;
        // `code` is comment-stripped, so a comment line arrives as an empty one:
        // what is left after dropping those is every *statement* the body has.
        let body: Vec<&str> = code[at..ends]
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let held = vec![
            format!(
                "    match {}(root, &[\"ps\", \"-q\", \"logs\"])?.trim() {{",
                "compose_output"
            ),
            "        \"\" => Ok(None),".to_owned(),
            format!("        id => {}(id),", "where_that_container_mounts"),
            "    }".to_owned(),
        ];
        assert_eq!(
            body, held,
            "the body that hands the container id from a project-scoped `compose ps -q` \
             to the `docker inspect` is not the one written down. Read it whole: \
             anything else in this function is a command addressing whatever project \
             named its argument"
        );
        let takes = format!("fn {}(id: &str)", "where_that_container_mounts");
        let at = code
            .find(&takes)
            .expect("the reader that takes the id is here")
            + takes.len();
        let runs = code[at..]
            .find("Command::new(\"docker\")")
            .expect("the command is below the signature");
        let between = &code[at..at + runs];
        let rebound = between.match_indices("id").any(|(where_, _)| {
            let before = between[..where_].chars().next_back();
            let after = between[where_ + 2..].chars().next();
            before.is_none_or(|it| !it.is_alphanumeric() && it != '_')
                && after.is_none_or(|it| !it.is_alphanumeric() && it != '_')
        });
        assert!(
            !rebound,
            "`id` is named between the reader's signature and the command it is handed \
             to, so the value the `docker inspect` exemption rests on is not the one a \
             project-scoped `compose ps -q` returned: {between}"
        );

        // **And the hole, not just the rule.** Which subcommand ran is not the
        // property: the sentence justifying the exemption is about what the
        // subcommand is *handed*. Measured with a subcommand-only exemption: the one exempted call
        // pointed at `"noctua-central-logs-1"` — the container name a
        // developer's own checkout produces — and this test stayed green, so
        // the gate whose whole subject is a copy reaching another project's
        // stack was satisfied by a command that does exactly that.
        for (scoped, fixture) in [
            (
                true,
                "Command::new(\"docker\").args([\"inspect\", id, \"--format\", \"{{.Name}}\"])",
            ),
            (
                false,
                "Command::new(\"docker\").args([\"inspect\", \"noctua-central-logs-1\"])",
            ),
            (
                false,
                "Command::new(\"docker\").args([\"compose\", \"up\", \"-d\"])",
            ),
            (
                true,
                "Command::new(\"docker\").args([\"compose\", [\"-p\", &project_for(root)], \"up\"])",
            ),
        ] {
            assert_eq!(
                the_invocations_that_name_no_project(fixture).is_empty(),
                scoped,
                "`{fixture}` was read as {} its project",
                if scoped { "not naming" } else { "naming" }
            );
        }

        // And the exemption is one subcommand long, and it is the one that takes
        // an id rather than a project.
        assert_eq!(
            code.matches("\"inspect\"").count(),
            1,
            "the invocation exemption covers `inspect` and this file names it {} time(s): \
             every other `docker` invocation has to name its project",
            code.matches("\"inspect\"").count()
        );
    }

    /// Every message that spells a `docker compose` command names its project.
    ///
    /// **Split from the invocation half at clippy's hundred-line bound**, which
    /// this crate pays rather than allows. The two halves are one rule asked of
    /// two things: what the verb
    /// *runs*, and what it *hands a developer to run*.
    #[test]
    fn every_compose_message_names_its_project() {
        // The one `docker compose` subcommand that is not project-scoped: `ls`
        // lists every project on the machine, which is how a developer finds a
        // stack another checkout left running.
        const NOT_SCOPED: [&str; 1] = ["ls"];
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is readable");
        let body = source
            .split_once("\nmod tests {")
            .expect("the test module is below")
            .0;

        // **And every message that spells one names it too.** The assertion
        // above is about the *invocation*; a failure message printing
        // `` `docker compose up -d --remove-orphans` failed `` with no `-p`
        // hands a developer a command addressing the directory's project —
        // which is by construction never the one that ran. Running it lists
        // nothing, and running `up` with it creates a second stack.
        let mut bare = Vec::new();
        for (index, line) in body.lines().enumerate() {
            // Code, not the prose that explains it: the doc on `failed` writes
            // the shape of a bare command in order to say it is wrong.
            let line = crate::gates::line_without_a_comment(line);
            // **Every occurrence, not the first.** `find` stops at one, so a
            // scoped command earlier on the same source line hides a bare one
            // after it — and the bare one is the command a developer is handed.
            // Measured: `` `docker compose -p {} ps` … `docker compose up -d` ``
            // passed, and the same line with the two swapped was a finding.
            for (at, _) in line.match_indices("`docker compose ") {
                let rest = &line[at..];
                if rest.starts_with("`docker compose -p ") {
                    continue;
                }
                // **One subcommand is not project-scoped, and it is the point of the
                // sentence it appears in.** `docker compose ls` lists *every*
                // project on the machine, which is how a developer finds the stack
                // another checkout left running — `-p` would narrow it to the one
                // they already know about. Written as a list so the hole is one
                // name and the test below can say what is in it.
                if NOT_SCOPED
                    .iter()
                    .any(|verb| rest.starts_with(&format!("`docker compose {verb}")))
                {
                    continue;
                }
                let found = format!("  dev.rs:{}: {}", index + 1, line.trim());
                bare.push(found);
            }
        }
        assert!(
            bare.is_empty(),
            "{} place(s) write a `docker compose` command without `-p`:\n{}\nThe project \
             is path-derived, so a command without it addresses a different stack than \
             the one being described",
            bare.len(),
            bare.join("\n")
        );
        // And the hole is tested, not just the rule: a bare `up` in a message is
        // still a finding, and the exemption is one name long.
        assert_eq!(
            NOT_SCOPED,
            ["ls"],
            "the exemption grew. Every `docker compose` subcommand except `ls` acts on \
             one project, so a message spelling one without `-p` names a stack that is \
             not the one it is about"
        );
        let probe = "        ui::detail(\"run `docker compose up -d` again\");";
        assert!(
            crate::gates::line_without_a_comment(probe).contains("`docker compose up"),
            "the scan's own needle no longer matches a bare command"
        );
        assert!(
            !NOT_SCOPED
                .iter()
                .any(|verb| probe.contains(&format!("`docker compose {verb}"))),
            "the exemption covers a bare `up`, which is the case this rule is for"
        );
    }

    /// And the project a checkout gets is its own, and stays its own.
    ///
    /// **A constant fails both halves.** `-p noctua-central`, written out,
    /// makes every directory on the machine address one stack: measured, the
    /// live database is reached from a scratch directory holding a two-line toy
    /// compose file and nothing else. The name has to separate checkouts, and it has to
    /// keep naming the same volumes tomorrow, or a developer's data is orphaned
    /// by a rename nobody asked for.
    #[test]
    fn the_project_name_does_not_move_under_a_checkout() {
        use std::path::Path;

        // Frozen. If this number changes, every existing checkout's stack is
        // renamed and its volumes are stranded under the old name. **Through
        // the project, since the digest moved to `nc_channel`**, whose own
        // tests hold it as a number; a path that does not exist is hashed as
        // written, on every system.
        assert_eq!(
            super::project_for(Path::new("/home/dev/noctua-central")),
            "noctua-central-5e24507c"
        );
        assert_eq!(super::project_for(Path::new("")), "noctua-central-811c9dc5");
        // **And the rendering, not only the digest.** `{:08x}` written `{:x}`
        // leaves both digests above unchanged, and drops the leading zero for
        // one checkout path in sixteen — renaming its project and stranding the
        // volumes the paragraph above says this exists to protect.
        assert_eq!(
            super::project_for(Path::new("/nonexistent/9/noctua-central")),
            "noctua-central-05d42f30"
        );

        // **A path that exists, so the canonicalizing branch is the one under
        // test.** A `/nonexistent/…` path takes the fallback — so with only
        // those, deleting the `canonicalize` call the doc comment calls
        // load-bearing leaves every case here green, and a symlinked checkout
        // and its target would be two stacks.
        let scratch = std::env::temp_dir().join(format!("nc-project-{}", std::process::id()));
        let real = scratch.join("noctua-central");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&real).expect("writable");
        // A symbolic link on Unix and a junction on Windows: the link a person
        // makes to a directory on each, and one `canonicalize` sees through.
        let linked = scratch.join("by-another-name");
        nc_platform::testing::link_directory(&real, &linked).expect("a link");
        assert_eq!(
            super::project_for(&real),
            super::project_for(&linked),
            "a linked checkout and its target were given two stacks, so a \
             `dev --down` through one would leave the other's containers"
        );
        let _ = std::fs::remove_dir_all(&scratch);

        let one = super::project_for(Path::new("/nonexistent/a/noctua-central"));
        let other = super::project_for(Path::new("/nonexistent/b/noctua-central"));
        assert_ne!(
            one, other,
            "two checkouts of this repository are two directories with the same name, \
             which is the case the project name exists to separate"
        );
        assert_eq!(
            one,
            super::project_for(Path::new("/nonexistent/a/noctua-central")),
            "the same checkout was given two different stacks"
        );
        // Readable in `docker compose ls`, and legal as a Compose project name,
        // which must be lowercase alphanumerics, `_` or `-`.
        assert!(one.starts_with("noctua-central-"), "{one}");
        assert!(
            one.chars().all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'),
            "{one} is not a legal Compose project name"
        );
    }

    /// A checkout's stack binds the checkout's ports, unless a variable moves
    /// one.
    ///
    /// **The half of "two checkouts' stacks run at once" this file decides.**
    /// `nc_channel` derives the pair and its own suite holds the formula; what
    /// is decided here is that the stack takes it. Each case would pass a
    /// reader that did otherwise — a fixed port for every checkout, the two
    /// ports swapped, the variable ignored, or an empty variable read as a
    /// port — only if that case were not here.
    #[test]
    fn a_stack_binds_its_checkouts_ports_unless_a_variable_moves_one() {
        use super::Port;
        use std::path::Path;

        let one = Path::new("/nonexistent/one/noctua-central");
        let other = Path::new("/nonexistent/other/noctua-central");
        let (ones, others) = (
            nc_channel::Checkout::of(one).stack(),
            nc_channel::Checkout::of(other).stack(),
        );
        for (root, stack) in [(one, &ones), (other, &others)] {
            assert_eq!(
                super::port_of(root, Port::Postgres, None),
                stack.postgres.to_string(),
                "PostgreSQL is not on the port this checkout derives"
            );
            assert_eq!(
                super::port_of(root, Port::Logs, None),
                stack.logs.to_string(),
                "the log store is not on the port this checkout derives"
            );
            // **Set empty is unset**, as compose reads `${NAME:?…}`: an
            // `.env.example` sourced whole writes both ports empty.
            for which in Port::ALL {
                assert_eq!(
                    super::port_of(root, which, Some(String::new())),
                    super::port_of(root, which, None),
                    "`{}` set empty moved the port",
                    which.variable()
                );
                assert_eq!(
                    super::port_of(root, which, Some("5433".to_owned())),
                    "5433",
                    "`{}` did not move the port",
                    which.variable()
                );
            }
        }
        // **Two checkouts, two pairs**, which is the point; these two paths'
        // digests fall on different pairs of the block.
        let taken: std::collections::BTreeSet<String> = [one, other]
            .iter()
            .flat_map(|root| Port::ALL.map(|which| super::port_of(root, which, None)))
            .collect();
        assert_eq!(
            taken.len(),
            4,
            "two checkouts' stacks share a port: {taken:?}"
        );
        // And the settings read the same rule.
        assert_eq!(super::as_compose_reads(Some(String::new())), None);
        assert_eq!(super::as_compose_reads(None), None);
        assert_eq!(
            super::as_compose_reads(Some("hunter2".to_owned())),
            Some("hunter2".to_owned())
        );
    }

    /// A port something else holds fails `dev` naming the variable that moves
    /// it, and one this stack holds is its own.
    ///
    /// **Against a real listener, held for the whole test**, so the port
    /// cannot be free; the free case is not driven, because a port released to
    /// prove it free is one any parallel test may take. What compose says this
    /// project publishes is handed in, so no docker is asked.
    #[test]
    fn a_port_something_else_holds_fails_naming_the_variable_that_moves_it() {
        use super::Port;

        let held = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let port = held
            .local_addr()
            .expect("a bound listener has an address")
            .port();
        let at = port.to_string();
        for which in Port::ALL {
            let refused = super::free_or_this_stacks(which, &at, || None)
                .expect_err("a port held by a listener this project does not run");
            assert!(
                refused.contains(&format!("`{}`", which.variable()))
                    && refused.contains(&format!("127.0.0.1:{port}")),
                "the refusal does not name the port and the variable that moves it: {refused}"
            );
            // **This project's own**, as `docker compose port` prints it: a
            // second `dev` over a running stack.
            assert_eq!(
                super::free_or_this_stacks(which, &at, || Some(format!("127.0.0.1:{port}\n"))),
                Ok(()),
                "a port this stack publishes was refused, so every `dev` after the first fails"
            );
            // Published by this project, but on another port: not this one.
            for elsewhere in [format!("127.0.0.1:{port}0\n"), format!("0.0.0.0:{port}\n")] {
                assert!(
                    super::free_or_this_stacks(which, &at, || Some(elsewhere.clone())).is_err(),
                    "`{}` was read as this stack holding 127.0.0.1:{port}",
                    elsewhere.trim()
                );
            }
            // **And a variable that holds no port** is refused by name before
            // anything is bound.
            for wrong in ["abc", "0", "65536", "-1", " 20000"] {
                let refused =
                    super::free_or_this_stacks(which, wrong, || None).expect_err("not a port");
                assert!(
                    refused.contains(&format!("`{}` is", which.variable())),
                    "{wrong:?} was refused without naming its variable: {refused}"
                );
            }
        }
        drop(held);
    }

    /// A port is called held only when its bind says something holds it, and
    /// a bind refused for another reason is refused with that reason.
    ///
    /// **This found a real bug, by review.** Every failed bind was read as a
    /// holder, so a port refused to an unprivileged user, or an address this
    /// system lacks, was refused as *held by something this checkout's stack
    /// does not run* with nothing listening there. Measured with
    /// `POSTGRES_PORT=543`. The bind's answer is handed in, so each kind is
    /// driven without a privileged port, and so is a free port, which a real
    /// bind proves only by releasing one that another test may take.
    #[test]
    fn a_bind_refused_for_another_reason_is_refused_with_that_reason() {
        use super::Port;
        use std::io::{Error, ErrorKind};

        for which in Port::ALL {
            let variable = format!("`{}`", which.variable());
            // **Free, and compose never asked**: a free port costs no docker
            // call.
            assert_eq!(
                super::bound_or_this_stacks(which, 21_000, Ok(()), || {
                    panic!("compose was asked whether it publishes a port that is free")
                }),
                Ok(()),
                "a port the bind took was refused, so no stack can come up"
            );
            for kind in [
                ErrorKind::AddrInUse,
                ErrorKind::PermissionDenied,
                ErrorKind::AddrNotAvailable,
            ] {
                let said = format!("what the bind said, {kind:?}");
                // **This project's own, whatever the bind said**: a second
                // `dev` over a running stack.
                assert_eq!(
                    super::bound_or_this_stacks(
                        which,
                        543,
                        Err(Error::new(kind, said.clone())),
                        || { Some("127.0.0.1:543\n".to_owned()) }
                    ),
                    Ok(()),
                    "a port this stack publishes was refused after a bind answering {kind:?}"
                );
                let refused = super::bound_or_this_stacks(
                    which,
                    543,
                    Err(Error::new(kind, said.clone())),
                    || None,
                )
                .expect_err("a port that cannot be bound and that this stack does not publish");
                assert!(
                    refused.contains("127.0.0.1:543") && refused.contains(&variable),
                    "the refusal after {kind:?} does not name the port and {variable}: {refused}"
                );
                assert_eq!(
                    refused.contains("is held by something"),
                    kind == ErrorKind::AddrInUse,
                    "after a bind answering {kind:?}, the refusal names a holder when and \
                     only when the port is in use: {refused}"
                );
                assert!(
                    kind == ErrorKind::AddrInUse || refused.contains(&said),
                    "the refusal after {kind:?} does not say what the bind said: {refused}"
                );
            }
        }
    }

    /// Each port is asked of the service that publishes it, at the port inside
    /// that service's container.
    ///
    /// **What `dev` hands `docker compose port`**, held to the compose file
    /// both ways: the service [`super::Port::published_by`] names publishes
    /// the port's variable at the container port it names, and no other
    /// service reads that variable. **This found a real bug's hiding place,
    /// by review**: with the two arms swapped the suites that judge this file
    /// passed, 138 of 138, and `dev` over a running stack refused its own
    /// port. CI's one `dev` runs on a fresh runner, where both ports are free
    /// and the ask is never made.
    #[test]
    fn each_port_is_asked_of_the_service_that_publishes_it() {
        let compose = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docker-compose.yml"
        ))
        .expect("this repository has a compose file");
        // Each service's lines, as (indentation, text): a service is a key two
        // in under `services:`, and its block runs to the next key at that
        // depth or less. **What compose reads, and not its comments.**
        let mut services: std::collections::BTreeMap<&str, Vec<(usize, &str)>> =
            std::collections::BTreeMap::new();
        let mut under_services = false;
        let mut current: Option<&str> = None;
        for line in compose.lines() {
            let text = line.trim();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            let depth = line.len() - line.trim_start().len();
            if depth == 0 {
                under_services = text == "services:";
                current = None;
            } else if under_services && depth == 2 {
                let name = text.strip_suffix(':').unwrap_or(text);
                services.insert(name, Vec::new());
                current = Some(name);
            } else if let Some(lines) = current.and_then(|name| services.get_mut(name)) {
                lines.push((depth, text));
            }
        }
        assert!(
            services.len() >= super::Port::ALL.len(),
            "the scan read {:?} as the compose file's services, fewer than the stack's \
             ports, so it read the wrong text",
            services.keys().collect::<Vec<_>>()
        );
        for which in super::Port::ALL {
            let (service, inside) = which.published_by();
            let variable = which.variable();
            let lines = services.get(service).unwrap_or_else(|| {
                panic!(
                    "`{variable}` is asked of the service `{service}`, which \
                     `docker-compose.yml` does not declare: {:?}",
                    services.keys().collect::<Vec<_>>()
                )
            });
            // The items of its `ports:` list, and nothing below another key.
            let key = lines
                .iter()
                .position(|(_, text)| *text == "ports:")
                .unwrap_or_else(|| panic!("`{service}` publishes no port at all"));
            let published: Vec<&str> = lines[key + 1..]
                .iter()
                .take_while(|(depth, _)| *depth > lines[key].0)
                .filter_map(|(_, text)| text.strip_prefix("- "))
                .collect();
            let (opens, closes) = (
                format!("\"127.0.0.1:${{{variable}:?"),
                format!("}}:{inside}\""),
            );
            assert!(
                published.iter().any(|item| {
                    item.strip_prefix(&opens)
                        .and_then(|rest| rest.strip_suffix(&closes))
                        .is_some_and(|why| !why.contains('}'))
                }),
                "`{service}` does not publish `{variable}` from its container's port \
                 {inside}, so `docker compose port {service} {inside}` cannot answer where \
                 this stack holds it, and every `dev` over a running stack refuses its own \
                 port: {published:?}"
            );
            let read = format!("${{{variable}");
            for (other, lines) in &services {
                assert!(
                    *other == service || !lines.iter().any(|(_, text)| text.contains(&read)),
                    "`{other}` reads `{variable}` too, which `{service}` publishes, so two \
                     services answer for one port"
                );
            }
        }
    }

    use super::the_log_store_is_serving;
    use std::io::{Read as _, Write as _};

    /// A server that answers `status` once, then closes.
    ///
    /// Reads the request and shuts the write side down before closing: dropping
    /// a socket with unread data sends an RST, which destroys the reply about
    /// one run in eight. `nc-log`'s fake servers close the same way, for the
    /// same reason.
    fn answering(status: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let port = listener.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                let mut swallow = [0_u8; 512];
                let _ = socket.read(&mut swallow);
                let _ = socket.write_all(status.as_bytes());
                let _ = socket.shutdown(std::net::Shutdown::Write);
            }
        });
        port.to_string()
    }

    #[test]
    fn a_store_that_is_running_and_not_serving_is_not_a_pass() {
        // **The case the container's own healthcheck cannot see.** It runs
        // `--help`, which exits 0 while the process is alive — so a store that
        // answers 503, or answers nothing at all, reports `healthy` and `dev`
        // would print its URL.
        let refusing = answering("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n");
        let error = the_log_store_is_serving(&refusing).expect_err("503 is not serving");
        assert!(error.contains("503"), "{error}");
        assert!(
            error.contains("--help"),
            "the message does not say why: {error}"
        );

        // Nothing listening at all.
        //
        // **Port 0, not a port this bound and released.** Binding `127.0.0.1:0`,
        // reading the port and dropping the listener leaves a window in which
        // anything else on the machine can take it — and this suite starts
        // listeners on ephemeral ports in a dozen other tests, so under
        // `cargo test`'s own parallelism the connect sometimes succeeds and
        // this fails for a reason that has nothing to do with the store. Port
        // zero is the wildcard: the kernel refuses a connection to it outright,
        // every time, with nothing to race against.
        let error = the_log_store_is_serving("0").expect_err("a closed port is not serving");
        assert!(error.contains("not accepting connections"), "{error}");
    }

    #[test]
    fn a_store_that_answers_is_a_pass() {
        // Or the assertions above would hold against a check that refused
        // everything.
        let serving = answering("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(the_log_store_is_serving(&serving), Ok(()));
    }

    #[test]
    fn a_body_that_begins_with_a_number_is_not_a_status() {
        // The same shape `nc-log`'s shipper checks: the protocol name at the
        // start and a three-digit 2xx after it, so a reply whose *body* opens
        // `200` is not read as one — and a 2000-something code is not a 2xx.
        for pretending in [
            "200 OK\r\n\r\n",
            "HTTP/1.1 2000 Invented\r\n\r\n",
            "HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\n\r\n",
        ] {
            let port = answering(pretending);
            assert!(
                the_log_store_is_serving(&port).is_err(),
                "`{pretending}` was read as serving"
            );
        }
    }

    /// Every method called in `text`, by name.
    ///
    /// The name whole, between the dot and its bracket, so a rule about what a
    /// method is called cannot be satisfied by a longer name that contains it.
    fn the_methods_in(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        for (at, _) in text.match_indices('.') {
            let rest = &text[at + 1..];
            let name: String = rest
                .chars()
                .take_while(|one| one.is_alphanumeric() || *one == '_')
                .collect();
            if !name.is_empty() && rest[name.len()..].starts_with('(') {
                found.push(name);
            }
        }
        found
    }

    #[test]
    /// A count that went backwards is a failed read, not a negative number.
    ///
    /// **Both halves of this are held.** Read with `unwrap_or(0)` — the shape
    /// `migrate` refuses for the two reads before it, *a permission error, a
    /// wrong path or a dropped connection all read as "nothing has been
    /// applied"* — and differenced with `saturating_sub`, which on `i64`
    /// saturates at `i64::MIN` rather than at zero, the count goes wrong
    /// silently. Measured with the pool closed before the read: `ok -12
    /// migration(s) applied, and the database now holds 0 of 12 on disk`,
    /// `check passed`, byte-identical to a clean run.
    fn a_count_that_went_backwards_is_a_failed_read_rather_than_a_negative_number() {
        // What a read of the applied count may call. An item, so it is written
        // where items go.
        const A_READ_MAY_CALL: [&str; 6] =
            ["fetch_one", "await", "map_err", "ok", "format", "bind"];

        assert_eq!(super::what_this_run_applied(12, 12), Ok(0));
        assert_eq!(super::what_this_run_applied(0, 12), Ok(12));
        assert_eq!(super::what_this_run_applied(12, 13), Ok(1));
        let backwards = super::what_this_run_applied(12, 0)
            .expect_err("a database cannot hold fewer migrations after applying some");
        assert!(
            backwards.contains("went backwards") && backwards.contains("search_path"),
            "the failure does not say what to look at: {backwards}"
        );
        // And the read itself, which no fixture here can make fail: the two
        // neighbours in `migrate` answer a failed read with `map_err(…)?`, and
        // this asks that the third does too. A source assertion, for the reason
        // `ship.rs` records about its own — the failure needs a database that
        // stops answering mid-function, and the arm is one line.
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is a file in this crate");
        let code = crate::gates::without_comments(&source, "//");
        // **Every count read in `migrate`, at any indentation.** A scan for one
        // byte sequence — the read, its two continuations at twelve spaces, and
        // `.unwrap_or(` — misses reads written at another indentation, and
        // `migrate` holds its `count(*) FROM _sqlx_migrations` reads at two:
        // a `.unwrap_or(already)` with a `saturating_sub` under it on the other
        // one passes such a scan. Written as a property, the rule reads every
        // one of them.
        let reads = code.match_indices("FROM _sqlx_migrations\")").count();
        assert!(
            reads >= 2,
            "this scan found {reads} count read(s) in the file and there are more than \
             one, so it is looking for the wrong text"
        );
        // **To the end of the function, not to the next semicolon.** A scan of
        // each arm up to its `;` lets a fallback move past that with one more
        // binding: measured,
        // `let counted = …map_err(…); let now = counted.unwrap_or(0);` left
        // `check passed` and a transcript differing only in the source-line
        // counts the extra line moves — the message string does not move, so
        // the sentence census says nothing.
        //
        // `migrate` legitimately falls back nowhere, so the question is asked
        // of the whole body rather than of one statement. Every `unwrap_or` in
        // it today is inside a comment, and comments are spent above.
        let opens = "fn migrate(";
        let body_at = code.find(opens).expect("this file declares `migrate`");
        let body = &code[body_at..];
        let body_end = body.find("\n}\n").unwrap_or(body.len());
        let body = &body[..body_end];
        // **The shape of the name, not a list of them.** A list of
        // `.unwrap_or`, `.or(`, `.or_else(` and `.unwrap_or_default`, under a
        // sentence about every spelling a fallback wears, leaves the body half
        // one binding away from free — which is why the arm rule beside it is
        // an allow-list. Measured:
        // `.map_or(0, |counted: i64| counted.max(0))` written one line below
        // the read passes all four, leaves clippy silent, and turns a failed
        // read of the applied count into zero with `check passed`.
        //
        // A fallback substitutes a value for a failure, and every such method
        // in this language says so in its name: it begins `unwrap_or`, ends
        // `_or`, holds `map_or`, or is `or` itself. What that must not refuse
        // is a method that merely contains the letters — `order`, `ordering`,
        // `for_each` — so the name is read whole between the dot and the
        // bracket.
        for called in the_methods_in(body) {
            // **`or` as a word of the name, not four ways of spelling one.**
            // `starts_with("unwrap_or") || ends_with("_or") ||
            // contains("map_or") || == "or"` is not satisfied by `or_else`,
            // which a hand-written deny-list names. Measured with that rule:
            // `.or_else(|error| Ok(…))` one binding below the read left
            // `check passed` and a transcript differing only in the source-line
            // counters the extra line moves. A rule that drops a spelling a
            // hand list catches is the *too weak* half of the rule this file's
            // own paragraph warns about.
            //
            // What it must not refuse is a method whose name merely holds the
            // letters: `order`, `ordering` and `for_each` are words, and `or`
            // is a word only when the underscores say so.
            let fallback = called.split('_').any(|word| word == "or");
            assert!(
                !fallback,
                "`migrate` falls back with `.{called}(…)` somewhere in its body, and every \
                 number it reports is a count of applied migrations: a read that failed \
                 must stop the run or answer *unread*, never a number this run can act on"
            );
        }
        for (at, _) in code.match_indices("FROM _sqlx_migrations\")") {
            let arms = &code[at..];
            let ends = arms.find(';').unwrap_or(arms.len());
            let arm = &arms[..ends];
            // **Every spelling a fallback wears, not one of them.** A rule
            // asking for `.unwrap_or` misses `.ok().or(Some(0))`, the same
            // sentence in different words — measured, it passes. What a read of
            // this count may end in is `map_err(…)?`, which stops the run, or
            // `.ok()`, which makes *unread* an answer of its own; anything that
            // substitutes a number is a claim about the database that the read
            // did not establish.
            // **What it may end in, rather than what it may not.** Spellings
            // written out under a sentence promising *every* spelling a
            // fallback wears always leave one more:
            // `.map_or(0, |counted: i64| counted.max(0))` is one, it leaves
            // clippy silent, and under it a failed read of the applied count
            // becomes zero with `check passed`. A list of the ways to be wrong
            // grows with the language; the list of ways to be right does not.
            //
            // A read of this count may end in `map_err(…)?`, which stops the
            // run, or `.ok()`, which makes *unread* an answer of its own. So
            // every method called on the arm is looked up, and one this list
            // does not name is a finding whatever it is called — including a
            // combinator nobody has written yet.
            for called in arm.split('.').skip(1) {
                let Some(name) = called.split(['(', ')', ' ', ',', ';', '\n']).next() else {
                    continue;
                };
                if name.is_empty() || !arm.contains(&format!(".{name}(")) && name != "await" {
                    continue;
                }
                assert!(
                    A_READ_MAY_CALL.contains(&name),
                    "a count of the applied migrations is read through `.{name}(…)`, which \
                     `A_READ_MAY_CALL` does not name. A read that failed must stop the run \
                     or answer *unread*, never a number this run can act on — if `{name}` \
                     does one of those two, add it to the list and say so: {arm}"
                );
            }
        }
    }

    #[test]
    fn every_setting_is_read_through_the_table_that_declares_it() {
        // The names and the defaults are held to `docker-compose.yml` in both
        // directions by `the_settings_this_tool_reads_are_the_ones_the_stack_
        // declares`, and **this asks whether this file reads them**: a `run`
        // reading `POSTGRES_PORT` with its own `std::env::var` and its own
        // inline `"5432"` — the port the migrations land in and the port the
        // success line prints — leaves that census green. Measured with that literal written
        // `"5433"`: `check passed`, byte-identical to a clean run, while
        // `cargo xtask dev` brought a stack up on one port and migrated another.
        //
        // A literal name inside `std::env::var` is what a bypass looks like;
        // `setting` itself reads the variable it is handed, so it holds none.
        //
        // **Every Rust file of every member, not this one.** `database.rs::
        // default_url` and `check.rs`'s reader of the log store's bounds read the
        // stack's settings and its ports too, and a bypass in either passes in
        // every shell that exports what `dev` prints. Measured: both ports read
        // with their own `env::var` beside a fixed fallback, and the modules that
        // judge the three files passed 448 tests of 448. Those two readers are
        // held to `port` by their own modules' tests; this refuses a read by
        // name anywhere in the tree, a reader not written yet included.
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let named: Vec<&str> = super::THE_STACK_SETTINGS
            .iter()
            .map(|(variable, _)| *variable)
            .chain(super::Port::ALL.map(super::Port::variable))
            .collect();
        let mut read: Vec<String> = Vec::new();
        let mut bypasses: Vec<(String, String)> = Vec::new();
        let mut this_file = String::new();
        crate::gates::walk(root, &mut |path| {
            if path.extension().is_none_or(|kind| kind != "rs") {
                return;
            }
            let relative = crate::relative_to(root, path);
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                // A file another test removed between the listing and the
                // read is not source.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) => panic!("`{relative}` is not readable: {error}"),
            };
            // **Comments only.** Spending the string contents too would remove a
            // variable's name, which lives *inside* the string it is read with —
            // the very text the scan is looking for — and the scan then passes
            // against the planted bypass, measured. A scanner that reads a
            // literal as code is a familiar shape; this is its mirror, and the
            // one that stays green. Whitespace goes, which no name holds.
            let code: String = crate::gates::without_comments(&text, "//")
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect();
            for variable in &named {
                for bypass in [
                    format!("var(\"{variable}\")"),
                    format!("var_os(\"{variable}\")"),
                    format!("env!(\"{variable}\")"),
                    format!("setting((\"{variable}\""),
                ] {
                    if code.contains(&bypass) {
                        bypasses.push((relative.clone(), bypass));
                    }
                }
            }
            if relative == "xtask/src/dev.rs" {
                this_file = code;
            }
            read.push(relative);
        })
        .expect("the tree is readable");
        // **And the ports, which no default stands beside.** A port read by its
        // name falls back to whatever its reader writes there — a fixed port,
        // which is two checkouts' stacks on one — rather than to the checkout's
        // pair `port` derives.
        assert!(
            bypasses.is_empty(),
            "{bypasses:?} (a file, and what it reads) read a variable of the stack by name \
             rather than through `setting` with the row the table declares, or `port` for \
             either port. A setting read that way falls back to a copy of its default that \
             nothing holds to the stack, and a port to whatever its reader writes there \
             rather than this checkout's port"
        );
        // The floor: a scan that spent the whole file would report no bypass
        // whatever this file said.
        assert!(
            this_file.contains("fnsetting("),
            "the scan read no code of this file, so it agrees with any file"
        );
        // **Every member tree, not the files that read the stack today.** A walk
        // that stopped at `xtask/` would still read all three.
        let members = crate::members::members(root).expect("cargo lists this workspace");
        let unread: Vec<&str> = members
            .iter()
            .map(|member| member.path.as_str())
            .filter(|tree| {
                !read
                    .iter()
                    .any(|file| file.starts_with(&format!("{tree}/")))
            })
            .collect();
        assert!(
            unread.is_empty() && members.len() > 10,
            "{} member(s) cargo resolves, and no Rust file was read under {unread:?}, so a \
             variable of the stack read by name there is one this scan never sees",
            members.len()
        );
    }

    /// The migrated schema is the one the URL names, and it comes first.
    ///
    /// **No test runs `migrate`, and this is the line that matters.**
    /// A `"$user",` in front of the schema is one token, `cargo fmt` and
    /// `cargo clippy` are silent about it, and with a role-named schema present
    /// `dev` then migrates *that* one — a second `_sqlx_migrations` with twelve
    /// rows, a second `journal_entries` with its append-only triggers — while
    /// reporting it applied nothing, and `cargo xtask check` passes over the
    /// result, measured.
    ///
    /// `check` resolves the same schema from the same string, so a comparison
    /// of the two cannot tell them apart: they agree by being right together or
    /// wrong together, which is why this test holds the statement itself.
    #[test]
    fn the_search_path_puts_the_named_schema_first() {
        assert_eq!(
            super::path_statement("public"),
            "SET search_path = \"public\", pg_temp",
            "the schema the URL names has to be first: anything before it — `$user` \
             above all — is a schema this run may migrate and `check` never inspects"
        );
        // A schema whose name needs quoting is still one element, not two.
        assert_eq!(
            super::path_statement("two words"),
            "SET search_path = \"two words\", pg_temp"
        );
        // And the ordinary role-named case, which is the one this test's
        // paragraph measures.
        let said = super::path_statement("noctua");
        assert!(
            said.starts_with("SET search_path = \"noctua\","),
            "nothing precedes the named schema: {said}"
        );
        assert!(
            said.ends_with(", pg_temp"),
            "a temporary table can shadow a real one: {said}"
        );
        // **And the catalogue is not named**, which is the property rather than
        // the string: PostgreSQL puts `pg_catalog` implicitly first *unless the
        // path names it*, so naming it moves the system catalogue behind a
        // schema this run may migrate. `nc-journal` pins its own path the same
        // way.
        for schema in ["public", "noctua", "two words"] {
            assert!(
                !super::path_statement(schema).contains("pg_catalog"),
                "`pg_catalog` is named, which puts a writable schema in front of the \
                 system catalogue — and the forward-only guard reads `pg_class`"
            );
        }
    }

    /// A status line split across segments is still a status line.
    ///
    /// **`read` returns what has arrived.** One call into a 64-byte buffer reads
    /// a store answering `HTTP/1.1 ` in one segment and `200 OK` in the next as
    /// `HTTP/1.1 ` — no code — and `dev` then reports a healthy store as
    /// "running and not serving". Nothing is wrong
    /// with the store in that case, which is what makes it expensive: the
    /// failure is intermittent and points at the wrong thing.
    ///
    /// Both halves are driven here, against a listener that writes the answer
    /// the two ways, because a reader that only ever sees whole lines cannot
    /// tell a reader that loops from one that does not.
    #[test]
    fn a_status_line_split_across_segments_is_read_whole() {
        use std::io::{Read as _, Write as _};

        for split in [false, true] {
            let listener =
                std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
            let port = listener.local_addr().expect("the port is known").port();
            let serving = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().expect("the check connects");
                let mut request = [0_u8; 256];
                let _ = socket.read(&mut request);
                if split {
                    socket.write_all(b"HTTP/1.1 ").expect("the first segment");
                    socket.flush().expect("flushed");
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    socket.write_all(b"200 OK\r\n\r\nok").expect("the rest");
                } else {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\n\r\nok")
                        .expect("one segment");
                }
            });
            let answered = super::the_log_store_is_serving(&port.to_string());
            serving.join().expect("the listener finishes");
            assert!(
                answered.is_ok(),
                "a store answering 200 was read as not serving, with the status line \
                 {} across segments: {answered:?}",
                if split { "split" } else { "whole" }
            );
        }
    }

    /// The stack's settings are the compose file's, in both directions.
    ///
    /// **Copies of the same values, held to each other.** `.env.example` and
    /// `docker-compose.yml` are held together by
    /// `the_example_environment_agrees_with_the_compose_defaults`; this holds the
    /// code that reads the same settings to both. Measured without it,
    /// `POSTGRES_DB`'s default changed in this file alone: `check passed` with
    /// no warn line, and `cargo xtask dev` failed with
    /// `database "central" does not exist`.
    ///
    /// **Both directions, because one is a sample.** Forwards: every setting
    /// this file declares is declared in the compose file with the same default.
    /// Backwards: every `${NAME:-value}` the compose file substitutes is one of
    /// them — a fourth added to the stack and not added here is a setting the
    /// tools do not read, which is the same stack with two configurations.
    ///
    /// **The ports, which have no default to agree on.** Each is the
    /// checkout's ([`super::Port`]), so the compose file writes it
    /// `${NAME:?…}` — required, with nothing to fall back to — and never
    /// `${NAME:-…}`, whose default is a port two checkouts' stacks would share;
    /// and its message names `cargo xtask dev`, because the developer who meets
    /// it ran compose by hand. Any other spelling of a variable is one neither
    /// list reads, and refused as well.
    #[test]
    fn the_settings_this_tool_reads_are_the_ones_the_stack_declares() {
        let compose = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docker-compose.yml"
        ))
        .expect("this repository has a compose file");
        // **What compose reads, and not its comments**, which compose never
        // substitutes and this file's prose spells variables in.
        let read: String = compose
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        // `${NAME:-value}` and `${NAME:?why}`, read as the compose file
        // substitutes them.
        let mut defaulted: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        let mut required: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        let mut otherwise: Vec<String> = Vec::new();
        let mut rest = read.as_str();
        while let Some(at) = rest.find("${") {
            rest = &rest[at + 2..];
            let Some(close) = rest.find('}') else { break };
            let (inside, after) = rest.split_at(close);
            rest = &after[1..];
            if let Some((name, default)) = inside.split_once(":-") {
                defaulted.insert(name.to_owned(), default.to_owned());
            } else if let Some((name, why)) = inside.split_once(":?") {
                required.insert(name.to_owned(), why.to_owned());
            } else {
                otherwise.push(inside.to_owned());
            }
        }
        assert!(
            otherwise.is_empty(),
            "`docker-compose.yml` substitutes {otherwise:?} with neither a default nor a \
             requirement, so what it reads unset is the empty text"
        );
        assert!(
            defaulted.len() + required.len()
                >= super::THE_STACK_SETTINGS.len() + super::Port::ALL.len(),
            "the compose file substitutes {} variable(s), which is fewer than this file \
             declares — the scan above read the wrong text: {defaulted:?} {required:?}",
            defaulted.len() + required.len()
        );
        for (variable, default) in super::THE_STACK_SETTINGS {
            assert_eq!(
                defaulted.get(variable).map(String::as_str),
                Some(default),
                "`{variable}` is `{default}` here and `{:?}` in `docker-compose.yml`, so \
                 `cargo xtask dev` brings up one stack and the tools talk to another",
                defaulted.get(variable)
            );
        }
        for which in super::Port::ALL {
            let variable = which.variable();
            let why = required.get(variable);
            assert!(
                why.is_some_and(|why| why.contains("cargo xtask dev")),
                "`docker-compose.yml` reads `{variable}` as {:?} and not as required with a \
                 message naming `cargo xtask dev`, so a stack not handed this checkout's \
                 port comes up on whatever its default is — two checkouts on one port",
                defaulted.get(variable).or(why)
            );
        }
        let unread: Vec<&String> = defaulted
            .keys()
            .filter(|name| {
                !super::THE_STACK_SETTINGS
                    .iter()
                    .any(|(variable, _)| *variable == name.as_str())
            })
            .chain(required.keys().filter(|name| {
                !super::Port::ALL
                    .iter()
                    .any(|which| which.variable() == name.as_str())
            }))
            .collect();
        assert!(
            unread.is_empty(),
            "`docker-compose.yml` reads {unread:?}, which this file does not declare — a \
             setting the stack takes and the tools do not is how the two come apart"
        );
    }

    /// The example environment declares what this tool reads, in both
    /// directions.
    ///
    /// **Every setting with its default, every port empty, and nothing else.**
    /// A port written there would be one checkout's, and a developer who
    /// sources it in a second checkout binds that one's stack on the first's
    /// port — the collision the ports are derived to avoid — and so would the
    /// `DATABASE_URL` it once wrote out, which carries a port. `cargo xtask dev`
    /// prints both instead.
    /// Backwards, a variable declared there that this file does not read is one
    /// a developer sets and watches do nothing; forwards, one this file reads
    /// and the example leaves out is one nobody is told about.
    #[test]
    fn the_example_environment_declares_what_this_tool_reads() {
        let example =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env.example"))
                .expect("this repository has an example environment");
        let mut written: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for line in example.lines().map(str::trim) {
            if line.starts_with('#') {
                continue;
            }
            if let Some((name, value)) = line.split_once('=') {
                written.insert(name.trim().to_owned(), value.trim().to_owned());
            }
        }
        let mut expected: std::collections::BTreeMap<String, String> = super::THE_STACK_SETTINGS
            .iter()
            .map(|(variable, default)| ((*variable).to_owned(), (*default).to_owned()))
            .collect();
        for which in super::Port::ALL {
            expected.insert(which.variable().to_owned(), String::new());
        }
        assert_eq!(
            written, expected,
            "`.env.example` declares what is on the left and this file reads what is on the \
             right: every setting with the default compose falls back to, every port empty \
             because it is the checkout's, and nothing else — a `DATABASE_URL` written out \
             carries one checkout's port"
        );
    }

    #[test]
    /// A migration numbered below one already applied is refused, and nothing
    /// else is.
    ///
    /// **The whole forward-only rule, decided here.** See
    /// [`super::is_backdated`] for what a neutered guard measures with the clause
    /// written inline in `migrate`, where no test reaches it: the workspace
    /// suite green at exit 0, and `cargo xtask dev` applying version 0 after
    /// version 12 while printing `ok`. `_sqlx_migrations` records the order it
    /// was told, so a database that has taken a backdated migration reads as
    /// though it had always had it — the shape acceptance criterion 10 is about.
    ///
    /// Both directions on each clause: the `<` and the `contains`.
    fn a_migration_numbered_below_an_applied_one_is_refused() {
        let applied = [1_i64, 2, 12];
        let highest = 12_i64;
        for (version, backdated, why) in [
            (0_i64, true, "new and below the highest applied"),
            (
                11,
                true,
                "new and below the highest applied, between two of them",
            ),
            (
                1,
                false,
                "already applied, so running it again is not out of order",
            ),
            (2, false, "already applied"),
            (12, false, "the highest applied itself"),
            (
                13,
                false,
                "new and above everything applied, which is the ordinary case",
            ),
        ] {
            assert_eq!(
                super::is_backdated(version, highest, &applied),
                backdated,
                "migration {version} is {why}, and the guard says otherwise. A false \
                 negative applies it out of order and records it as though it had \
                 always been there; a false positive refuses a stack that is correct"
            );
        }
        // The floor: a rule that answered the same thing to everything would
        // satisfy any one of the cases above on its own.
        assert!(
            [0_i64, 11]
                .iter()
                .all(|v| super::is_backdated(*v, highest, &applied))
                && [1_i64, 12, 13]
                    .iter()
                    .all(|v| !super::is_backdated(*v, highest, &applied)),
            "the guard does not separate the two groups at all"
        );
    }
}
