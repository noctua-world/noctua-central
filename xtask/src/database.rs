//! The gate needs a database, and says so.
//!
//! **A check that silently skips its integration tests when `DATABASE_URL` is
//! unset is the `NOCTUA.md §19.3` hazard in its purest form**: the suite goes
//! green, the count goes up, and the tests that would have caught a wrong column
//! never ran. `crates/nc-journal/tests/in_postgres.rs` returns early without a
//! url — which is right for a test binary somebody runs by hand, and would be
//! catastrophic as the gate's default.
//!
//! So the default **fails**, with the command that fixes it. The escape is
//! `--offline`, which *skips* and says so — the `--colors-only` shape of
//! `§19.4` — and CI never passes it.

use std::path::Path;

use crate::ui;

/// The tables whose protections this gate inspects, named once.
///
/// **Eleven hand-written lists said these two names and nothing tied any of
/// them to the tables the migrations create.** `check.rs`'s `SHAPES` held the
/// pair, and the ten `IN (…)` lists below held it again — so
/// `the_tables_are_the_shape_they_were_made` filtered its answer with the
/// predicate it was testing: [`table_shapes`] returns rows for these two names,
/// and the assertion then looked those two names up in what came back.
///
/// **What is out of scope, written down rather than left silent.** A third
/// table added by a migration fails the unbounded-table gate, which is about
/// *rows*. Nothing else here would see it: its columns, its triggers, its
/// constraints, its indexes, its rules and its grants would be inspected by
/// nobody, under `ok  44 protection(s) on the journal and the access records`.
/// Measured — `CREATE TABLE tenant_numbers` added to
/// `migrations/0002_access_records.sql` with a matching `[tables.*]` entry and
/// the catalog regenerated — `cargo test -p xtask --bins` reported
/// `411 passed; 0 failed`, and the only line in `xtask/src/` naming the table
/// was a comment about a sequence.
///
/// Closing that generally is not a gate's decision: what protections a new
/// table deserves is a judgement, and an append-only trigger belongs on an
/// audit log and not on a cache. So this list is held to one cheap identity
/// instead — `the_tables_the_gate_inspects_are_the_ones_the_migrations_make`
/// asserts that it *is* the set of tables the migrations create, less the ones
/// declaring [`crate::limits::BOUNDED`] with a reason. A third table then fails
/// the moment it is added, naming itself, and the repair is a deliberate line
/// here saying which of these inspections it gets.
///
/// The order is not load-bearing — `relname IN (…)` is a set and the assertion
/// sorts both sides — so it is alphabetical, which is how a reader checks a list
/// of two against a list of two.
pub(crate) const PROTECTED: [&str; 2] = ["access_records", "journal_entries"];

/// Where a query below wants [`PROTECTED`] spliced in as a SQL list.
///
/// Not a bind parameter: `IN ($2)` is one value to PostgreSQL, and the ten
/// lists this stands in for are a fixed set of identifiers from this file
/// rather than anything a caller supplies.
const THE_PROTECTED_TABLES: &str = "{tables}";

/// `sql` with every [`THE_PROTECTED_TABLES`] replaced by the names themselves.
///
/// The names are quoted as SQL strings, which is what `relname IN (…)` compares
/// against. They are lower-case identifiers written in this file and hold no
/// quote to escape; what keeps them the right names is the assertion named on
/// [`PROTECTED`].
fn asking_about_the_protected_tables(sql: &str) -> String {
    let list = PROTECTED
        .iter()
        .map(|table| format!("'{table}'"))
        .collect::<Vec<_>>()
        .join(", ");
    sql.replace(THE_PROTECTED_TABLES, &list)
}

/// Where the local stack puts PostgreSQL.
///
/// **The same four variables `cargo xtask dev` reads, with the same defaults**,
/// which is a correction: this hardcoded the url, so
/// `POSTGRES_PORT=5433 cargo xtask dev` succeeded and
/// `POSTGRES_PORT=5433 cargo xtask check` then told the developer to run
/// `cargo xtask dev` — the command they had just run. `dev.rs` names the rule:
/// a stack whose operator variables the tool that brings it up does not read is
/// a stack with two configurations. The tool that was not reading them was this
/// one.
///
/// `DATABASE_URL` still wins when it is set, because a developer pointing the
/// gate at another server is saying so explicitly.
fn default_url() -> String {
    // **The stack's own table**, rather than a fourth copy of these four
    // literals. The paragraph above says "the same defaults" and nothing made
    // that true: `POSTGRES_DB` changed in `dev.rs` alone left `check passed`
    // and broke `cargo xtask dev`.
    let user = crate::dev::setting(crate::dev::POSTGRES_USER);
    let password = crate::dev::setting(crate::dev::POSTGRES_PASSWORD);
    let database = crate::dev::setting(crate::dev::POSTGRES_DB);
    let port = crate::dev::setting(crate::dev::POSTGRES_PORT);
    format!("postgres://{user}:{password}@127.0.0.1:{port}/{database}")
}

/// The url the test suite should run against, or why it cannot.
///
/// # Errors
///
/// Returns what to do when nothing is listening.
pub(crate) fn url() -> Result<String, String> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| default_url());
    // **Connectivity, not just a string.** A url that points at nothing produces
    // a suite that passes having skipped every integration test, which is the
    // failure this module exists to prevent — and a variable somebody exported
    // last week and forgot is exactly how that happens.
    if reachable(&url) {
        return Ok(url);
    }
    Err(format!(
        "nothing is listening for PostgreSQL at {}. The journal's chain is proven \
         against the store that runs, not only in memory — so this gate needs one.\n\
         Run `cargo xtask dev` to bring the stack up, or `cargo xtask check --offline` \
         to skip the tests that need it. CI never passes --offline",
        redacted(&url)
    ))
}

/// Whether something accepts a TCP connection at the url's host and port.
///
/// Deliberately not a query. What the gate needs to know is whether running the
/// suite is worth doing; sqlx will produce a far better message than this could
/// if the server is there and refuses. A connect with a timeout is the cheapest
/// question that distinguishes "the stack is down" from "the stack said no".
fn reachable(url: &str) -> bool {
    let Some((host, port)) = host_and_port(url) else {
        return false;
    };
    let Ok(addresses) = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port)) else {
        return false;
    };
    addresses.into_iter().any(|address| {
        std::net::TcpStream::connect_timeout(&address, core::time::Duration::from_millis(500))
            .is_ok()
    })
}

/// The host and port of a PostgreSQL connection url.
fn host_and_port(url: &str) -> Option<(String, u16)> {
    let rest = url.strip_prefix("postgres://")?;
    let authority = rest.split('/').next()?;
    // Credentials are before the last `@`; a password may contain one.
    let authority = authority.rsplit('@').next()?;
    let (host, port) = authority.split_once(':')?;
    Some((host.to_owned(), port.parse().ok()?))
}

/// A url with its password removed, for a message somebody may paste.
fn redacted(url: &str) -> String {
    host_and_port(url).map_or_else(
        || "the url in DATABASE_URL".to_owned(),
        |(host, port)| format!("{host}:{port}"),
    )
}

/// Reports what the gate decided about the database.
pub(crate) fn report(offline: bool, failures: &mut Vec<String>) -> Option<String> {
    ui::heading("database");
    if offline {
        ui::warn("skipped: --offline, and the journal's PostgreSQL tests need one");
        ui::detail("CI never passes --offline, so those tests always run there");
        return None;
    }
    match url() {
        Ok(url) => {
            ui::ok(&format!("postgres is reachable at {}", redacted(&url)));
            Some(url)
        }
        Err(problem) => {
            ui::failure(&problem);
            failures.push("no database".to_owned());
            None
        }
    }
}

/// The checksum sqlx computes for each migration **as it is on disk now**.
///
/// **The other half of `what ran is what the file says`.** The database records
/// a checksum for every migration it applied and the gate read past it, so an
/// applied migration could be rewritten with the run green. This is the same
/// arithmetic on the other side, so no hash function has to be chosen here.
///
/// **From the tree, not from the binary.** It read `sqlx::migrate!`, which
/// embeds the files at *compile* time, and cargo decides what to rebuild by
/// mtime — so the gate compared a database against a build artifact and both
/// directions were wrong:
///
/// ```text
/// edit 0003, then `touch -r` its original mtime
///   -> nothing rebuilt, ok 6 migration(s), check passed
///   -> the file now reads CHECK (sequence >= -1), inverting the constraint
///      that migration exists to add
/// restore 0003 to its correct bytes with its older mtime
///   -> the pristine tree reported as tampered
/// ```
///
/// `Migrator::new` reads the directory at run time, which is what
/// `cargo xtask dev` applies from, so both sides are live.
///
/// # Errors
///
/// Returns why `migrations/` could not be read as a migration set.
pub(crate) fn on_disk_checksums(root: &Path) -> Result<Vec<(i64, Vec<u8>)>, String> {
    on_disk_checksums_in(root, crate::limits::MIGRATIONS)
}

/// The same, for the migrations in `directory` under `root`:
/// `local-migrations/` as well, whose checksums no database of this machine's
/// remembers.
///
/// # Errors
///
/// Returns why `directory` could not be read as a migration set.
pub(crate) fn on_disk_checksums_in(
    root: &Path,
    directory: &str,
) -> Result<Vec<(i64, Vec<u8>)>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime to read the migrations with: {error}"))?;
    runtime.block_on(async {
        let migrator = sqlx::migrate::Migrator::new(root.join(directory))
            .await
            .map_err(|error| format!("{directory}/ is not a migration set: {error}"))?;
        Ok(migrator
            .migrations
            .iter()
            .map(|migration| (migration.version, migration.checksum.to_vec()))
            .collect())
    })
}

/// Every migration version this database has applied, with its description.
///
/// **Which ones, not how many.** This returned a count and the caller compared
/// it with the number of files, so any deletion compensated by an addition
/// passed: renaming a migration so a rebase's file took its number left
/// `ok 3 migration(s)` and `check passed`, with the database still recording
/// that version as the constraint that had gone. A count cannot say which one
/// went, and which one went is the whole question.
///
/// **The one thing outside the tree that remembers.** There is no git in this
/// repository (`NOCTUA.md §5.3`), so nothing *in* it recalls what was there
/// yesterday — both sides of a tree-derived comparison shrink together.
/// `_sqlx_migrations` holds a row per migration this database has run.
///
/// Empty when the stack has never been brought up, which is every CI run: an
/// empty answer is "nothing to corroborate", not "nothing was applied wrongly",
/// and the caller says which.
///
/// # Errors
///
/// Returns why the database could not be asked.
pub(crate) fn applied(url: &str) -> Result<Vec<(i64, String, Vec<u8>)>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime to ask the database with: {error}"))?;
    runtime.block_on(async {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .map_err(|error| format!("the database did not answer: {error}"))?;
        // **A missing table is not a failed read.** This ended
        // `.unwrap_or_default()`, so a permission error, a wrong `search_path`
        // and a table that has never existed all answered "none applied" — the
        // floor turned itself off silently, and could not say it had.
        // **And the path pinned first, for the reason `function_paths` gives.**
        // `to_regclass` resolves through this connection's `search_path`, so a
        // schema named after the connecting role holding a *copy* of the record
        // answered for it — measured, one row deleted from the copy gave
        // `11 of them are recorded applied` with the real table holding twelve,
        // which redirects the checksum corroboration that makes an applied
        // migration un-rewritable.
        sqlx::query("SET search_path = pg_catalog, pg_temp")
            .execute(&pool)
            .await
            .map_err(|error| format!("the search path could not be pinned: {error}"))?;
        // **In the schema the string names, not the lowest-oid one anywhere.**
        // `function_paths` sixty lines below carries the paragraph that says
        // "neither guess is the answer; the string is" — and that fix was applied
        // there and not here, which is where the applied record and the checksum
        // corroboration that makes an applied migration un-rewritable are read.
        // Measured: `app` recording six and `public` twelve, this reported twelve
        // and took its checksums from a table the connection's path never
        // reaches, while the half beside it inspected `app`.
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT c.oid::regclass::text FROM pg_class c \
             WHERE c.relname = '_sqlx_migrations' AND c.relkind = 'r' \
               AND c.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = $1)",
        )
        .bind(configured_schema(url))
        // **`fetch_optional`, because zero rows is the ordinary case.**
        // `fetch_one` turns "no such table" into `RowNotFound`, so the
        // `is_none()` branch below was dead code and the paragraph above it — "an
        // empty answer is nothing to corroborate, which is every CI run" —
        // described a path nothing could reach. Measured against exactly what CI
        // gives this: a fresh `postgres:18-alpine` service with an empty database
        // and no migrations applied, `check` reported `error the migration record
        // could not be looked for: no rows returned by a query that expected to
        // return at least one row` and `check failed`. The workflow runs
        // `cargo xtask check` and nothing applies the migrations first, so the
        // gate could not pass on the database its own CI hands it.
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("the migration record could not be looked for: {error}"))?
        .flatten();
        if exists.is_none() {
            pool.close().await;
            return Ok(Vec::new());
        }
        // `oid::regclass` prints a name this connection can use as written: bare
        // when the schema is on its path, qualified and quoted when it is not.
        // Nothing here interpolates anything a caller supplied — the value comes
        // from `pg_class` — and the alternative, resolving by path, is the defect
        // this pinning exists to close.
        // **And the checksum, which nothing read.** It is in the row — 48 bytes
        // of SHA-384, for every one of them — and the gate SELECTed past it. So
        // an *applied* migration could be rewritten and `check` stayed green:
        // `0003`'s `CHECK (sequence >= 0)` gutted to `>= -9223372036854775808`
        // still printed `ok 6 migration(s), numbered once each and in order`.
        // sqlx refuses a changed migration when it *applies* one, and CI applies
        // to a fresh database where nothing is recorded, so that refusal never
        // runs there. The record is what remembers, and this is what reads it.
        // **Read from the table that was found, by name.** Every name here is
        // unqualified and the path is pinned to `pg_catalog` above, so this asked
        // for a table it had just decided not to resolve by path. `exists` holds
        // the qualified name `oid::regclass` printed, which is the one table this
        // function is about.
        let named = exists.unwrap_or_default();
        let rows: Vec<(i64, String, Vec<u8>)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT version, description, checksum FROM {named} WHERE success \
             ORDER BY version"
        )))
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("the migration record could not be read: {error}"))?;
        pool.close().await;
        Ok(rows)
    })
}

/// The schema a connection string says its statements run in.
///
/// **The URL, not the connection.** Every way of asking the database —
/// `current_schema()`, `(current_schemas(false))[1]`, `to_regclass` — resolves
/// through `search_path`, and `search_path` begins `"$user"` by default. They all
/// move together the moment a schema named after the connecting role exists,
/// which is what `CREATE SCHEMA AUTHORIZATION` produces on purpose and what the
/// twelfth migration calls an ordinary arrangement.
///
/// `public` when the string says nothing, because that is where the migrations
/// create their tables, and a deployment that puts them elsewhere says so in the
/// string it already writes.
pub(crate) fn configured_schema(url: &str) -> String {
    // **Percent-decoded, because sqlx decodes and this did not.** sqlx builds its
    // connection from `query_pairs()`, and the canonical spelling of this option
    // writes `%3D` for the `=` and `%20` for the space — so the connection went
    // to one schema and this read another:
    //
    // ```text
    // ?options=-csearch_path%3Dapp    sqlx -> "app"   this -> "public"
    // ?options=-c%20search_path%3Dapp sqlx -> "app"   this -> "public"
    // ?options=-csearch_path=app      sqlx -> "app"   this -> "app"
    // ```
    //
    // Measured end to end: with the encoded form, three unpinned functions in
    // `app` and three honest ones in `public`, the gate reported `ok 3 migrated
    // function(s) search pg_temp last` and a foreign row went to the top of the
    // journal's range.
    //
    // **And split on `-c` as a token, not as a substring.** `?options=-c
    // search_path=my-cool` truncated at the `-c` inside the schema's own name.
    let Some(query) = url.split_once('?').map(|(_, query)| query) else {
        return "public".to_owned();
    };
    let options = query
        .split('&')
        .filter_map(|part| part.strip_prefix("options="))
        .map(percent_decoded)
        .collect::<Vec<_>>()
        .join(" ");
    // `-c name=value`, with the space optional — both are what libpq accepts.
    let mut words = options.split_whitespace().peekable();
    let mut settings: Vec<String> = Vec::new();
    while let Some(word) = words.next() {
        if word == "-c" {
            if let Some(next) = words.next() {
                settings.push((*next).to_owned());
            }
        } else if let Some(rest) = word.strip_prefix("-c") {
            settings.push(rest.to_owned());
        }
    }
    settings
        .iter()
        .filter_map(|setting| setting.strip_prefix("search_path="))
        .flat_map(|path| path.split(','))
        .map(|name| name.trim().trim_matches('"').to_owned())
        .find(|name| !name.is_empty() && name != "$user" && name != "pg_temp")
        .unwrap_or_else(|| "public".to_owned())
}

/// `%XX` decoded, and `+` left alone.
///
/// A dozen lines rather than a dependency: `AGENTS.md`'s third rule says a
/// dependency that could reasonably be written here is one to ask about first,
/// and this is the whole of what is needed. `+` is a form-encoding convention and
/// not a URL one; libpq does not apply it, so neither does this.
fn percent_decoded(text: &str) -> String {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0_usize;
    while at < bytes.len() {
        if bytes[at] == '%'
            && let (Some(high), Some(low)) = (bytes.get(at + 1), bytes.get(at + 2))
            && let (Some(high), Some(low)) = (high.to_digit(16), low.to_digit(16))
            && let Some(decoded) = char::from_u32(high * 16 + low)
        {
            out.push(decoded);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    out
}

/// Every column of every protected table, as `pg_attribute` describes it.
///
/// Held apart from its one call site for the reason [`THE_PROTECTIONS`] is: the
/// table list in it is spliced from [`PROTECTED`] rather than written out, so
/// the statement is a `String` by the time it reaches sqlx.
const THE_SHAPES: &str = "SELECT c.relname::text, \
                    string_agg( \
                      a.attname || ' ' || pg_catalog.format_type(a.atttypid, a.atttypmod) \
                        || CASE WHEN a.attnotnull THEN ' not null' ELSE '' END \
                        || ' storage ' || a.attstorage::text \
                        || a.attcompression::text \
                        || CASE WHEN a.attcollation = 0 THEN '' \
                                ELSE ' collate ' \
                                     || co.collnamespace::regnamespace::text \
                                     || '.' || co.collname::text \
                                     || ':' || co.collprovider::text END, \
                      ', ' ORDER BY a.attnum) \
             FROM pg_attribute a \
             JOIN pg_class c ON c.oid = a.attrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_collation co ON co.oid = a.attcollation \
             WHERE n.nspname = $1 AND c.relname IN ({tables}) \
               AND a.attnum > 0 AND NOT a.attisdropped \
             GROUP BY c.relname ORDER BY c.relname";

/// The columns each protected table holds, as the live database describes them.
///
/// **The third thing the migrations create that nothing checked.** After the
/// triggers and the constraints, the columns themselves: `ALTER TABLE
/// journal_entries ALTER COLUMN hash DROP NOT NULL` left `check passed`, and a
/// journal row with no hash is a row `verify` cannot read and the append-only
/// trigger will not let anyone remove.
///
/// Name, type and nullability, in column order — the whole shape rather than one
/// column at a time, because a type widened or a column reordered is the same
/// class of change and picking favourites is how a list of favourites grows.
///
/// # Errors
///
/// Returns why the database could not be asked.
pub(crate) fn table_shapes(url: &str) -> Result<Vec<(String, String)>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime to ask the database with: {error}"))?;
    let schema = configured_schema(url);
    runtime.block_on(async {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .map_err(|error| format!("the database could not be reached: {error}"))?;
        let query = asking_about_the_protected_tables(THE_SHAPES);
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(&*query))
            .bind(&schema)
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("the tables' shapes could not be read: {error}"))?;
        pool.close().await;
        Ok(rows)
    })
}

/// How this database sorts and encodes, as it was actually initialised.
///
/// **`POSTGRES_INITDB_ARGS` applies once, and nothing looked afterwards.**
/// `docker-compose.yml` passes `--locale=C --encoding=UTF8` so that `ORDER BY`
/// answers the same on two machines, and the comment beside it says a mismatch
/// "surfaces as one failing test nobody can reproduce". The argument is read by
/// `initdb`, which runs only when the data directory is empty — so a volume
/// created before that line existed, or by a `docker run` without it, keeps the
/// host's locale forever while the file goes on saying otherwise. The file is
/// now checked; this is the database.
///
/// Returns `(collate, ctype, encoding, provider, settings)` for the database
/// the connection names.
///
/// **The provider, because `datcollate` is not the answer without it.** From
/// PostgreSQL 15 a database's default collation is `datcollate` only while
/// `datlocprovider` is `c`; under the ICU provider `datcollate` goes on reading
/// `C` while every `ORDER BY` on text uses the ICU locale instead. Measured on a
/// database created `LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LC_COLLATE 'C'`,
/// with the twelve migrations applied and the gate green:
///
/// ```text
/// SELECT string_agg(x, ',' ORDER BY x) FROM (VALUES ('a'),('B'),('_z'),('Z')) t(x)
///   ->  _z,a,B,Z          and byte order is  B,Z,_z,a
/// cargo xtask check
///   ok  the database sorts by `C` and is encoded `UTF8`, as the stack initialises it
/// ```
///
/// A database the stack did not initialise is the whole case this exists for,
/// and a non-libc provider is where one comes from.
///
/// **And `settings`, which is what a database was told to do afterwards.**
///
/// Two doors, because `pg_db_role_setting` is only one of them. `ALTER SYSTEM`
/// writes `postgresql.auto.conf` inside the data volume and is neither a role
/// setting nor a trigger, a constraint, an index or a rule — measured,
/// `ALTER SYSTEM SET synchronous_commit = off` followed by `pg_reload_conf()`
/// left the subquery here empty, `SHOW synchronous_commit` answering `off`, and
/// `check passed` printing that the database "carries no stored setting". A
/// setting's `sourcefile` is what tells the two apart: the image's own
/// configuration is `postgresql.conf`, and everything `ALTER SYSTEM` has ever
/// written is in the file beside it.
/// `ALTER DATABASE … SET` and `ALTER ROLE … IN DATABASE … SET` write defaults
/// into `pg_db_role_setting` that every session then inherits, and none of them
/// is a trigger, a constraint, an index or a rule. `synchronous_commit = off`
/// is `USERSET`, needs no superuser, and acknowledges every journal append
/// before its WAL reaches disk — so a host crash loses committed audit entries
/// with no error anywhere. `search_path`, `lock_timeout` and
/// `standard_conforming_strings` go through the same door.
///
/// **The file, not the effective value, and only when this role can see it.**
/// `pg_settings.sourcefile` is populated once a setting has been *applied*, so
/// `ALTER SYSTEM SET …` without `pg_reload_conf()` left the subquery empty while
/// `postgresql.auto.conf` on disk already said otherwise — and the next restart
/// would apply it. `pg_file_settings` answers the question actually being asked:
/// what is written down. Both views also hide their contents from a role without
/// `pg_read_all_settings`, and they hide them as a *silent empty result*, so the
/// gate reported "carries no stored setting" about a database it could not read.
/// The role is asked first, so an unreadable answer is a failure rather than a
/// pass — `default_url` takes `POSTGRES_USER` from the environment and this
/// module documents pointing the gate at another server, which is exactly where
/// the connecting role is not a superuser.
///
/// # Errors
///
/// Returns why the database could not be asked.
pub(crate) fn how_it_sorts(url: &str) -> Result<(String, String, String, String, String), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime to ask the database with: {error}"))?;
    runtime.block_on(async {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .map_err(|error| format!("the database could not be reached: {error}"))?;
        let row: (String, String, String, String, String) = sqlx::query_as(
            "SELECT datcollate::text, datctype::text, \
                    pg_catalog.pg_encoding_to_char(encoding)::text, \
                    datlocprovider::text, \
                    trim(both ', ' FROM \
                      COALESCE(( \
                        SELECT string_agg(setting, ', ' ORDER BY setting) \
                        FROM pg_db_role_setting s, \
                             LATERAL unnest(s.setconfig) AS setting \
                        WHERE (s.setdatabase = d.oid OR s.setdatabase = 0) \
                          AND (s.setrole = 0 \
                               OR s.setrole = (SELECT r.oid FROM pg_roles r \
                                               WHERE r.rolname = current_user))), '') \
                      || ', ' || \
                      COALESCE(( \
                        SELECT string_agg(g.name || '=' || g.setting, ', ' ORDER BY g.name) \
                        FROM pg_settings g \
                        WHERE g.source = 'client' \
                          AND g.name NOT IN ('search_path', 'application_name', \
                                             'client_encoding', 'DateStyle', 'TimeZone', \
                                             'extra_float_digits', 'bytea_output', \
                                             'client_min_messages', 'lc_messages', \
                                             'default_transaction_read_only', \
                                             'IntervalStyle', 'standard_conforming_strings')), '') \
                      || ', ' || \
                      CASE WHEN NOT has_table_privilege(current_user, \
                                                       'pg_catalog.pg_file_settings', \
                                                       'SELECT') \
                           THEN 'this role cannot read the stored settings' \
                      ELSE COALESCE(( \
                        SELECT string_agg(f.name || '=' || f.setting, ', ' ORDER BY f.name) \
                        FROM pg_file_settings f \
                        WHERE f.sourcefile LIKE '%postgresql.auto.conf'), '') END) \
             FROM pg_database d WHERE datname = current_database()",
        )
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("the database would not say how it sorts: {error}"))?;
        Ok(row)
    })
}
/// The one query that asks what protects the journal and the access records.
///
/// **`AND a.grantee <> 0` is gone from the `writers=` count, and `0` is
/// `PUBLIC`.** That clause filtered out the one grant shape that reaches every
/// role, present and future. Measured on a live stack:
///
/// ```text
/// GRANT UPDATE, DELETE, TRUNCATE ON journal_entries TO PUBLIC;
///   relacl = {noctua=arwdDxtm/noctua,=wdD/noctua}
///   journal_entries writers=0
///   ok    44 protection(s) on the journal and the access records …
///   check passed
///
/// the same privilege to a named role:
///   journal_entries writers=1
/// ```
///
/// `AGENTS.md` names `GRANT ALL … TO PUBLIC` as one of the six shapes this
/// reads, and states the property as "no role other than the owner holds
/// `UPDATE`, `DELETE` or `TRUNCATE`" — both false for exactly the shape the
/// first one names in its own parenthesis. The triggers still refuse the
/// statement, so this was a hole in the second line of defence; `writers=`
/// exists because the first line can be disabled by its owner.
/// `a.grantee <> c.relowner` already covers the `acldefault` fallback, so the
/// clause bought nothing.
///
/// **And `relacl` is one of four places a write can be granted from.** The
/// property `AGENTS.md` states is about who can write the table; `writers=` was
/// a statement about `pg_class.relacl` alone, and `access_records` has no
/// append-only trigger, so it is that table's only guard. Two ordinary shapes
/// walked past it, both measured on a live stack:
///
/// ```text
/// GRANT UPDATE (outcome, masked) ON public.access_records TO PUBLIC;
///   relacl stays NULL; the grant lands in pg_attribute.attacl as {=w/noctua}
///   access_records writers=0        -> a non-owner ran UPDATE … SET outcome='allow'
///                                      and the row's outcome changed
///
/// GRANT pg_write_all_data TO <role>;
///   access_records writers=0        -> that role ran DELETE FROM access_records
///                                      and emptied the table
/// ```
///
/// So the row now carries `colwriters=` from `pg_attribute.attacl`, `writeall=`
/// for the membership of the predefined write-everything role, and
/// `superusers=`, which is the fourth door and the one no grant on the table can
/// close. Column privileges in PostgreSQL are `SELECT`, `INSERT`, `UPDATE` and
/// `REFERENCES` — there is no column-level `DELETE`.
///
/// **`UPDATE` alone, and counting `INSERT` here contradicted the row beside
/// it.** `writers=` deliberately omits `INSERT`: the paragraph in `AGENTS.md`
/// naming this property says a list pinning *no grants ever* "broke permanently
/// the first time an application role was given `INSERT`", so a role that may
/// insert is the ordinary case this census has to survive. Counting it at the
/// *column* level made the narrower grant the refused one — measured on a live
/// stack: `GRANT INSERT ON journal_entries` to a scratch role left `check
/// passed` with a transcript identical to a clean run, while
/// `GRANT INSERT (id, realm, principal, occurred_at, outcome, masked) ON
/// access_records` — the same permission, least-privilege — failed the run with
/// `colwriters=6`. A gate that refuses the safer of two spellings teaches the
/// wider one.
///
/// What it must still count is a column-level `UPDATE`, which is a write door
/// the table-level row cannot see: `GRANT UPDATE (outcome, masked)` moves this
/// to 2 and `relacl` does not move at all.
///
/// **And `owners=`, which is the shortest door of the four.** The census counted
/// two role memberships and omitted the one that confers ownership. Measured on
/// a live stack: `GRANT noctua TO <an ordinary login>` moved no number — the row
/// came back byte-identical — and an ordinary `UPDATE` as that role was still
/// refused by the trigger, so the row looked right and read right. Then, as that
/// role, in one batch:
///
/// ```text
/// SET ROLE noctua;
/// ALTER TABLE journal_entries DISABLE TRIGGER USER;
/// UPDATE journal_entries SET action='Escalate', principal='attacker' WHERE sequence=0;
/// -- UPDATE 1, then the four triggers re-enabled ALWAYS
///   ok  44 protection(s) on the journal and the access records …
///   check passed
/// ```
///
/// `migrations/0001_journal.sql` scopes ownership out as "a privilege question,
/// not a schema one" — but this row already answers privilege questions: it
/// counts `pg_write_all_data` members and superusers. It counted two
/// memberships and left out the one that is a rewritten audit log in three
/// statements.
///
/// Held apart from the function so that adding a catalog to it is not also a
/// reason for `clippy::too_many_lines` to ask that the *function* be split —
/// which would put the query in two places, which is how a list of favourites
/// grows in the first place.
///
/// **And the sequence branch is anchored like every other one.** It filtered on
/// the schema alone while the eight branches around it name the two tables, so
/// any sequence in `public` — the next `BIGSERIAL` or identity column on any
/// unrelated table — arrived as an unexplained row and was reported as *"a rule,
/// an index or a trigger that was not in the migrations is one nobody has
/// read"*, followed by the paragraph about `UPDATE`, `DELETE` and `TRUNCATE` on
/// the audit log. Measured with one `CREATE SEQUENCE tenant_numbers`: 45 rows
/// where the list holds 44, and a step whose subject is the journal failing over
/// a table that has nothing to do with it. In an incident that is the wrong
/// place to send a reader. `pg_depend` is what ties a sequence to the table it
/// belongs to, and the two protected tables are what this step is about.
///
/// **And `agrees=` is gone, because it was a claim about a clock.** It asserted
/// that the database's own `recorded` order agrees with the *text* order of
/// `id` — a client-generated Snowflake — and two ordinary rows break it: two
/// nodes writing in the same millisecond with different machine flakes, the
/// higher-flake one committing first. Measured, no DDL, two `INSERT`s:
///
/// ```text
/// error `access_records.recorded` is `ordering distinct=true agrees=false` in this
///       database and nothing here declares it — a rule, an index or a trigger that
///       was not in the migrations is one nobody has read
/// error check failed: 4 of the journal's protections are not in force
/// ```
///
/// An operator is told their audit log is editable because two rows arrived out
/// of order. `catalog/limits.toml` refuses `occurred_at` as an ordering column
/// in as many words — "it holds whatever wrote the row" — and `id` carries the
/// same writer's clock. It also expires: `nc-ids` grows identifiers from twelve
/// characters to thirteen in 2034, after which an alphabetical sort stops
/// matching a chronological one and the assertion is false unconditionally. And
/// it was the one branch here that reads *rows* rather than catalog, so on a
/// table at its 500,000-row bound every `check` sorted it twice.
///
/// `distinct=` stays: that `recorded` is unique is what the sequence default and
/// the trigger actually guarantee.
///
/// **And `ahead=`, which is the half `distinct=` is not.** Uniqueness is not
/// monotonicity, and the six `pg_sequence` columns above are the sequence's
/// *declaration* — `ALTER SEQUENCE … RESTART` writes none of them. Measured, one
/// statement and two ordinary inserts: the two newest rows carried `recorded` 1
/// and 2 while the oldest carried 4 and 5, every value distinct, every
/// `pg_sequence` column identical, `ok 44 protection(s) in force` and `check
/// passed` — and `delete_oldest("access_records", "recorded", 2)` then answered
/// `Ok(2)` having removed **the two newest rows**. The pruner's whole promise is
/// that it takes the oldest.
///
/// The fact is clock-free, which is what `agrees=` was not: every value in the
/// column came from `nextval`, so the sequence is at or above the largest of
/// them, in every database, for ever. It reads the sequence's *value* rather
/// than its declaration, and it reads no rows the row above did not already
/// read. An empty table has no largest value and is not a violation.
///
/// It names the table the way the half beside it does — unqualified, through
/// the path this connection pins before it asks anything. Written `$1`-qualified
/// instead, the two halves of one row can name two different relations, and the
/// first thing that happened is that they did: the unit test below drives this
/// statement with a scratch schema in `$1` and no `search_path` set, so
/// `distinct=` read the developer's own table while the qualified name found a
/// scratch table with no such column. One row, one relation.
///
const THE_PROTECTIONS: &str = "SELECT t.tgname::text, \
                    'trigger ' || t.tgenabled::text || ' ' \
                      || replace(pg_catalog.pg_get_triggerdef(t.oid), \
                                 quote_ident($1) || '.', '') \
                      || ' body:' || md5(p.prosrc) \
             FROM pg_trigger t \
             JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_proc p ON p.oid = t.tgfoid \
             WHERE c.relname IN ({tables}) \
               AND n.nspname = $1 AND NOT t.tgisinternal \
             UNION ALL \
             SELECT k.conname::text, 'constraint ' || pg_catalog.pg_get_constraintdef(k.oid) \
             FROM pg_constraint k \
             JOIN pg_class c ON c.oid = k.conrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname IN ({tables}) \
               AND n.nspname = $1 \
             UNION ALL \
             SELECT i.relname::text, \
                    'index ' || replace(pg_catalog.pg_get_indexdef(x.indexrelid), \
                                        quote_ident($1) || '.', '') \
                      || ' valid=' || x.indisvalid::text \
                      || ' ready=' || x.indisready::text \
                      || ' live=' || x.indislive::text \
             FROM pg_index x \
             JOIN pg_class i ON i.oid = x.indexrelid \
             JOIN pg_class c ON c.oid = x.indrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname IN ({tables}) AND n.nspname = $1 \
             UNION ALL \
             SELECT r.rulename::text || ' on ' || r.tablename::text, 'rule ' || r.definition \
             FROM pg_rules r \
             WHERE r.schemaname = $1 \
               AND r.tablename IN ({tables}) \
             UNION ALL \
             SELECT c.relname::text || ' row security', \
                    'setting on=' || c.relrowsecurity::text \
                      || ' forced=' || c.relforcerowsecurity::text \
                      || ' persistence=' || c.relpersistence::text \
                      || ' writers=' \
                      || (SELECT count(*)::text \
                          FROM aclexplode(COALESCE(c.relacl, \
                                                   acldefault('r', c.relowner))) a \
                          WHERE a.privilege_type IN ('UPDATE', 'DELETE', 'TRUNCATE') \
                            AND a.grantee <> c.relowner) \
                      || ' colwriters=' \
                      || (SELECT count(*)::text \
                          FROM pg_attribute at \
                          CROSS JOIN LATERAL aclexplode(at.attacl) aa \
                          WHERE at.attrelid = c.oid \
                            AND at.attacl IS NOT NULL \
                            AND aa.privilege_type = 'UPDATE' \
                            AND aa.grantee <> c.relowner) \
                      || ' writeall=' \
                      || (SELECT count(*)::text \
                          FROM pg_auth_members m \
                          JOIN pg_roles g ON g.oid = m.roleid \
                          WHERE g.rolname = 'pg_write_all_data') \
                      || ' owners=' \
                      || (SELECT count(*)::text \
                          FROM pg_auth_members m \
                          WHERE m.roleid = c.relowner AND m.member <> c.relowner) \
                      || ' superusers=' \
                      || (SELECT count(*)::text \
                          FROM pg_roles \
                          WHERE rolsuper AND rolname <> current_user) \
                      || ' replident=' || c.relreplident::text \
                      || ' options=' \
                      || COALESCE(array_to_string(c.reloptions, ','), 'none') \
                      || ' hastriggers=' || c.relhastriggers::text \
                      || ' checks=' || c.relchecks::text \
                      || ' hasindex=' || c.relhasindex::text \
                      || ' hasrules=' || c.relhasrules::text \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname IN ({tables}) AND n.nspname = $1 \
             UNION ALL \
             SELECT c.relname::text || ' inheritance', \
                    'children=' || (SELECT count(*) FROM pg_inherits WHERE inhparent = c.oid)::text \
                      || ' parents=' \
                      || (SELECT count(*) FROM pg_inherits WHERE inhrelid = c.oid)::text \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname IN ({tables}) AND n.nspname = $1 \
             UNION ALL \
             SELECT c.relname::text || '.' || a.attname::text || ' default', \
                    'default ' || pg_catalog.pg_get_expr(d.adbin, d.adrelid) \
             FROM pg_attrdef d \
             JOIN pg_class c ON c.oid = d.adrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum \
             WHERE c.relname IN ({tables}) AND n.nspname = $1 \
             UNION ALL \
             SELECT c.relname::text || ' statistics', \
                    'statistics=' || count(e.oid)::text \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_statistic_ext e ON e.stxrelid = c.oid \
             WHERE c.relname IN ({tables}) AND n.nspname = $1 \
             GROUP BY c.relname \
             UNION ALL \
             SELECT s.relname::text, \
                    'sequence increment=' || q.seqincrement::text \
                      || ' min=' || q.seqmin::text || ' max=' || q.seqmax::text \
                      || ' start=' || q.seqstart::text || ' cycle=' || q.seqcycle::text \
                      || ' cache=' || q.seqcache::text \
             FROM pg_sequence q \
             JOIN pg_class s ON s.oid = q.seqrelid \
             JOIN pg_namespace n ON n.oid = s.relnamespace \
             WHERE n.nspname = $1 \
               AND EXISTS (SELECT 1 FROM pg_depend d \
                           JOIN pg_class t ON t.oid = d.refobjid \
                           WHERE d.objid = s.oid AND d.deptype = 'a' \
                             AND t.relname IN ({tables})) \
             UNION ALL \
             SELECT 'access_records.recorded', \
                    'ordering distinct=' \
                      || (count(*) = count(DISTINCT recorded))::text \
                      || ' ahead=' \
                      || COALESCE(pg_sequence_last_value( \
                           pg_get_serial_sequence('access_records', \
                                                  'recorded')::regclass) \
                           >= max(recorded), true)::text \
             FROM access_records \
             ORDER BY 1";

/// A schema name, quoted so it cannot be read as more than one identifier.
///
/// PostgreSQL doubles an embedded quote, and nothing else needs escaping inside
/// a quoted identifier. Used where the name has to reach a statement that takes
/// no bind parameter — `SET search_path` is one.
pub(crate) fn quoted_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod quoting_tests {
    /// A quote inside a name is doubled, so the name cannot be split.
    ///
    /// **Decided by no test.** The live path only ever passes `public`, so the
    /// gate can never notice — measured on 2026-09-08, the doubling removed:
    /// `cargo test -p xtask --bin xtask` green at 481 with the stack up. What it
    /// buys is that a schema whose name holds a quote is one identifier to
    /// PostgreSQL rather than two, which is the difference between a name and an
    /// injection.
    #[test]
    fn a_quote_inside_a_name_is_doubled() {
        assert_eq!(super::quoted_identifier("public"), "\"public\"");
        assert_eq!(super::quoted_identifier("nc test"), "\"nc test\"");
        assert_eq!(
            super::quoted_identifier("a\"b"),
            "\"a\"\"b\"",
            "a quote inside the name closed it, so what follows is read as SQL"
        );
        // Two quotes, and a quote at each end: the doubling is of every one.
        assert_eq!(super::quoted_identifier("a\"b\"c"), "\"a\"\"b\"\"c\"");
        assert_eq!(super::quoted_identifier("\"lead"), "\"\"\"lead\"");
        assert_eq!(super::quoted_identifier("trail\""), "\"trail\"\"\"");
        // And an empty name is still one identifier, not nothing.
        assert_eq!(super::quoted_identifier(""), "\"\"");
    }
}

/// The journal's own protections, as the live database holds them today.
///
/// **`check` corroborated that the migrations were *applied* and never that what
/// they created is still in force.** Nothing in `xtask/` read `pg_trigger` at
/// all — measured, `grep -rn pg_trigger xtask/src/` was empty — and a table's
/// owner can always disable its own triggers, which the role this stack ships
/// with is. One statement:
///
/// ```text
/// ALTER TABLE journal_entries DISABLE TRIGGER USER;
/// UPDATE journal_entries SET action='Escalate';   -- accepted
///
/// cargo xtask check   ->   check passed
/// ```
///
/// The row read `Escalate` where it had read `Read`, every append-only trigger
/// was `D`, and the gate had no opinion. That is acceptance criterion 3 and the
/// reason `the plan's ordering rule` puts the journal before anything that can write to it: audit
/// cannot be retrofitted, and a guarantee nobody checks is one that has already
/// stopped holding somewhere.
///
/// The state is `tgenabled` and `tgtype` together, because neither alone is the
/// guarantee. `O` — the ordinary enabled state — is not enough: `session_replication_role = replica` turns an
/// `O` trigger off for the session, which is why the migrations declare these
/// `ENABLE ALWAYS`. Only `A` survives that.
///
/// # Errors
///
/// Returns why the database could not be asked.
pub(crate) fn journal_protections(url: &str) -> Result<Vec<(String, String)>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime to ask the database with: {error}"))?;
    let schema = configured_schema(url);
    runtime.block_on(async {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .map_err(|error| format!("the database could not be reached: {error}"))?;
        // **The path this connection resolves names by, pinned before it asks
        // anything.** Every catalog branch below is anchored `n.nspname = $1`;
        // the one branch that reads a *table* was not, and an unqualified name
        // resolves through `search_path`, which begins `"$user"`. So one
        // `CREATE SCHEMA` named after the connecting role — a schema that role
        // may create, and which `configured_schema` deliberately skips because
        // `$user` is not a schema name — put an empty copy of `access_records`
        // in front of the real one, and the gate read the copy: measured,
        // `ordering distinct=true` from the gate's own SQL and
        // `distinct=false` from `public.access_records` in the same instant,
        // with `check passed`.
        //
        // `nc-journal` and `nc-prune` both pin their connection's path for this
        // reason and say so; the gate that checks their tables did not.
        // `pg_temp` last, for the reason `NOCTUA.md §14.9` gives.
        //
        // **Built by the one function that writes this statement**, because it
        // was written here a second time and only the other copy had a test.
        // That test says in as many words that the path must not name
        // `pg_catalog` — naming it moves the system catalogue *behind* a schema
        // the connecting role owns — and this query reads fifteen catalogue
        // relations unqualified. Measured: the catalogue appended to the
        // argument rather than to the literal left `ok 44 protection(s) …` and
        // `check passed`, byte-identical to a clean run, because the sentence
        // census reads the format string and the format string had not moved.
        sqlx::query(sqlx::AssertSqlSafe(crate::dev::path_statement(&schema)))
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("the connection's search path could not be pinned: {error}")
            })?;
        // **The definition, not the name.** The first version of this asked
        // `pg_trigger` for `tgname` and `pg_constraint` for `conname` and
        // nothing else — so six independent tampers passed with `check passed`
        // and ten rows byte-identical to the expectation:
        //
        //   * the trigger *function body* replaced with `RETURN COALESCE(NEW,
        //     OLD)`, which the migrations' own paragraph documents as the attack
        //     and believed closed by a test that builds its own schema and never
        //     looks at the deployed one;
        //   * a constraint dropped and re-added under the same name as
        //     `CHECK (true)`;
        //   * an index dropped;
        //   * `CREATE RULE … DO INSTEAD NOTHING`, which discards every append
        //     silently;
        //   * row-level security switched on;
        //   * and a *fifth* trigger the list simply forgot.
        //
        // After those seven statements the journal accepted `UPDATE`, `DELETE`
        // and `TRUNCATE`, an honest append vanished, and the gate said
        // `10 protection(s) … every trigger ENABLE ALWAYS`.
        //
        // So each row now carries what the object *is*: a trigger its events,
        // its enabled state, the function it calls and a digest of that
        // function's body; a constraint and an index their rendered definition;
        // and the two table-level settings that turn the whole apparatus off.
        let query = asking_about_the_protected_tables(THE_PROTECTIONS);
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(&*query))
            .bind(&schema)
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("the journal's protections could not be read: {error}"))?;
        pool.close().await;
        Ok(rows)
    })
}

/// Every function this database holds, and whether its path ends in `pg_temp`.
///
/// **The half of the search-path rule that is not in the files.** PostgreSQL
/// looks in the temporary schema for a relation name *before* it consults
/// `search_path` at all, and `CREATE TEMP TABLE` is a default privilege — so a
/// trigger function reading an unqualified name is decided by whoever plants one
/// first. Measured against the journal, on the default path, with no DDL and no
/// trigger disabled:
///
/// ```text
/// CREATE TEMP TABLE journal_entries (sequence BIGINT);
/// INSERT INTO pg_temp.journal_entries VALUES (4611686018427387902);
/// INSERT INTO public.journal_entries (...) VALUES (4611686018427387903, 'attacker', ...);
/// INSERT 0 1
/// ```
///
/// A foreign row at the top of `0008`'s range, after which every honest append is
/// refused and `DELETE` and `TRUNCATE` are refused too — the outage `0007`,
/// `0008` and `0010` were each written to close. `0011_a_trigger_names_its_own_schema.sql` closes it.
///
/// **Listing `pg_temp` last is the fix, and an empty path is not.** With a real
/// table holding 7 and a temporary one of the same name holding 999:
///
/// ```text
/// search_path = ''                  + unqualified body  ->  999
/// search_path = probe2, pg_temp     + unqualified body  ->    7
/// ```
///
/// Left unlisted the temporary schema is searched first whatever the path says,
/// so `search_path = ''` — the recipe usually given for this — leaves the hole
/// wide open for relations. That is why this asks where `pg_temp` *is* rather
/// than whether a path was set: `limits::every_function_resolves_its_own_names`
/// reads the files and can only see that one was.
///
/// # Errors
///
/// Returns why the database could not be asked.
pub(crate) fn function_paths(url: &str) -> Result<Vec<(String, Option<String>, bool)>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime to ask the database with: {error}"))?;
    runtime.block_on(async {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .map_err(|error| format!("the database did not answer: {error}"))?;
        // **The schema the migration record is in, not `current_schema()`.** The
        // integration tests migrate into a schema of their own, so `public` is
        // wrong — and `current_schema()` is wrong in the way that matters: it is
        // the *first existing* schema on this connection's path, so one
        // `CREATE SCHEMA` named after the connecting role moved it, this query
        // returned nothing, and the step reported "nothing to corroborate" while
        // the journal it was guarding held a foreign row. One statement both
        // landed the row and switched off the gate that would have caught it.
        //
        // `_sqlx_migrations` is where the migrations actually went, and
        // `to_regclass` finds it by the same path resolution the migrator used.
        //
        // **Every language, not `plpgsql` alone.** A `LANGUAGE sql` function
        // resolves a relation name through `search_path` exactly as a plpgsql one
        // does, and the filter made it invisible here — while the static half
        // could not bracket its bodyless definition either, so both halves of the
        // gate missed the same shape at once.
        // **Pinned before anything is asked.** `to_regclass` and every other
        // unqualified name below resolve through *this connection's*
        // `search_path`, which is the identical re-expansion the twelfth
        // migration exists to remove — one layer over. Measured: two ordinary
        // statements as the role this repository connects as —
        //
        // ```text
        // CREATE SCHEMA noctua;
        // CREATE TABLE noctua._sqlx_migrations AS SELECT * FROM public._sqlx_migrations;
        // ```
        //
        // — moved `to_regclass` onto the copy, and a single decoy function in
        // that schema then answered for the whole gate: `ok 1 migrated
        // function(s) search pg_temp last`, while the three real ones — one of
        // them freshly unpinned, having just admitted a foreign row at the top of
        // the journal's range — were never inspected. The same two statements
        // redirect the applied-migration record, so the checksum corroboration
        // that makes an applied migration un-rewritable goes with it.
        //
        // `pg_catalog` and nothing else: every name this function uses is a
        // catalog one, and `pg_temp` is listed last for the reason the migrations
        // give — left out, it is searched first.
        // **Read from the URL, because asking the connection asks `$user`.**
        // `(current_schemas(false))[1]` is `current_schema()` spelled differently
        // — both return the first *existing* schema on the path and both expand
        // `"$user"` — so the fix that replaced one with the other changed
        // nothing. Measured: with a schema named after the connecting role
        // holding three decoy functions and the two real triggers in `public`
        // unpinned, this gate reported `ok 3 migrated function(s) search pg_temp
        // last` over a journal that had just taken a foreign row at the top of
        // its range.
        //
        // The schema a deployment means is the one it wrote down, and it wrote it
        // in the connection string. Nothing here asks the database to guess.
        //
        // The earlier version picked the lowest-oid `_sqlx_migrations`, which
        // chose arbitrarily among whatever schemas existed — a developer's
        // database holding 139 leftover test schemas had this reporting two
        // unpinned functions out of one of *those*. Neither guess is the answer;
        // the string is.
        let intended = configured_schema(url);
        sqlx::query("SET search_path = pg_catalog, pg_temp")
            .execute(&pool)
            .await
            .map_err(|error| format!("the search path could not be pinned: {error}"))?;
        // **And whether every schema the path names actually exists.** A path is
        // a *spelling* until something looks: PostgreSQL skips a schema that is
        // not there, silently, so `audit_vault, pg_temp` where `audit_vault` does
        // not exist resolves at `pg_temp` exactly as an empty path does — the
        // very case the migrations call "the recipe usually given, which leaves
        // the hole wide open". Measured, a migration pinning the journal's
        // trigger to a schema a later migration was going to create: both halves
        // of the gate green, and one `CREATE TEMP TABLE journal_entries` from an
        // ordinary session put a foreign row at the top of the range. No attacker
        // is needed — a typo, or two migrations written out of order, does it.
        //
        // **And `USAGE`, because PostgreSQL ignores a schema the role cannot
        // enter exactly as it ignores one that is absent.** Measured on 18.6, the
        // same function pinned to a real schema holding the real table:
        //
        // ```text
        // as the owner                       ->      5   (the real table)
        // as a role without USAGE on it      -> 424242   (the planted temp table)
        // ```
        //
        // `EXISTS (… pg_namespace)` answered `true` in both. What the rule is
        // about is whether the path *reaches* the relation, and existence is only
        // half of that.
        //
        // **And a schema that holds no table reaches nothing either.**
        // `pg_catalog, pg_temp` exists, grants `USAGE`, has `pg_temp` last and
        // something real in front — and holds not one user table, so every
        // unqualified name in a body pinned to it resolves at `pg_temp` exactly
        // as an empty path does. Measured: the journal's density trigger pinned
        // that way, a `CREATE TEMP TABLE journal_entries`, and a foreign row at
        // the top of the range with both halves of this gate reporting `ok`.
        // `information_schema, pg_temp` behaves identically.
        //
        // A path element earns its place by holding an ordinary table. That is
        // the weakest question that distinguishes a schema the tables are in from
        // one they are not.
        let rows: Vec<(String, Option<Vec<String>>, bool)> = sqlx::query_as(
            "SELECT p.proname, p.proconfig, COALESCE(( \
                 SELECT bool_and( \
                     btrim(btrim(part), '\"') = 'pg_temp' OR EXISTS ( \
                         SELECT 1 FROM pg_namespace n \
                         WHERE n.nspname = btrim(btrim(part), '\"') \
                           AND pg_catalog.has_schema_privilege( \
                               current_user, n.oid, 'USAGE') \
                           AND EXISTS (SELECT 1 FROM pg_class t \
                                       WHERE t.relnamespace = n.oid \
                                         AND t.relkind = 'r'))) \
                 FROM unnest(string_to_array(( \
                     SELECT substring(setting from position('=' in setting) + 1) \
                     FROM unnest(p.proconfig) AS setting \
                     WHERE setting LIKE 'search\\_path=%' LIMIT 1), \
                     ',')) AS part \
                 WHERE btrim(part) <> ''), true) AS every_schema_exists \
             FROM pg_proc p \
             WHERE p.pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = $1) \
               AND p.prokind IN ('f', 'p')",
        )
        .bind(intended)
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("the functions could not be listed: {error}"))?;
        pool.close().await;
        Ok(rows
            .into_iter()
            .map(|(name, config, exists)| {
                let path = config.unwrap_or_default().into_iter().find_map(|setting| {
                    setting
                        .strip_prefix("search_path=")
                        .map(std::borrow::ToOwned::to_owned)
                });
                (name, path, exists)
            })
            .collect())
    })
}

#[cfg(test)]
mod tests {
    use super::{host_and_port, redacted};

    #[test]
    fn a_url_yields_its_host_and_port() {
        assert_eq!(
            host_and_port("postgres://noctua:noctua@127.0.0.1:5432/noctua"),
            Some(("127.0.0.1".to_owned(), 5432))
        );
        // A password holding an `@`, which is why the split is from the right.
        assert_eq!(
            host_and_port("postgres://user:p@ss@db.local:5433/x"),
            Some(("db.local".to_owned(), 5433))
        );
        // And with no credentials at all.
        assert_eq!(
            host_and_port("postgres://localhost:5432/x"),
            Some(("localhost".to_owned(), 5432))
        );
    }

    /// Every list of tables in a query is the one constant, spliced in.
    ///
    /// **Ten `IN (…)` literals became one name, and a splice that silently does
    /// not happen is a syntax error no offline run can see.** The two statements
    /// here are `String`s by the time sqlx reads them; if the placeholder in
    /// them and [`super::THE_PROTECTED_TABLES`] ever stop being the same text,
    /// `asking_about_the_protected_tables` hands back the template unchanged and
    /// every one of these queries fails at the server — on a machine with a
    /// database. This is the reader that needs none.
    ///
    /// The count is here for the same reason [`super::PROTECTED`] is a constant:
    /// a list deleted from one of the two statements takes a whole class of
    /// protection out of the gate, and the number is what makes that a diff.
    #[test]
    fn every_query_asks_about_the_tables_the_constant_names() {
        let mut lists = 0_usize;
        for (named, sql) in [
            ("THE_PROTECTIONS", super::THE_PROTECTIONS),
            ("THE_SHAPES", super::THE_SHAPES),
        ] {
            lists += sql.matches(super::THE_PROTECTED_TABLES).count();
            let asked = super::asking_about_the_protected_tables(sql);
            assert!(
                !asked.contains(super::THE_PROTECTED_TABLES),
                "`{named}` still holds the placeholder after the splice, so it reaches \
                 PostgreSQL as a statement it cannot parse"
            );
            for table in super::PROTECTED {
                assert!(
                    asked.contains(&format!("'{table}'")),
                    "`{named}` does not ask about `{table}`, which `PROTECTED` names"
                );
            }
        }
        assert_eq!(
            lists, 10,
            "the two statements hold {lists} table list(s) and there were ten: a list \
             removed is an inspection the gate no longer makes"
        );
    }

    #[test]
    fn something_that_is_not_a_postgres_url_yields_nothing() {
        assert_eq!(host_and_port("http://127.0.0.1:5432"), None);
        assert_eq!(host_and_port("postgres://127.0.0.1/x"), None, "no port");
        assert_eq!(host_and_port("postgres://127.0.0.1:nope/x"), None);
    }

    /// The `row security` row the shipped query reports for the scratch table.
    async fn measured(pool: &sqlx::PgPool, schema: &str) -> String {
        let query = super::asking_about_the_protected_tables(super::THE_PROTECTIONS);
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(&*query))
            .bind(schema)
            .fetch_all(pool)
            .await
            .expect("the protections query runs");
        rows.into_iter()
            .find(|(subject, _)| subject == "access_records row security")
            .expect("the scratch table is measured")
            .1
    }

    /// The two doors a role opens, and the one that cannot be given a control.
    ///
    /// Its own function because the test it belongs to crossed a hundred lines
    /// with it inline.
    async fn the_role_doors_move(pool: &sqlx::PgPool, schema: &str) {
        // **The other two doors, which were watched at their passing value
        // alone.** `writeall=` was asserted `0` before any grant and never
        // moved, so one character — `pg_write_all_data` misspelled
        // `pg_write_all_dat` — left `cargo xtask check` byte-identical to
        // baseline. `owners=` is the shortest door of the four and was not
        // counted at all until a reviewer used it: `GRANT <owner> TO <an
        // ordinary login>` moves no other number, and that role can disable
        // the triggers and rewrite the audit log.
        //
        // The role is named for this process, because a role is
        // cluster-global, and it is dropped on the way out of this block.
        let role = format!("nc_writers_probe_{}", std::process::id());
        let named = super::quoted_identifier(&role);
        let owner: String = sqlx::query_scalar("SELECT current_user")
            .fetch_one(pool)
            .await
            .expect("the connection has a role");
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE IF EXISTS {named}")))
            .execute(pool)
            .await
            .expect("no role of this name is left over");
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE ROLE {named} NOLOGIN")))
            .execute(pool)
            .await
            .expect("a role of this test's own");
        for (grant, expected, door) in [
            (
                format!("GRANT pg_write_all_data TO {named}"),
                "writeall=1",
                "writeall",
            ),
            (
                format!("GRANT {} TO {named}", super::quoted_identifier(&owner)),
                "owners=1",
                "owners",
            ),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(&*grant))
                .execute(pool)
                .await
                .expect("the grant lands");
            let after = measured(pool, schema).await;
            assert!(
                after.contains(expected),
                "`{grant}` is a role that can write this table and `{door}=` did not \
                 move: {after}"
            );
        }
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE IF EXISTS {named}")))
            .execute(pool)
            .await
            .expect("the role is this test's to remove");

        // `superusers=` cannot be given a positive control safely — making
        // one, even for a moment, is a cluster-global privilege this test
        // has no business creating. It gets a second reader instead: the
        // same population, counted by a query that shares no text with the
        // one under test.
        let superusers: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_roles WHERE rolsuper")
            .fetch_one(pool)
            .await
            .expect("the catalog answers");
        let others = superusers - 1;
        let last = measured(pool, schema).await;
        assert!(
            last.contains(&format!("superusers={others}")),
            "the row says something other than the {others} superuser(s) besides \
             `{owner}` that `pg_roles` reports: {last}"
        );
    }

    /// The four write doors are counted, and each one moves its own number.
    ///
    /// **The passing value is `0`, so nothing was testing the clause.** Every
    /// number in the `row security` row reads zero on a correct database, which
    /// means re-adding `AND a.grantee <> 0` — the filter that hid every grant to
    /// `PUBLIC` — left the whole suite green. A measurement whose only observed
    /// value is the passing one is a measurement nobody has watched work.
    ///
    /// Two of the four were also *wrong*, measured on a live stack before this
    /// existed: `GRANT UPDATE (outcome, masked) ON access_records TO PUBLIC`
    /// leaves `relacl` NULL and lands in `pg_attribute.attacl`, and
    /// `GRANT pg_write_all_data TO <role>` is a membership rather than an ACL —
    /// a non-owner then ran `UPDATE` and `DELETE` against the audit log, which
    /// has no append-only trigger, with `writers=0` and `check passed`.
    ///
    /// Run against a scratch schema of this test's own, never `public`: the
    /// grants below are real grants, and the developer's tables are not the
    /// place to rehearse them. Skipped, announced, when no database answers —
    /// the rule this module's header states.
    #[test]
    fn a_grant_this_query_is_written_for_moves_its_own_number() {
        // **`--offline` is the flag, and `url()` does not read it.** `url()`
        // falls back to `postgres://…@127.0.0.1:${POSTGRES_PORT:-5432}/noctua`
        // and probes the port, so under the environment `cargo xtask check
        // --offline` hands the test binary — `DATABASE_URL` removed, `NC_OFFLINE`
        // set — this connected anyway and issued `CREATE SCHEMA`, two `GRANT`s
        // and `DROP SCHEMA … CASCADE` against whatever was listening, which by
        // default is the developer's own stack. Measured:
        // `env -u DATABASE_URL NC_OFFLINE=1 POSTGRES_DB=nc_no_such_db` panicked
        // with SQLSTATE `3D000`, which is a real connection.
        //
        // `nc-journal`'s and `nc-prune`'s integration suites both read
        // `NC_OFFLINE`; this did not.
        if std::env::var_os("NC_OFFLINE").is_some() {
            // **And it says so.** A control that returns in silence is counted
            // by cargo as a test that passed, and `check --offline` printed its
            // usual total with this one never run — the same hazard the three
            // suites in `ANNOUNCES_NO_DATABASE` announce for. Reported on
            // 2026-09-14 by a reviewer reading what stands aside without a word.
            println!("xtask database: skipped, no database (NC_OFFLINE)");
            return;
        }
        // **And no database is a failure, not a skip.** This module's header
        // calls a silent skip "the `NOCTUA.md §19.3` hazard in its purest form",
        // and `eprintln!` is captured by libtest — so on a machine with no stack
        // the control that proves the five write doors are watched did not run,
        // the printed total stayed at its usual number, and it was not one of
        // the announced `--offline` skips either. Every other database test in
        // this tree fails rather than skips.
        let url = super::url().unwrap_or_else(|why| {
            panic!(
                "the write doors could not be measured: {why}. This is the control that \
                 proves `writers=`, `colwriters=`, `writeall=`, `owners=` and \
                 `superusers=` are watched at all — a skip here is the gate reporting \
                 that it checked them"
            )
        });
        let schema = format!("nc_writers_probe_{}", std::process::id());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect(&url)
                .await
                .expect("the database answers");
            let quoted = super::quoted_identifier(&schema);
            for statement in [
                format!("DROP SCHEMA IF EXISTS {quoted} CASCADE"),
                format!("CREATE SCHEMA {quoted}"),
                format!(
                    "CREATE TABLE {quoted}.access_records \
                     (id bigint PRIMARY KEY, outcome text, masked boolean)"
                ),
            ] {
                sqlx::query(sqlx::AssertSqlSafe(&*statement))
                    .execute(&pool)
                    .await
                    .expect("the scratch schema is this test's own");
            }

            // **Every leftover probe role, not this pid's.** A role is cluster-global
            // and `writeall=`/`owners=` are cluster-wide counts folded into the row
            // `PROTECTIONS` pins — so one red run that left a role behind made
            // `cargo xtask check` fail *permanently*, with four errors naming the
            // migrations and none naming a role. Measured: a weakening fired this
            // control, the role survived, and the reverted tree then reported
            // `access_records row security … writeall=1 … nothing here declares it`.
            // A sweep that names only the running pid can never clear it.
            for stale in sqlx::query_scalar::<_, String>(
                "SELECT rolname FROM pg_roles WHERE rolname LIKE 'nc_writers_probe_%'",
            )
            .fetch_all(&pool)
            .await
            .expect("the catalog answers")
            {
                let stale = super::quoted_identifier(&stale);
                sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE IF EXISTS {stale}")))
                    .execute(&pool)
                    .await
                    .expect("a probe role is this test's to remove");
            }

            let before = measured(&pool, &schema).await;
            assert!(
                before.contains("writers=0")
                    && before.contains("colwriters=0")
                    && before.contains("writeall=0"),
                "a table nobody has been granted anything on reads: {before}"
            );

            // The column-level grant, which `relacl` cannot see.
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "GRANT UPDATE (outcome, masked) ON {quoted}.access_records TO PUBLIC"
            )))
            .execute(&pool)
            .await
            .expect("the grant lands");
            let column = measured(&pool, &schema).await;
            assert!(
                column.contains("writers=0") && column.contains("colwriters=2"),
                "a column-level UPDATE to PUBLIC is two column privileges and no table \
                 ACL at all, and this is what the query said: {column}"
            );

            // **And the grant this must *not* count, which is the one it used
            // to.** A role that may insert is the ordinary case — `writers=`
            // omits `INSERT` for exactly that reason — so counting it at the
            // column level made the narrower, least-privilege spelling the
            // refused one: measured on a live stack, `GRANT INSERT ON
            // journal_entries` left the run identical to a clean one and
            // `GRANT INSERT (…six columns…) ON access_records` failed it with
            // `colwriters=6`.
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "GRANT INSERT (outcome, masked) ON {quoted}.access_records TO PUBLIC"
            )))
            .execute(&pool)
            .await
            .expect("the grant lands");
            let inserting = measured(&pool, &schema).await;
            assert!(
                inserting.contains("colwriters=2"),
                "a column-level INSERT moved the write-door count, so the narrower of two \
                 ways to say the same thing is the one this gate refuses: {inserting}"
            );

            // And the table-level one, which it can.
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "GRANT DELETE ON {quoted}.access_records TO PUBLIC"
            )))
            .execute(&pool)
            .await
            .expect("the grant lands");
            let table = measured(&pool, &schema).await;
            assert!(
                table.contains("writers=1"),
                "a DELETE to PUBLIC is grantee 0, the case `AND a.grantee <> 0` used to \
                 filter out: {table}"
            );

            the_role_doors_move(&pool, &schema).await;

            sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {quoted} CASCADE")))
                .execute(&pool)
                .await
                .expect("the scratch schema is removed");
        });
    }

    /// A sequence rewound under the ordering column is a finding.
    ///
    /// **`distinct=` is uniqueness, and uniqueness is not order.** The six
    /// `pg_sequence` columns beside it are the sequence's *declaration*, and
    /// `ALTER SEQUENCE … RESTART` writes none of them — so one statement and two
    /// ordinary inserts left every value in the column distinct, every pinned
    /// column identical, `ok 44 protection(s) in force` and `check passed`, with
    /// the two newest rows carrying the two lowest values. `nc-prune` takes the
    /// oldest by that column, so it would have taken them.
    ///
    /// It is measured here rather than against the developer's own table because
    /// that one is empty on a fresh stack: with no rows there is no largest
    /// value, the fact is vacuously true, and a test that ran there would pass
    /// whatever the query said.
    #[test]
    fn a_rewound_sequence_under_the_ordering_column_is_read() {
        if std::env::var_os("NC_OFFLINE").is_some() {
            println!("xtask database: skipped, no database (NC_OFFLINE)");
            return;
        }
        let url = super::url().unwrap_or_else(|why| {
            panic!(
                "the ordering column could not be measured: {why}. A skip here is the \
                 gate reporting that it checked what the pruner takes"
            )
        });
        let schema = format!("nc_rewound_probe_{}", std::process::id());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let options: sqlx::postgres::PgConnectOptions = url.parse().expect("a valid url");
            let admin = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_with(options.clone())
                .await
                .expect("the database answers");
            let quoted = super::quoted_identifier(&schema);
            for statement in [
                format!("DROP SCHEMA IF EXISTS {quoted} CASCADE"),
                format!("CREATE SCHEMA {quoted}"),
                // The shape the migrations give it: the column is the database's
                // to write, from a sequence of its own.
                format!(
                    "CREATE TABLE {quoted}.access_records (recorded bigint GENERATED BY \
                     DEFAULT AS IDENTITY PRIMARY KEY, id bigint, outcome text, masked boolean)"
                ),
                format!(
                    "INSERT INTO {quoted}.access_records (outcome) \
                     VALUES ('a'), ('b'), ('c'), ('d'), ('e')"
                ),
                // The pruner has been through: the oldest are gone, which is the
                // ordinary state of this table and not a violation of anything.
                format!("DELETE FROM {quoted}.access_records WHERE recorded < 4"),
            ] {
                sqlx::query(sqlx::AssertSqlSafe(&*statement))
                    .execute(&admin)
                    .await
                    .expect("the scratch schema is this test's own");
            }
            // The ordering branch reads the table through the path, the way the
            // half beside it does, so the connection that asks pins the path.
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_with(options.options([("search_path", schema.as_str())]))
                .await
                .expect("the database answers");
            let honest = the_ordering_row(&pool, &schema).await;
            assert_eq!(
                honest, "ordering distinct=true ahead=true",
                "a table nothing has been done to reads: {honest}"
            );

            // One statement, and then two writes that are in every other way
            // ordinary.
            for statement in [
                format!("ALTER SEQUENCE {quoted}.access_records_recorded_seq RESTART WITH 1"),
                format!("INSERT INTO {quoted}.access_records (outcome) VALUES ('f'), ('g')"),
            ] {
                sqlx::query(sqlx::AssertSqlSafe(&*statement))
                    .execute(&admin)
                    .await
                    .expect("the scratch schema is this test's own");
            }
            let rewound = the_ordering_row(&pool, &schema).await;
            assert_eq!(
                rewound, "ordering distinct=true ahead=false",
                "the two newest rows carry the two lowest values and every value is \
                 still distinct, so `distinct=` cannot see it and this is what the \
                 query said: {rewound}"
            );

            pool.close().await;
            sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {quoted} CASCADE")))
                .execute(&admin)
                .await
                .expect("the scratch schema is removed");
        });
    }

    /// The `access_records.recorded` row the shipped query reports.
    async fn the_ordering_row(pool: &sqlx::PgPool, schema: &str) -> String {
        let query = super::asking_about_the_protected_tables(super::THE_PROTECTIONS);
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(&*query))
            .bind(schema)
            .fetch_all(pool)
            .await
            .expect("the protections query runs");
        rows.into_iter()
            .find(|(subject, _)| subject == "access_records.recorded")
            .expect("the ordering column is measured")
            .1
    }

    #[test]
    fn a_password_never_reaches_the_message() {
        let shown = redacted("postgres://user:hunter2@db.local:5433/x");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert_eq!(shown, "db.local:5433");
    }
}

#[cfg(test)]
mod the_schema_comes_from_the_string {
    use super::configured_schema;

    /// The schema is read the way sqlx reads it, encoding and all.
    ///
    /// **`configured_schema` had no test, and everything now depends on it.**
    /// It did `strip_prefix` on the raw URL while sqlx builds its connection from
    /// `query_pairs()`, which percent-decodes — so the canonical spelling of this
    /// option connected to one schema and was read as another, and the whole
    /// search-path gate reported `ok` over a journal in a schema it never looked
    /// at. The `the_frozen_path_rule` module's own doc says `pg_temp_is_last`
    /// "had no test at all"; this function was added the round after and had
    /// none either.
    #[test]
    fn every_spelling_of_the_option_reads_the_same_schema() {
        let base = "postgres://u:p@h:5432/d";
        for (query, schema) in [
            // The three spellings libpq and sqlx all accept.
            ("?options=-csearch_path=app", "app"),
            ("?options=-csearch_path%3Dapp", "app"),
            ("?options=-c%20search_path%3Dapp", "app"),
            // A schema whose name holds the two characters the old split cut on.
            ("?options=-c%20search_path%3Dmy-cool", "my-cool"),
            // Quoted, and with the temporary schema written out.
            ("?options=-csearch_path%3D%22Prod%22,pg_temp", "Prod"),
            // `$user` is not a schema, so the next real element is the answer.
            ("?options=-csearch_path%3D%24user,app", "app"),
            // Beside other options, in either order.
            (
                "?options=-c%20lock_timeout%3D5s%20-c%20search_path%3Dapp",
                "app",
            ),
            ("?application_name=x&options=-csearch_path%3Dapp", "app"),
            // Nothing said: the migrations create their tables in `public`.
            ("", "public"),
            ("?application_name=x", "public"),
            ("?options=-clock_timeout%3D5s", "public"),
        ] {
            assert_eq!(
                configured_schema(&format!("{base}{query}")),
                schema,
                "`{query}`"
            );
        }
    }
}
