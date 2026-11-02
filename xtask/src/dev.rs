//! Brings the local development stack up, or takes it down.
//!
//! One command to a working PostgreSQL and a working central log store —
//! Stage 10's local development stack, and the tenth of its acceptance criteria.
//!
//! **Not *from a clean machine*, which this said.** That is the criterion's own
//! wording and it is the half that has never been watched: what was seen on
//! 2026-08-27 was `dev --down` → `dev` → `check` on a machine that already held
//! both images, and `down -v` takes volumes rather than images. `NOCTUA-PLAN.md`
//! records it as unmet for that reason, and the test module below says the same.
//! This header is the half rustdoc publishes, which is why it is written here;
//! the same split put a retracted claim back into the published documentation of
//! `gates.rs`, one file over.
//!
//! **And it was not the only place in the file, which this claimed.** A comment
//! in [`run`] said `dev --down` is what a developer runs *to get back to a clean
//! machine* — the second sense of the same two words, four hundred lines below a
//! paragraph asserting there was no second. Found on F5's seventh pass, and the
//! sentence naming it was wrong in its own turn on the eighth: it sent a reader
//! to `compose_command`, which is four hundred lines below the comment rather
//! than its home and mentions `--down` nowhere. What `--down` empties is the
//! database; the image store is untouched, which is the whole reason the
//! criterion's own wording is unmet.
//!
//! **It passes no `.env`.** Docker Compose would read one from beside the
//! compose file, which is inside the repository, and `cargo xtask check` fails
//! on a `.env` at any depth (`NOCTUA.md §15.5`). Every variable the compose file
//! reads has a default, so the stack comes up with none set; a developer who
//! wants different values exports them, and `.env.example` says how.

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
/// **These were written out four times and held to nothing.** `dev.rs` read the
/// five variables with their defaults inline, `database.rs::default_url` read
/// four of them again — its own paragraph asserting *"the same defaults"* — and
/// `check.rs` read `LOGS_PORT` a third time, while `docker-compose.yml` declares
/// each as `${NAME:-value}`. Measured on 2026-09-09, `POSTGRES_DB`'s default in
/// this file alone changed to `"central"`: `check passed` at `1232 test(s)` with
/// no warn line, and `cargo xtask dev` then failed outright —
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
/// Where PostgreSQL listens, on loopback.
pub(crate) const POSTGRES_PORT: Setting = ("POSTGRES_PORT", "5432");
/// Where the log store listens, on loopback.
pub(crate) const LOGS_PORT: Setting = ("LOGS_PORT", "9428");

/// Every one of them, so a test can read the set rather than five names.
///
/// A reverse loop over `docker-compose.yml` is what makes this a set rather than
/// a sample: a sixth `${NAME:-value}` added to the stack and not added here is a
/// setting the tools do not read, which is the same stack-with-two-configurations
/// the paragraph above measures.
#[cfg(test)]
pub(crate) const THE_STACK_SETTINGS: [Setting; 5] = [
    POSTGRES_USER,
    POSTGRES_PASSWORD,
    POSTGRES_DB,
    POSTGRES_PORT,
    LOGS_PORT,
];

/// What the environment says, or what the stack is without it.
pub(crate) fn setting((variable, default): Setting) -> String {
    std::env::var(variable).unwrap_or_else(|_| default.to_owned())
}

/// The url of the stack `docker-compose.yml` brings up.
///
/// **Read from the same four variables compose reads, with the same defaults.**
/// The previous fix stopped `migrate` reading `DATABASE_URL` — which was right,
/// since that pointed anywhere — and hardcoded the credentials instead, so three
/// of the four variables `.env.example` documents broke `dev` outright:
/// `POSTGRES_PASSWORD=hunter2` gave *password authentication failed*, and
/// `POSTGRES_DB=central` gave *database "noctua" does not exist*. A stack whose
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
/// **Split out because the whole repair is in this one string and nothing read
/// it.** `migrate` has no test — `dev.rs`'s module covers the health table,
/// `--down`, the project name and the log store — so a single token added here
/// put the schema second:
///
/// ```text
/// SET search_path = "$user", "noctua", pg_catalog, pg_temp
/// ```
///
/// With a role-named schema present — the `CREATE SCHEMA … AUTHORIZATION`
/// arrangement the twelfth migration calls ordinary — `dev` then built a second
/// complete journal there, append-only triggers and all, reported
/// `12 migration(s) were already applied, and this run applied none`, and the
/// whole gate passed over it. `check` pins to the schema the URL names, so it
/// can never disagree: the two agree by both being right, and nothing notices
/// when one stops.
///
/// The schema is first among the schemas a name may resolve in, and `pg_temp`
/// is last so a temporary table cannot shadow a real one.
///
/// **`pg_catalog` is not named, and naming it was the defect.** PostgreSQL puts
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
/// `nc-journal`'s pinned path is `<schema>, pg_temp` for this reason and this
/// file's paragraph above claimed the opposite in as many words. It needs
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
/// **`saturating_sub` on a signed count saturates at `i64::MIN`.** The
/// difference was taken inline, and a `now` smaller than `already` — which only
/// a wrong read produces, and did — printed
/// `ok -12 migration(s) applied, and the database now holds 0 of 12 on disk`
/// under a green run. A count that went backwards is not a negative number of
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
/// Split out because `migrate` crossed clippy's line limit when it learned to
/// do this. The path is set once for the whole pool, so the forward-only guard's
/// read, the count beside it and `Migrator::run` all resolve
/// `_sqlx_migrations` in the same place — which is the defect this closed.
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
/// **The guard's *argument* was outside every mechanism that covers its
/// predicate.** [`is_backdated`] is decided from two directions — its own unit
/// test and `corpus::the_forward_only_predicate_decides_every_clause` — and both
/// hand it `highest` written out as `12`, so the expression that computes it was
/// read by nothing. Measured on 2026-09-09, `applied.last()` replaced by
/// `applied.first()`: `cargo run -p xtask -- check` gave `check passed` at
/// `1232 test(s)` with no warn line, while against a database holding versions
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
/// **A clause no test in this workspace decided.** It was written inline in
/// [`migrate`], whose own neighbour records that `migrate` has no test, and
/// `dev` was not then one of the files `xtask/mutants.py` mutates — it is one
/// of the six now, which is what closed this — so nothing
/// could reach it from either direction. Measured, the condition replaced by
/// `false` and `cargo test --workspace --locked` run whole: **exit 0**, all 34
/// targets `ok`. With the same neutering and
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
/// **Forward-only is not one of the migrator's rules**, though this said it was.
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
    // **The stack's own database, and not `DATABASE_URL`.** This read the
    // environment first, so a developer with `DATABASE_URL` pointing anywhere
    // else — which `.env.example` tells them to export — got a run that migrated
    // *that* database and then printed the compose url as though it were ready.
    // "One command to a working platform" produced a stack whose advertised
    // database had no schema, and said nothing.
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
        // different rule, and `dev.rs` used to claim both were its.
        // **The schema the connection string names, not whichever one the
        // search path reaches first.** This read `_sqlx_migrations` unqualified,
        // and `database::applied` — the *reader* twenty lines of prose away —
        // carries the whole paragraph explaining why that is wrong, with the
        // measurement `app` recording six and `public` twelve. The fix was
        // applied to the reader and not to the writer beside it.
        //
        // Measured: with a second `_sqlx_migrations` in a role-named schema
        // ahead of `public` on the path, a migration numbered *below* the
        // twelve already applied ran, was recorded first, and `dev` printed
        // `13 migration(s) applied, and the database now holds 13 of 13 on
        // disk` — while `check`, which *is* pinned, read `public` and printed
        // `12 migration(s), numbered once each and in order`. Two parallel
        // schemas, and the guard's own sentence about applying out of order
        // "as though it had always been there" describes exactly what happened.
        let schema = pin_the_path(&pool, &url).await?;
        // **A read that failed is not a database with nothing applied.** This
        // was `unwrap_or_default()`, so a permission error, a wrong path or a
        // dropped connection all read as "nothing has been applied" and turned
        // the forward-only guard off. Only a table that is genuinely absent is
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

        // **What was already there, so the line can say what this run did.** The
        // number printed was `migrator.iter().count()` — the files on disk — so
        // a second `dev` against an already-migrated database reported
        // `12 migration(s) applied` having applied none. That is `§19.3`'s
        // shape: a step reporting on something it did not do, and the sentence
        // a reader uses to conclude the stack is at the right version.
        // Resolved through the pinned path above, and a failed read is a
        // failure rather than a zero — the same two repairs as the guard's own
        // read, which this line sat beside and did not share.
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
        // before it committed and recorded — permanently, in a repository with
        // no git. This said only which migration failed. Measured, a valid `0013`
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
        // it is past this point. The success path two screens down was rewritten
        // to report exactly what *this run* did; the failure path reported
        // nothing at all.
        if let Err(error) = migrator.run(&pool).await {
            // **No fallback at all, because every one of them is a claim.**
            // This read `unwrap_or(already)` and took its difference with
            // `saturating_sub` — the pair the repair four lines down removed
            // from its sibling, left standing here — so a read that failed
            // reported *"None landed"*, which is a statement about the database
            // this path cannot make. A count that could not be read is its own
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
        // **A read that failed is not a database holding nothing.** This was
        // `unwrap_or(0)`, the shape repaired ninety lines above for `present`
        // and `already` — *a permission error, a wrong path or a dropped
        // connection all read as "nothing has been applied"* — and the read that
        // decides what this run **reports** was left out of that repair. It is
        // also the one number a developer reads to know the stack is ready.
        let now: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .map_err(|error| format!("the migrations ran and could not be counted: {error}"))?;
        pool.close().await;
        // **And `saturating_sub` on a signed count saturates at `i64::MIN`, not
        // at zero.** With `now` smaller than `already` — which only a wrong read
        // can produce, and did — the arm below printed
        // `ok -12 migration(s) applied, and the database now holds 0 of 12 on
        // disk` under a green run.
        let applied = what_this_run_applied(already, now)?;
        if applied == 0 {
            ui::ok(&format!(
                // `now`, the rows, not `count`, the files on disk — the
                // substitution this function's own paragraph records as the
                // defect it was fixed for, left standing on the other arm.
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

/// Brings the stack up, or takes it down with its volumes.
///
/// # Errors
///
/// Returns a message naming what to do when Docker is absent, when the compose
/// file is missing, or when a service does not become healthy.
pub(crate) fn run(root: &Path, down: bool) -> Result<(), String> {
    ui::heading("dev");
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
        // **Not a clean machine, which this said.** `down -v` takes containers,
        // networks and volumes and leaves the image store alone, so the tree a
        // `--down` leaves behind still holds both pinned images. The header of
        // this file records that confusion costing two rounds and claims to be
        // the one place in the file that asserted it — and this line was a
        // second, four hundred lines below it. Two senses of one word, found by
        // a reviewer on F5's seventh pass reading the header and then the
        // code.
        // **And it says whether there was anything to take down.** It reported
        // `the stack is down and its volumes are gone` over compose's own
        // `Warning: No resource found to remove for project "…"` — the same
        // shape `migrate` was fixed for one function below, where a second run
        // reported `12 migration(s) applied` having applied none. It matters
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
    compose_command(root, &["up", "-d", "--remove-orphans"])?;
    ui::ok("the stack is starting");
    wait_for_health(root)?;

    let logs_port = setting(LOGS_PORT);
    // **Through the table, like its neighbour.** This read the variable itself
    // with an inline `"5432"`, which is a fifth copy of a default the stack
    // declares once — and it is the port `migrate` connects to *and* the port
    // the success line prints. `database.rs::default_url` was repaired to read
    // all four through `setting` and carries the paragraph saying why; this
    // reader, the one that decides where the migrations land, was left out.
    // Measured with the literal written `"5433"`: `check passed`, byte-identical
    // to a clean run, while `cargo xtask dev` brought the stack up on one port
    // and tried to migrate another.
    let postgres_port = setting(POSTGRES_PORT);
    the_log_store_is_serving(&logs_port)?;
    migrate(root, &postgres_port)?;
    ui::gap();
    ui::ok("the stack is up");
    ui::detail(&format!("postgres    {}", stack_url(&postgres_port)));
    ui::detail(&format!(
        "log store   http://127.0.0.1:{logs_port}/select/vmui/  (interactive, live tailing)"
    ));
    ui::detail("both bind to loopback only; `cargo xtask dev --down` removes them and their data");
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

/// The log store answers `GET /health`, asked from outside the container.
///
/// **The container's own healthcheck cannot fail.** The image has no shell, no
/// `curl` and no `wget`, so the only thing a compose `CMD` can run is the binary
/// — and `--help` parses arguments and exits 0 whether or not the HTTP server
/// is serving. Measured: it exits 0 and the container reports `healthy` with the
/// server never asked anything. `dev` then printed the store's URL, which is a
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
    // **One statement of the rule, in `store.rs`.** This was a fourth copy, and
    // the copies had drifted: only `nc-log`'s required the three characters to
    // be digits, so `HTTP/1.1 2ab Invented` was a refusal there and
    // `the log store answers /health` here.
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
/// **Acceptance criterion 10's one real decision, and nothing entered it.** The
/// eleven `dev::` tests cover the log store's status line, the project name,
/// `--down`, the search path and the forward-only rule; the loop that decides
/// *whether the stack is up* was reached by none of them. Measured on
/// 2026-09-09, `health != "healthy"` replaced by `health == "unhealthy"` so that
/// a container reporting `starting` counts as healthy: `check passed` at
/// `1232 test(s)` with no warn line, and a cold `cargo xtask dev` printed
/// `ok 2 service(s) healthy` against a PostgreSQL still initialising its data
/// directory, then failed with `Connection reset by peer`.
///
/// Every arm of that loop was in the same position, and each is a defect this
/// file already carries a paragraph about. A verdict over one line is a pure
/// function, so it is one — driven from a written table in `corpus.rs`, which is
/// the suite `xtask/mutants.py` runs for this file.
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
    /// sentences left that record, which is a verdict a reader sees and no list
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
        // **`-a`, because `docker compose ps` omits what is not running.** A
        // service that exited on start simply disappeared from this list instead
        // of appearing as unhealthy, so the loop below found nothing wrong and
        // the stack was reported healthy with a declared service dead. The
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
/// *and its volumes*. A reviewer working in a copy called `noctua-central`
/// measured that: the live stack's containers carried
/// `com.docker.compose.project=noctua-central` with a working directory in the
/// original tree.
///
/// A written-out constant is worse, and was what replaced it for one round: it
/// makes the collision unconditional, from any directory under any name. A
/// second reviewer measured *that*, from a directory sharing nothing with this
/// repository but the flag:
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
/// The two stacks still cannot run at once — both bind 127.0.0.1:5432 — and that
/// is the point: the second `dev` fails on the port, loudly, instead of quietly
/// migrating the first's database.
fn project_for(root: &Path) -> String {
    let path = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    format!("noctua-central-{:08x}", digest_of(&path.to_string_lossy()))
}

/// FNV-1a, 32 bits of it, over the checkout's path.
///
/// **Written here rather than taken from a crate**, and not because a
/// dependency would be heavy: what this number names is a Docker volume holding
/// a developer's database, so it has to mean the same thing in five years. A
/// hasher whose output is documented as unstable between releases — which is
/// every `std` one — would rename the project on a toolchain upgrade and orphan
/// the data. This is eight lines and it is frozen by
/// `the_project_name_does_not_move_under_a_checkout`.
fn digest_of(text: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// The compose invocation that was made, written so it can be run again.
///
/// **The `-p` is not optional, and three messages left it out.** Every
/// invocation here passes `["-p", &project_for(root)]`, because a bare
/// `docker compose` resolves the project from the *directory basename* — which
/// by construction is never that name. So the printed command addressed a
/// different project, and a developer who ran it saw:
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
/// two messages already carried the project, with the measurement written above
/// them; these three were the same lesson, unlearnt one function along.
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
/// lost.** The three readers below each wrote `Command::new("docker")`, the
/// `-p`, the arguments and `.current_dir(root)` out in full and differed only
/// in which stream they read. A reviewer planted three defects in round
/// seventy-one and every one left the suite green: an `--project-name`
/// appended after the `-p`, which docker resolves to the **later** flag —
/// measured, `docker compose -p a --project-name b config` answers `name: b`;
/// a `root` shadowed by `Path::new("/")`, which points every checkout on the
/// machine at one project while the printed command names another; and
/// `.current_dir(root)` deleted, after which compose walks up from wherever
/// the developer is standing and labels *that* tree's services with this
/// tree's project. What held them was a count of tokens in this file's text,
/// not the command that gets built.
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
/// file's own text, comments and all**, because that rule compared a
/// comment-stripped copy for one round while saying it did not, and a comment
/// inside the block is where the next reader is told what it holds. The lines
/// that decide what each arm does are held beside it, and so are the three
/// readers' whole bodies: the variant each one passes is the whole of what
/// tells output-through from captured, and for one round it was a token nothing
/// read.
///
/// That refuses an ordinary edit as loudly as it refuses a defect: an
/// `--ansi never`, a `--progress plain`, a `.stdin(Stdio::null())` or an
/// `env_clear` of the `COMPOSE_*` variables is a red test naming the block. It
/// is the price, and it is paid on purpose — a checkout's stack is not worth an
/// edit nobody read.
///
/// **What a stack that cannot come up says, measured rather than described.**
/// Two places in this tree pointed here for the transcript of a service aimed at
/// a reference that cannot resolve, and there was no transcript anywhere —
/// `grep` for any of docker's resolution errors returned nothing. Taken on
/// 2026-09-15 in a scratch copy with its own compose project name, the log
/// store's image replaced by `example.invalid/noctua-does-not-exist:1`:
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
/// `a_stack_that_did_not_come_up_says_what_to_look_at` anchors on, and a second
/// copy in this file is what sent that test's own predecessor to read its own
/// text instead of the shipped line. The test now refuses a second copy rather
/// than silently preferring the first.
///
/// `cargo xtask dev` exited 1. So docker's own words reach the reader ahead of
/// this tool's sentence, which is what letting the output through is for, and
/// the sentence names the right three causes with the real one first.
fn compose(root: &Path, args: &[&str], want: Compose) -> Result<String, String> {
    // **And a caller cannot name a project either.** The block below is held
    // whole, and `.args(args)` sits inside it — so the one channel the pin left
    // open was the slice itself: `compose_command(root, &["--project-name",
    // "noctua-central", "up", "-d", …])` brought the stack up under the shared
    // name with the whole suite green, measured by a reviewer in round
    // seventy-one, because the walk that reads this file discards everything
    // above the first `Command::new("docker")` and the call site is four hundred
    // lines above it. Docker takes the later of two, so `--down` would then
    // address a project that never ran.
    //
    // Refused here rather than pinned at three call sites, because this line is
    // inside the held block and a text rule over call sites is one more thing
    // that reads text.
    // **Among the top-level flags, and every spelling of them.** Docker takes
    // `-p x`, `-px`, `-p=x`, `--project-name x` and `--project-name=x` alike and
    // resolves the flag to the *later* of two — and this asked for the exact
    // token `-p` and the long form only, so `-pnoctua-central` at a call site
    // brought every checkout up under one project with the suite green.
    //
    // **And only the top-level ones, because `run`'s `-p` is `--publish`.** A
    // rule over the whole slice refuses `compose(root, &["run", "-p",
    // "8080:80", …])` with a message about project names, which is the
    // too-strong half and was shipped for one round. The distinction is
    // positional: docker's own flags come before the subcommand, so the scan
    // stops at the first argument that is not a flag. A reviewer named the
    // choice in round seventy-one and left it to the developer; this is the
    // cheaper of the two they offered, and it needs no second parameter.
    // **And a top-level flag may take a separated value.** `take_while` over
    // flags stopped at the first token that is not one — so `--ansi never -p
    // shared up` slipped past, because `never` is the *value* of `--ansi` and
    // stands before the `-p`. Measured by a reviewer in round seventy-one;
    // `--ansi`, `-f`, `--profile`, `--progress`, `--env-file`,
    // `--project-directory` and `--parallel` all do this.
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
/// where the sentence this returns now lives.
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
/// is.** The rule that compares the log store's `-storageDataPath` against its
/// volume read the mount out of `docker-compose.yml`, so a container still
/// running from before the file was edited — or one started by hand with no
/// volume at all — satisfied it: measured, the compose `logs` container stopped
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
    // **The whole body, not four lines somewhere in it.** Four rules have been
    // written about this hand-over and a reviewer walked through each: a
    // spelling, a count of `let`, a name in the callee, and then these four
    // lines held as a block — which says they are *written*, not that they are
    // all the function does. Three lines above them returning another
    // container's mount left every pin green. The body is compared whole now,
    // comments and all, so anything at all here is a failure that says to read
    // twelve lines.
    match compose_output(root, &["ps", "-q", "logs"])?.trim() {
        "" => Ok(None),
        id => where_that_container_mounts(id),
    }
}

/// Where the container with this id mounts, by asking docker about that id.
///
/// **Split from its caller so the id cannot be rebound on the way.** The gate
/// that exempts `docker inspect` from naming a project rests on the id having
/// come from a project-scoped `compose ps -q`, and that rested on a rule about
/// the text between the two — first a spelling (`let id = `), then a count of
/// `let`. A reviewer walked through each: `let id: &str = "…";` keeps the
/// spelling count, and `let mut id = …; id = "…";` keeps the `let` count. The
/// same rule then refused an ordinary hoist of the format string, which is the
/// other way to be wrong.
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
    /// A stack that did not come up says what to look at.
    ///
    /// **Acceptance criterion 10's failure half, which is the half that can be
    /// measured here.** The pass half — `docker compose down -v` then
    /// `cargo xtask dev` on a machine that has never had the images — was
    /// watched once on a machine that already had them, and a genuinely cold
    /// pull has never been seen; `NOCTUA-PLAN.md` records that and this does not
    /// close it. What this closes is the sentence a reader meets when it goes
    /// wrong, which used to be the command with the word *failed* after it.
    ///
    /// Written against the message rather than against docker, because
    /// producing the real failure means pointing a service at a reference that
    /// cannot resolve — measured, and the transcript is on [`super::compose`] —
    /// and a test that needs a daemon is a test `--offline` cannot run.
    ///
    /// **That link named `the_stack_did_not_come_up`, which is nothing.**
    /// `grep -rn` returned the one line, so the rendered documentation sent a
    /// reader to an item that does not exist while the body comment below sent
    /// them somewhere else again — the same split between a doc block and a
    /// comment that put a retracted claim back into the published docs one file
    /// over. Both named `compose_command` next, and round seventy-one moved the
    /// transcript again — onto [`super::compose`], when three builder chains
    /// were collapsed into one — so for a few hours all three references named
    /// the item that no longer held it. That is this same defect for the third
    /// time, and the third time is the one that says a rule is owed: nothing in
    /// this tree reads a rustdoc link's target for relevance. All three name
    /// `compose` now.
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
        // contains* — four lines inside the six-hundred-byte window. When the
        // live sentence moved, `find` landed on the test's own text and every
        // word was satisfied by the list that was looking for it. Measured: the
        // sentence reworded to name none of the five left `433 passed` and
        // `check passed`. `the_over_deletion_guard_runs_before_the_commit` was
        // repaired for exactly this by cutting at `mod tests`; this one was not.
        let source = include_str!("dev.rs")
            .split("mod tests")
            .next()
            .expect("this file has a test module");
        // **Exactly one**, not the first of however many. Cutting at `mod tests`
        // stopped the test reading its own literals and left the other half of
        // that defect standing: any earlier copy anywhere in the file — a doc
        // block quoting the line, say — is what `find` returns, and the window
        // then measures the copy rather than the shipped sentence. Measured
        // when a transcript was added to the runner's documentation: the
        // window landed on the quotation and the test failed against a sentence
        // that had not changed. A needle that must be unique is asserted unique.
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
    /// files allowed to hold one; a reviewer wrote a third classifier here whose
    /// protocol test was `version.len() > 5 && version.contains('/')` and whose
    /// value test was `code.as_bytes().first().copied() == Some(50)` — `50` being
    /// ASCII `2` — and every needle missed it, both scans reported the same two
    /// files, and `cargo xtask dev` printed `the log store answers /health` for
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
    /// **It said `the stack is down and its volumes are gone` either way**, over
    /// compose's own warning — the shape `migrate` was fixed for one function
    /// up, where a second run reported `12 migration(s) applied` having applied
    /// none. It matters when `--down` is run from the wrong checkout: the
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
    /// **Split out so the hole can be tested.** The loop lived inside the test
    /// and read one file, so the exemption in it — `docker inspect` is scoped
    /// by the id it is handed rather than by `-p` — could only ever be
    /// exercised by the tree's own call. That exemption asked which subcommand
    /// ran, and the property its justification rests on is about the argument:
    /// an `inspect` pointed at a container *name* reaches whatever project
    /// named it, which for a reviewer's copy is the developer's own.
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
    /// spellings kept failing to do — four consecutive rounds repaired the
    /// compose-project rule one spelling at a time.
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
            // suite — measured in round sixty-seven — because the
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
    /// **The flag, not one spelling of its argument.** The rule asked for the
    /// literal `["-p", &project_for(root)]` exactly once, so hoisting the call
    /// into a local — `let project = project_for(root);` above three
    /// invocations, which is byte-identical behaviour and which `rustfmt` and
    /// clippy are both silent about — made the gate report all three as
    /// belonging to whatever the directory is called. Measured against a live
    /// stack on 2026-09-12: same project, same containers, three false
    /// findings. That is the too-strong half, and this file already records the
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
        // begins with the pinned call and ends somewhere else: measured on
        // 2026-09-12 with the suite and clippy green, `compose up` under one
        // project while `--down -v`, `ps` and the printed command addressed
        // another.
        //
        // **Every binding of that name, not exactly one of them.** Three
        // invocations in three functions hoisting the same call is ordinary —
        // and it was refused, with the most alarming message this tool prints
        // claiming three live invocations took the project from the directory's
        // name. Measured by a reviewer on 2026-09-14, `fmt`- and clippy-clean,
        // behaviour identical. What matters is that *no* binding of that name
        // comes from anywhere else, which is what this compares.
        //
        // **And every way of writing a binding, not one of them.** The count of
        // bindings was `code.matches("let project = ")`, which a type annotation
        // walks straight past: `let project: String = "whatever".to_owned();` is
        // a binding of that name this never saw, so an invocation could take its
        // project from anywhere while the gate reported every invocation naming
        // the one this tree computes. Measured in round sixty-three with the
        // plant in `compose_command` — the function that runs `up -d
        // --remove-orphans` and `down -v` — and `check passed`, 1366 tests, fmt
        // and clippy clean. `--down -v` then removes a *fixed* project's
        // volumes, which is the accident `project_for` exists to prevent, and
        // `ran()` still prints the computed name, so the message names a project
        // that is not the one that ran. Rounds sixty and sixty-one both carried
        // this as a note nobody planted.
        //
        // `let mut` is counted as a binding — it has to be, or one pinned
        // binding beside a `let mut project = …` would balance — and never as
        // a *pinned* one, for the reason written beside the comparison below.
        // **Every place Rust introduces a name, because every one of them is a
        // binding.** The rule counted `let` and `for` heads. A *parameter* is
        // neither: measured in round sixty-five, moving `compose_command`'s
        // invocation into a closure `|project: String|` — the function that
        // runs `up -d --remove-orphans` and `down -v` — and hoisting the call
        // once elsewhere left `check passed`, 1376 test(s), with both compose
        // verbs running under the *fixed* project every checkout's directory is
        // called while `ran()` still printed the computed one. Four rounds in a
        // row this rule has been repaired one spelling at a time; what closes
        // the list is the grammar, not the next spelling.
        //
        // The binders are: a `let` head, a `for` head, a function's parameters,
        // a closure's parameters, and a `match` arm's pattern. Strings are
        // spent first, because a `=>` inside a message is not an arm.
        //
        // **And a binding is *pinned* by its value, not by its spelling.** The
        // pinned form was the literal `let <name> = <call>;`, so the same hoist
        // written `let project: String = project_for(root);` — which round
        // sixty-three taught this rule to *see* — was a binding nobody had
        // taught it to *accept*, and an ordinary line printed the most alarming
        // message this tool has. Reported in round sixty-five as the other way
        // of being wrong.
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
            // whole rule exists to prevent — counted as one binding, one
            // pinned, and the gate reported the invocation as naming what this
            // tree computes. Measured in round sixty-seven, `check passed` with
            // only four line counts moving. The doc above records the same
            // defect written as *one* statement and repaired with "the whole
            // statement, because a prefix is not a value"; two statements walk
            // past that, and so would three.
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
    /// other's containers and volumes. Measured on a reviewer's copy: the live
    /// stack's containers carried `com.docker.compose.project=noctua-central`
    /// with a working directory in the original tree.
    #[test]
    fn every_compose_invocation_names_its_project() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is readable");
        // **The test module, not the first `#[cfg(test)]`** — a file gains one
        // above its own tests sooner or later, and then the scan stops early
        // without saying so. `ui.rs` had exactly that.
        let body = source
            .split_once("\nmod tests {")
            .expect("the test module is below")
            .0;
        // **And code, not prose.** The doc on `failed` quotes the flag it is
        // about, and counting raw text counted the quotation: four `-p`s against
        // three invocations, a red test for a comment.
        // **Every comment, not the one spelling.** `line_without_a_comment`
        // reads a line at a time and knows `//`, so a `/* … */` holding
        // `.args(["-p", &project_for(root)])` beside a live invocation made the
        // chunk's flag count 2 — and one occurrence is all the rule below asks
        // for. Measured on 2026-09-11. `without_comments` is a state machine
        // over the whole file: a block comment nests, a raw string ends at its
        // own hash count, and neither opens inside a string.
        let code = crate::gates::without_comments(body, "//");
        // **Every process this file starts, and exactly two of them.** The
        // floor was `>= 2` over a file with four invocations, and it counted one
        // *spelling*: `let program = "docker";` a line above, then
        // `Command::new(program)` with its `-p` dropped, leaves three literal
        // invocations, clears a floor of two, and is invisible to the chunk walk
        // below — because the text before the first literal is what `split`
        // throws away. Measured on 2026-09-11 on the invocation that runs
        // `compose up -d --remove-orphans`: `17 passed` and `check passed`, with
        // the stack coming up under the project named after the directory while
        // `--down` still addressed the hashed one.
        //
        // So the number is the number, and every process started in this file
        // has to be the literal `docker`. A third invocation is a line to change
        // here after reading it, not a floor to clear.
        //
        // **It was four until round seventy-one.** Three of those four were one
        // builder chain written out three times, differing only in which stream
        // they read, and three defects planted in that duplication — a second
        // project flag, a shadowed `root`, a deleted `.current_dir` — each left
        // this suite green. The chain is `compose` now, and the second process
        // is the `docker inspect` the rule below exempts.
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
        // `docker compose ps -a`, and writing it twice on another, left this
        // green: measured, 514 of 514 tests passing and `check passed`, with the
        // call that decides whether the stack is up addressing the project
        // named after the directory, which for a reviewer's copy is the
        // developer's own.
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
    /// command that gets built.** Round seventy-one planted three defects in
    /// `compose` and its two predecessors, and every one left the whole suite
    /// green, clippy silent and `fmt` clean:
    ///
    /// - `.args(["--project-name", "noctua-central"])` appended after the `-p`.
    ///   The rule counts `["-p", ` and the long form shares no prefix with it,
    ///   so the count stayed one — and docker resolves the flag to its **later**
    ///   occurrence, measured: `-p a --project-name b` answers `name: b`. Every
    ///   checkout on the machine then addressed one project.
    /// - `root` shadowed by `Path::new("/")` as the function's first statement,
    ///   with the working directory left correct so the tool still worked. The
    ///   argument was still the literal `&project_for(root)`, which the rule
    ///   accepts on sight; the stack came up under the digest of `/` while the
    ///   printed command named the real one.
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
    /// file has already chosen to pay once, for the four lines that hand a
    /// container id to `docker inspect`.
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
        // **This file's own text, not a comment-stripped copy of it.** The block
        // was compared against `without_comments`, which turns every comment
        // line into a blank one that the filter below then drops — so a comment
        // inside the pinned block was free while the doc on both sides said
        // "comments and all". Planted in round seventy-one: a sentence added
        // between `command` and `.arg("compose")` left the suite green. A
        // comment inside this block is where the next reader is told what it
        // holds, which is exactly what must not change unread.
        let at = body.find(&opens).expect("the one runner is here") + opens.len();
        // **The whole body, not the lines a needle finds in it.** The pin ended
        // at the `match` and a second clause then collected the lines naming
        // `command`, `output.stdout` or `output.stderr` — a filter standing
        // between the region and the judgement, which is this tree's own
        // catalogue entry for a counter above the accounting. A reviewer wrote
        // two lines into the `Captured` arm naming none of the three —
        // `let stream = match stream { Stream::Stdout | Stream::Stderr =>
        // Stream::Stdout };` — and the suite, clippy and `fmt` were all silent
        // while `what_compose_reported` asked for stderr and got stdout, so
        // `dev --down` says the volumes are gone having removed nothing.
        //
        // There is no needle now. Everything from the signature to the closing
        // brace is compared, so the price is the messages too: rewording the
        // diagnosis is a red test quoting the body. That is the price, and it is
        // paid on purpose — the alternative was measured twice and both times a
        // defect walked through the gap the filter left.
        let ends = body
            .get(at..)
            .and_then(|rest| rest.find("\n}\n"))
            .expect("and the runner closes")
            + at;
        // **Compared trimmed, and reported one line at a time.** The `held`
        // vector is 65 lines; an `assert_eq!` over it renders two 2,500-byte
        // `Debug` dumps with the one differing element buried in the middle, and
        // a developer rewording the diagnosis meets that rather than a sentence.
        // And the wrapped literal's continuation lines sit at a column rustfmt
        // will not touch while it re-indents the code around them — so interior
        // indentation was load-bearing in the pin, and a no-op nesting change
        // would have turned it red. Neither is what this holds: it holds which
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
        // sees. `compose` refuses one at run time now, and this says the refusal
        // is never reached, because a `cargo xtask dev` that fails on its own
        // arguments is a defect that shipped rather than one that was caught.
        let elsewhere: Vec<&str> = body[..at]
            .lines()
            .chain(body[ends..].lines())
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| line.contains("\"--project-name") || line.contains("\"-p"))
            // **And the two halves of one repair agree about `run -p`.** The
            // rule above was narrowed so a published port after the subcommand
            // is legal — `run`'s `-p` is `--publish` — and this needle read that
            // same slice as a project flag, so one half refused what the other
            // blessed. A reviewer planted the legal slice in round seventy-one
            // and it went red.
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
    /// line by line, because a filter between the region and the judgement is
    /// how the two defects before this one walked through.
    #[cfg(test)]
    const THE_RUNNER_AS_WRITTEN: [&str; 109] = [
        "// **And a caller cannot name a project either.** The block below is held",
        "// whole, and `.args(args)` sits inside it — so the one channel the pin left",
        "// open was the slice itself: `compose_command(root, &[\"--project-name\",",
        "// \"noctua-central\", \"up\", \"-d\", …])` brought the stack up under the shared",
        "// name with the whole suite green, measured by a reviewer in round",
        "// seventy-one, because the walk that reads this file discards everything",
        "// above the first `Command::new(\"docker\")` and the call site is four hundred",
        "// lines above it. Docker takes the later of two, so `--down` would then",
        "// address a project that never ran.",
        "//",
        "// Refused here rather than pinned at three call sites, because this line is",
        "// inside the held block and a text rule over call sites is one more thing",
        "// that reads text.",
        "// **Among the top-level flags, and every spelling of them.** Docker takes",
        "// `-p x`, `-px`, `-p=x`, `--project-name x` and `--project-name=x` alike and",
        "// resolves the flag to the *later* of two — and this asked for the exact",
        "// token `-p` and the long form only, so `-pnoctua-central` at a call site",
        "// brought every checkout up under one project with the suite green.",
        "//",
        "// **And only the top-level ones, because `run`'s `-p` is `--publish`.** A",
        "// rule over the whole slice refuses `compose(root, &[\"run\", \"-p\",",
        "// \"8080:80\", …])` with a message about project names, which is the",
        "// too-strong half and was shipped for one round. The distinction is",
        "// positional: docker's own flags come before the subcommand, so the scan",
        "// stops at the first argument that is not a flag. A reviewer named the",
        "// choice in round seventy-one and left it to the developer; this is the",
        "// cheaper of the two they offered, and it needs no second parameter.",
        "// **And a top-level flag may take a separated value.** `take_while` over",
        "// flags stopped at the first token that is not one — so `--ansi never -p",
        "// shared up` slipped past, because `never` is the *value* of `--ansi` and",
        "// stands before the `-p`. Measured by a reviewer in round seventy-one;",
        "// `--ansi`, `-f`, `--profile`, `--progress`, `--env-file`,",
        "// `--project-directory` and `--parallel` all do this.",
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
    /// **The one channel the body pin left open.** `.args(args)` is inside the
    /// held block, and `args` is whatever a caller passes: a reviewer put
    /// `--project-name noctua-central` into `run`'s slice in round seventy-one
    /// and the stack came up under the shared name with the whole suite green,
    /// because the walk that reads this file discards everything above the first
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
        // **And the too-strong half, which shipped for one round.** `run`'s
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
        // **And the variant each reader passes.** The collapse turned a whole
        // function body into one token at three call sites, and nothing read it:
        // planted in round seventy-one, `compose_command` asking for
        // `Captured(Stdout)` instead of `Through` left the suite green while
        // `cargo xtask dev` swallowed docker's entire transcript during a
        // two-image pull and the long diagnosis above became unreachable from
        // every caller. Their bodies are one line each, so they are held whole.
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
    /// hundred-line bound, when the pattern cases arrived. The two are one
    /// question asked of two things: which invocations in this file name a
    /// project, and what the predicate counts as a binding of the name.
    #[test]
    fn a_binding_of_the_project_is_counted_however_it_is_written() {
        // **A hoist in every function is still a hoist.** The rule asked for
        // exactly one binding of the name in the whole file, so hoisting the
        // call in each of the three functions that start a compose command —
        // ordinary, `fmt`-clean, identical behaviour — reported all three as
        // taking the project from the directory's name. Measured on 2026-09-14.
        // What is refused is a binding that comes from somewhere else, and that
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
        // `let project = ` at all, so the count of bindings did not see it and
        // one pinned binding elsewhere in the file paid for it. The function it
        // was planted in is the one that runs `down -v`.
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
        // is not a binder — so a hoist followed by an ordinary mutation counted
        // as one binding, one pinned, and the gate reported the invocation as
        // naming what this tree computes. `truncate` is the cheap one: it
        // yields `noctua-central`, the directory's own name, which is the
        // collision `project_for` exists to prevent. Measured in round
        // sixty-seven against `compose_command`, the function that runs
        // `up -d --remove-orphans` and `down -v`.
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
    /// **Eight clauses, and a fixture reached none of them.** The two tables
    /// above drive the five *arms* of [`the_names_a_binder_introduces`]
    /// — a `let` head, a `for` head, a function's parameters, a closure's
    /// parameters, a `match` arm — and nothing drove the guards, cuts and
    /// boundary tests inside them. Each was deleted alone in round sixty-seven
    /// and every one left the whole suite green, and `xtask/mutants.py` carries
    /// no operator over this function at all.
    ///
    /// Two of them protect *correct* code and were undecided in both
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
        // way, which is this round's own defect written into its own fixture.
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
    /// Split from [`a_binding_of_the_project_is_counted_however_it_is_written`],
    /// which had grown past what one body may hold, and separate on its own
    /// merits: every spelling there writes `let <name>` as two adjacent words
    /// and every spelling here does not, which is the difference the walk over
    /// the grammar exists for.
    #[test]
    fn a_binding_written_as_a_pattern_or_a_parameter_is_counted_too() {
        // **A pattern is a binding.** Three spellings, none of which holds
        // `let project` as adjacent words: a tuple, a refutable pattern, and a
        // loop's own binding. The plant that produced this list sat in
        // `compose_command`, the function that runs `up -d --remove-orphans`
        // and `down -v`, and the whole gate stayed green.
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
        // `match` arm's: none of the three has a `let` or a `for` head, so the
        // rule that read heads alone could not see them. The closure case is
        // the one measured in round sixty-five — `compose_command`'s invocation
        // moved into `|project: String|`, with one legitimate hoist elsewhere
        // paying for it — and it left the whole gate green while both compose
        // verbs ran under the fixed project every checkout is called.
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
        // **And a hoist written with a type annotation is a hoist.** Round
        // sixty-three taught this rule to *see* that spelling; nothing taught
        // it to accept it, so the pinned form stayed the bare literal and an
        // ordinary line printed the most alarming message this tool has.
        // Reported in round sixty-five. A binding is pinned by its **value**.
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
        // `.args(["-p", &project_for(root)])` beside a live invocation made the
        // chunk's flag count 2 — and one occurrence is all the rule below asks
        // for. Measured on 2026-09-11. `without_comments` is a state machine
        // over the whole file: a block comment nests, a raw string ends at its
        // own hash count, and neither opens inside a string.
        let code = crate::gates::without_comments(body, "//");

        // **And where that `id` came from.** The exemption asks for the binding
        // by name, and a name is not a provenance: one line added inside
        // `where_the_logs_container_mounts` — `let id = "noctua-central-logs-1";`
        // after the project-scoped read — left this whole suite green and
        // pointed the command at the container a developer's own checkout
        // produces. Measured on 2026-09-11. So the file has to bind `id` from
        // `compose ps -q` and must not bind it again between there and the
        // `inspect`: two bindings, the read and its `trim`, and no third.
        // **And the four lines that hand the id over, held whole.** Three rules
        // have been written about the text between the project-scoped read and
        // the command — a spelling (`let id = `), a count of `let`, and a name
        // in the callee's body — and a reviewer walked through each in turn. The
        // third scanned the callee and left the caller open, so
        // `let id = "noctua-central-logs-1";` on the line above the hand-over
        // still passed, which is the third round running this one gate has
        // fallen to the same class.
        //
        // There is nowhere to put that line now: the caller is a `match` whose
        // binding's only use is the call, and the whole block is compared. An
        // edit here is a failure that says to read four lines rather than a
        // number to move.
        // **The whole body**, from the signature to its closing brace. The
        // block was pinned as four lines, which says they are written somewhere
        // in the function rather than that they are the function: three lines
        // above them, returning the mount of a container named by the
        // directory's own project, left all four pins green and `check passed`.
        // Comments included, because a comment is where the next reader is told
        // what this holds.
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

        // **And the hole, not just the rule.** The exemption used to ask which
        // subcommand ran; the sentence justifying it is about what the
        // subcommand is *handed*. Measured on 2026-09-11: the one exempted call
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
    /// is the fourth time that bound has been paid rather than allowed in this
    /// crate. The two halves are one rule asked of two things: what the verb
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
        // above is about the *invocation*; three failure messages printed
        // `` `docker compose up -d --remove-orphans` failed `` with no `-p`, so
        // the command a developer was handed addressed the directory's project —
        // which is by construction never the one that ran. Running it lists
        // nothing, and running `up` with it creates a second stack.
        let mut bare = Vec::new();
        for (index, line) in body.lines().enumerate() {
            // Code, not the prose that explains it: the doc on `failed` writes
            // the shape of a bare command in order to say it is wrong.
            let line = crate::gates::line_without_a_comment(line);
            // **Every occurrence, not the first.** `find` stops at one, so a
            // scoped command earlier on the same source line hid a bare one
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
    /// **A constant failed both halves.** `-p noctua-central`, written out,
    /// made every directory on the machine address one stack: a reviewer reached
    /// the live database from a scratch directory holding a two-line toy compose
    /// file and nothing else. The name has to separate checkouts, and it has to
    /// keep naming the same volumes tomorrow, or a developer's data is orphaned
    /// by a rename nobody asked for.
    #[test]
    fn the_project_name_does_not_move_under_a_checkout() {
        use std::path::Path;

        // Frozen. If this number changes, every existing checkout's stack is
        // renamed and its volumes are stranded under the old name.
        assert_eq!(super::digest_of("/home/dev/noctua-central"), 0x5e24_507c);
        assert_eq!(super::digest_of(""), 0x811c_9dc5);
        // **And the rendering, not only the digest.** `{:08x}` became `{:x}`
        // with all five of these green — which drops the leading zero for one
        // checkout path in sixteen and renames its project, stranding the
        // volumes the paragraph above says this exists to protect.
        assert_eq!(
            super::project_for(Path::new("/nonexistent/9/noctua-central")),
            "noctua-central-05d42f30"
        );

        // **A path that exists, so the canonicalizing branch is the one under
        // test.** Every path here was `/nonexistent/…`, which takes the
        // fallback — so deleting the `canonicalize` call the doc comment calls
        // load-bearing left all five of these green, and a symlinked checkout
        // and its target would have been two stacks.
        let scratch = std::env::temp_dir().join(format!("nc-project-{}", std::process::id()));
        let real = scratch.join("noctua-central");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&real).expect("writable");
        #[cfg(unix)]
        {
            let linked = scratch.join("by-another-name");
            std::os::unix::fs::symlink(&real, &linked).expect("a symlink");
            assert_eq!(
                super::project_for(&real),
                super::project_for(&linked),
                "a symlinked checkout and its target were given two stacks, so a \
                 `dev --down` through one would leave the other's containers"
            );
        }
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

    use super::the_log_store_is_serving;
    use std::io::{Read as _, Write as _};

    /// A server that answers `status` once, then closes.
    ///
    /// Reads the request and shuts the write side down before closing: dropping
    /// a socket with unread data sends an RST, which destroys the reply about
    /// one run in eight. `nc-log`'s fake servers learned this the same way.
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
        // printed its URL.
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
        // `cargo test`'s own parallelism the connect sometimes succeeded and
        // this failed for a reason that had nothing to do with the store. Port
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
    /// **Both halves of this were free.** The count was read with
    /// `unwrap_or(0)` — the shape repaired ninety lines above for its two
    /// neighbours, *a permission error, a wrong path or a dropped connection all
    /// read as "nothing has been applied"* — and the difference was taken with
    /// `saturating_sub`, which on `i64` saturates at `i64::MIN` rather than at
    /// zero. Measured with the pool closed before the read: `ok -12
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
        // **Every count read in `migrate`, at any indentation.** This scanned
        // for one byte sequence — the read, its two continuations at twelve
        // spaces, and `.unwrap_or(` — and `migrate` holds three
        // `count(*) FROM _sqlx_migrations` reads at two indentations. Only one
        // had that shape, and the failure path's read four lines above it was
        // `.unwrap_or(already)` with a `saturating_sub` under it: the pair the
        // repair beside it exists to remove. Written as a property, the rule
        // reads every one of them.
        let reads = code.match_indices("FROM _sqlx_migrations\")").count();
        assert!(
            reads >= 2,
            "this scan found {reads} count read(s) in the file and there are more than \
             one, so it is looking for the wrong text"
        );
        // **To the end of the function, not to the next semicolon.** The scan
        // read each arm up to its `;`, and a fallback moves past that with one
        // more binding: measured on 2026-09-12,
        // `let counted = …map_err(…); let now = counted.unwrap_or(0);` left
        // `check passed` and a transcript differing only in the source-line
        // counts the extra line moves — the message string does not move, so
        // the sentence census, which is what caught this shape when the rule
        // was written, said nothing.
        //
        // `migrate` legitimately falls back nowhere, so the question is asked
        // of the whole body rather than of one statement. The three
        // `unwrap_or` in it today are all inside comments, which are spent
        // above.
        let opens = "fn migrate(";
        let body_at = code.find(opens).expect("this file declares `migrate`");
        let body = &code[body_at..];
        let body_end = body.find("\n}\n").unwrap_or(body.len());
        let body = &body[..body_end];
        // **The shape of the name, not four of them.** This listed
        // `.unwrap_or`, `.or(`, `.or_else(` and `.unwrap_or_default` under a
        // sentence about every spelling a fallback wears, and the arm rule
        // beside it was rewritten to an allow-list for exactly that reason —
        // leaving the body half one binding away from free. Measured:
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
            // The first version of this rule was `starts_with("unwrap_or") ||
            // ends_with("_or") || contains("map_or") || == "or"`, and
            // `or_else` — which the deny-list it replaced named by hand —
            // satisfies none of the four. Measured: `.or_else(|error| Ok(…))`
            // one binding below the read left `check passed` with 1288 test(s)
            // and a transcript differing only in the source-line counters the
            // extra line moves. A repair that drops a spelling the list it
            // replaced already caught is the *too weak* half of the rule this
            // file's own paragraph warns about.
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
            // **Every spelling a fallback wears, not one of them.** The rule
            // asked for `.unwrap_or` and `.ok().or(Some(0))` is the same
            // sentence in different words — measured, it passed. What a read of
            // this count may end in is `map_err(…)?`, which stops the run, or
            // `.ok()`, which makes *unread* an answer of its own; anything that
            // substitutes a number is a claim about the database that the read
            // did not establish.
            // **What it may end in, rather than what it may not.** The rule
            // was four spellings written out under a sentence promising *every*
            // spelling a fallback wears, and there are more than four:
            // `.map_or(0, |counted: i64| counted.max(0))` is a fifth, it leaves
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
        // declares`, and **nothing asked whether this file reads them**. It did
        // not: `run` read `POSTGRES_PORT` with its own `std::env::var` and its
        // own inline `"5432"`, which is the port the migrations land in and the
        // port the success line prints. Measured with that literal written
        // `"5433"`: `check passed`, byte-identical to a clean run, while
        // `cargo xtask dev` brought a stack up on one port and migrated another.
        //
        // A literal name inside `std::env::var` is what a bypass looks like;
        // `setting` itself reads the variable it is handed, so it holds none.
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dev.rs"))
            .expect("this file is a file in this crate");
        // **Comments only.** The first version of this spent the string contents
        // too — and a variable's name lives *inside* the string it is read with,
        // so the scan removed the very text it was looking for and passed
        // against the planted bypass. A scanner that reads a literal as code is
        // a shape this repository has repaired repeatedly; this is its mirror,
        // and it is the one that is green.
        let code = crate::gates::without_comments(&source, "//");
        for (variable, default) in super::THE_STACK_SETTINGS {
            assert!(
                !code.contains(&format!("env::var(\"{variable}\")")),
                "`{variable}` is read here with its own `env::var` rather than through \
                 `setting`, so the default beside it is a copy of `{default}` that \
                 nothing holds to the stack"
            );
        }
        // The floor: a scan that spent the whole file would report no bypass
        // whatever this file said.
        assert!(
            code.contains("fn setting("),
            "the scan read no code at all, so it agrees with any file"
        );
    }

    /// The migrated schema is the one the URL names, and it comes first.
    ///
    /// **`migrate` is tested by nothing, and this is the line that matters.**
    /// A `"$user",` in front of the schema is one token, `cargo fmt` and
    /// `cargo clippy` are silent about it, and with a role-named schema present
    /// `dev` then migrated *that* one — a second `_sqlx_migrations` with twelve
    /// rows, a second `journal_entries` with its append-only triggers — while
    /// reporting it had applied nothing, and `cargo xtask check` passed over
    /// the result.
    ///
    /// `check` resolves the same schema from the same string, so the two can
    /// never contradict each other: they agree by both being right, which is
    /// why only a test of this statement can notice when one stops.
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
        // And the ordinary role-named case, which is the one that bit.
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
        // way, and this file wrote the opposite for eleven rounds.
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
    /// **`read` returns what has arrived.** The health check took one call into
    /// a 64-byte buffer, so a store answering `HTTP/1.1 ` in one segment and
    /// `200 OK` in the next was read as `HTTP/1.1 ` — no code — and `dev`
    /// reported a healthy store as "running and not serving". Nothing is wrong
    /// with the store in that case, which is what makes it expensive: the
    /// failure is intermittent and points at the wrong thing.
    ///
    /// Both halves are driven here, against a listener that writes the answer
    /// the two ways, because a reader that only ever sees whole lines cannot
    /// tell the repair from the defect.
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
    /// **Four copies of five values, held to nothing.** `.env.example` and
    /// `docker-compose.yml` are held together by
    /// `the_example_environment_agrees_with_the_compose_defaults`; the code that
    /// reads the same five was not held to either. Measured on 2026-09-09,
    /// `POSTGRES_DB`'s default changed in this file alone: `check passed` at
    /// `1232 test(s)` with no warn line, and `cargo xtask dev` failed with
    /// `database "central" does not exist`.
    ///
    /// **Both directions, because one is a sample.** Forwards: every setting
    /// this file declares is declared in the compose file with the same default.
    /// Backwards: every `${NAME:-value}` the compose file substitutes is one of
    /// them — a sixth added to the stack and not added here is a setting the
    /// tools do not read, which is the same stack with two configurations.
    #[test]
    fn the_settings_this_tool_reads_are_the_ones_the_stack_declares() {
        let compose = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docker-compose.yml"
        ))
        .expect("this repository has a compose file");
        // `${NAME:-value}`, read as the compose file substitutes it.
        let mut declared: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        let mut rest = compose.as_str();
        while let Some(at) = rest.find("${") {
            rest = &rest[at + 2..];
            let Some(close) = rest.find('}') else { break };
            let (inside, after) = rest.split_at(close);
            rest = &after[1..];
            let Some((name, default)) = inside.split_once(":-") else {
                continue;
            };
            declared.insert(name.to_owned(), default.to_owned());
        }
        assert!(
            declared.len() >= super::THE_STACK_SETTINGS.len(),
            "the compose file substitutes {} variable(s), which is fewer than this file \
             declares — the scan above read the wrong text: {declared:?}",
            declared.len()
        );
        for (variable, default) in super::THE_STACK_SETTINGS {
            assert_eq!(
                declared.get(variable).map(String::as_str),
                Some(default),
                "`{variable}` is `{default}` here and `{:?}` in `docker-compose.yml`, so \
                 `cargo xtask dev` brings up one stack and the tools talk to another",
                declared.get(variable)
            );
        }
        let unread: Vec<&String> = declared
            .keys()
            .filter(|name| {
                !super::THE_STACK_SETTINGS
                    .iter()
                    .any(|(variable, _)| *variable == name.as_str())
            })
            .collect();
        assert!(
            unread.is_empty(),
            "`docker-compose.yml` reads {unread:?}, which this file does not declare — a \
             setting the stack takes and the tools do not is how the two come apart"
        );
    }

    #[test]
    /// A migration numbered below one already applied is refused, and nothing
    /// else is.
    ///
    /// **The whole forward-only rule, decided by nothing.** See
    /// [`super::is_backdated`] for what a neutered guard measured: the workspace
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
