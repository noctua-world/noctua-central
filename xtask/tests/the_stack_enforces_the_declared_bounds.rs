//! `docker-compose.yml` is what enforces the log store's declared bounds.
//!
//! **The catalog named an enforcer and nothing checked the thing it named.**
//! `catalog/limits.toml` gives `central_log` a thirty-day retention and eight
//! gibibytes, and says by name that the compose file's flags are what apply
//! them. `docs/limits.md` publishes both numbers, and a test compares the page
//! to the catalog. Measured: setting `--retentionPeriod=3d` and
//! `--retention.maxDiskSpaceUsageBytes=80GiB` left `cargo xtask check` green
//! while the catalog, the page and the generated code all continued to say
//! thirty days and eight gibibytes.
//!
//! The compose file's own comment claims "the two cannot drift into a limit
//! nothing applies". That claim is what this file makes true.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).to_path_buf()
}

/// Every healthcheck in the resolved stack runs a command that asks something.
///
/// Its own function because the test that calls it is at clippy's line limit —
/// the same reason its neighbours are functions.
///
/// Read from the projection docker prints, for the same reason the ports are:
/// two spellings got past the file rule, which counts the `test:` key.
fn every_healthcheck_asks_something(resolved: &str) {
    //
    // * `test: ["NONE"]` — docker's other way of saying *no healthcheck*. The
    //   file still holds a `test:` line, so the count held and `cargo xtask
    //   check` was **byte-identical to a clean run**; only `cargo xtask dev`
    //   noticed, with `logs is running and declares no healthcheck`.
    // * one `# ` in front of the folded scalar's first line. Inside a `>-`
    //   block a `#` is content, and folding joins the lines with spaces — so
    //   the whole command becomes one shell comment. Measured:
    //   `docker exec <pg> sh -c '# PGPASSWORD=… psql …'` exits 0 while the real
    //   command exits 2, and the file test strips every line whose
    //   `trim_start()` opens with `#` — which is exactly the line it needs to
    //   read. All eight needles survived.
    //
    // The resolved projection has neither problem: docker prints the command it
    // will actually run.
    let mut healthchecks = 0_usize;
    let mut inside_test = false;
    let mut opened_test_at = 0_usize;
    for line in resolved.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        // **`disable` is judged here rather than in the file's needle census.**
        // That census counts `disable:` in the raw text, and YAML permits
        // whitespace before a colon — `disable : true` keeps the count at zero
        // while docker normalises it. Measured, one such line in postgres's
        // healthcheck: all six tests here passed, `cargo xtask check` was
        // **byte-identical** to a clean run, and
        // `docker inspect --format '{{json .Config.Healthcheck.Test}}'` answered
        // `["NONE"]` with `State.Health` null — PostgreSQL running with no
        // healthcheck at all. Docker has already normalised the spelling in the
        // projection, which is why the question is asked of it.
        assert!(
            !trimmed.starts_with("disable:") || trimmed.ends_with("false"),
            "a healthcheck is disabled in the resolved stack, so `cargo xtask dev` waits \
             on a service that was never asked whether it was ready: {trimmed}"
        );
        if trimmed.starts_with("test:") {
            healthchecks += 1;
            inside_test = true;
            opened_test_at = indent;
        } else if inside_test && !trimmed.is_empty() && indent <= opened_test_at {
            inside_test = false;
        }
        if inside_test || trimmed.starts_with("test:") {
            let command = trimmed.trim_start_matches("test:").trim();
            let command = command.trim_matches(['-', '[', ']', '"', '\'', ' ']);
            // **The first word, and any shell comment — not the first
            // character.** `true # <the real command>` begins with neither `#`
            // nor exactly `true`, so all three clauses passed: measured, two
            // words added to the folded scalar left all six tests `ok` and
            // `check passed`, while in the container
            // `sh -c 'true # PGPASSWORD=… psql …'` exits 0 and the same command
            // without them exits 2. PostgreSQL then reports healthy the instant
            // it starts, which is the race the `psql -h` form exists to prevent.
            let first = command.split_whitespace().next().unwrap_or_default();
            assert!(
                !command.contains(" #") && !first.starts_with('#'),
                "a healthcheck's command carries a shell comment, so everything after it \
                 is never run: {trimmed}"
            );
            assert!(
                !command.starts_with('#'),
                "a healthcheck's command begins with `#`, so the shell runs a comment and \
             the service reports healthy having been asked nothing: {trimmed}"
            );
            // Docker writes the list one entry per line, so `NONE` arrives
            // under the key rather than on it.
            // **A shell no-op asks nothing either.** `["CMD-SHELL", "true",
            // "<the real command>"]` satisfies every needle in the file census
            // — the command is right there in the file — while `sh -c true
            // '<psql>'` exits 0 having run `true` with the psql call as an
            // argument. Measured in the container: the no-op exits 0, the real
            // command exits 2, and `cargo xtask dev` printed
            // `ok 2 service(s) healthy`.
            assert!(
                !matches!(first, "true" | ":" | "/bin/true"),
                "a healthcheck's command is a shell no-op, so the service reports healthy \
                 having been asked nothing: {trimmed}"
            );
            assert!(
                command.trim() != "NONE",
                "a service's healthcheck is `NONE`, which is docker's spelling for none \
             at all — `cargo xtask dev` then waits on a service that was never \
             asked whether it was ready"
            );
        }
    }
    assert!(
        healthchecks >= 2,
        "{healthchecks} healthcheck(s) are in the resolved stack and it has two services, \
     so one of them reports healthy the instant it starts"
    );
}

/// The `test:` entries of each healthcheck this stack declares.
///
/// **A denylist of shell no-ops is unbounded, and this is what bounds it.**
/// [`every_healthcheck_asks_something`] refuses `true`, `:` and `/bin/true` by
/// the command's first word, and measured, `exit 0;` folded onto the front of
/// postgres's scalar left all six tests in this file `ok`. In the container
/// `sh -c 'exit 0; <the psql call>'` exits 0 while the same command without the
/// prefix exits 2, so PostgreSQL reports healthy the instant it starts — the
/// race the `psql -h` form exists to prevent. `:`, `test 1`, `[ 1 ]`, a leading
/// comment, `sh -c true` and `/bin/echo x` are that same defect written five
/// more ways, and nobody can write the end of that list.
///
/// The bounded question is the other one: this stack runs two healthchecks,
/// each of them the subject of a paragraph in `docker-compose.yml`, so what is
/// asked is whether those two are still the ones it runs. Changing one is a
/// two-place edit whose second place is here, and nothing is parsed — the
/// entries are compared for equality, so no shell grammar is re-implemented and
/// no spelling of a no-op has to be foreseen.
///
/// Read from the projection [`declared_stack`] returns, which is the one with
/// no variable filled in, for the reason written there.
const HOW_EACH_SERVICE_ANSWERS: [&[&str]; 2] = [
    &["CMD", "/victoria-logs-prod", "--help"],
    &[
        "CMD-SHELL",
        "PGPASSWORD=\"${POSTGRES_PASSWORD:-noctua}\" psql -h \"$$(hostname -i | cut -d' ' -f1)\" -U ${POSTGRES_USER:-noctua} -d ${POSTGRES_DB:-noctua} -c 'SELECT 1' > /dev/null 2>&1",
    ],
];

/// Whether every healthcheck in `declared` runs one of the shapes above.
///
/// # Errors
///
/// Returns which service runs something else, or which declared shape nothing
/// in the stack runs at all.
fn the_healthchecks_are_the_shapes_this_stack_declares(declared: &str) -> Result<(), String> {
    let mut asked: Vec<(String, Vec<String>)> = Vec::new();
    for (path, entry, is_item) in by_path(declared) {
        let under_a_test = path.len() == 4
            && path[0] == "services"
            && path[2] == "healthcheck"
            && path[3] == "test";
        if !is_item || !under_a_test {
            continue;
        }
        match asked.last_mut() {
            Some((service, entries)) if *service == path[1] => entries.push(entry),
            _ => asked.push((path[1].clone(), vec![entry])),
        }
    }
    let is_written_down = |entries: &[String]| {
        HOW_EACH_SERVICE_ANSWERS
            .iter()
            .any(|shape| shape.iter().copied().eq(entries.iter().map(String::as_str)))
    };
    for (service, entries) in &asked {
        if !is_written_down(entries) {
            return Err(format!(
                "`{service}` answers `cargo xtask dev` with {entries:?}, which is not one \
                 of the two commands this stack declares. Every shell no-op is this shape \
                 — it exits 0 having asked the service nothing, and the service then \
                 reports healthy the instant it starts. Put the command back, or declare \
                 the new one in `HOW_EACH_SERVICE_ANSWERS` in the same edit"
            ));
        }
    }
    for shape in HOW_EACH_SERVICE_ANSWERS {
        if !asked
            .iter()
            .any(|(_, entries)| shape.iter().copied().eq(entries.iter().map(String::as_str)))
        {
            return Err(format!(
                "nothing in the resolved stack runs `{}`, so the service that used to ask \
                 it either lost its healthcheck or asks something else now",
                shape.join(" ")
            ));
        }
    }
    // **Every service answers, not merely every declared command.** The floor
    // below counts the healthchecks that are there against the ones written
    // down, and a *third service carrying no healthcheck block at all* moves
    // neither number — measured, a projection holding three services and two
    // healthchecks passed this rule and `every_healthcheck_asks_something`
    // both. `cargo xtask dev` waits on each service in turn, so a service with
    // nothing to ask is one it reports ready the instant the container starts.
    for (path, name, _) in by_path(declared) {
        if path.len() != 1 || path[0] != "services" {
            continue;
        }
        if !asked.iter().any(|(named, _)| *named == name) {
            return Err(format!(
                "the service `{name}` declares no healthcheck, so `cargo xtask dev` has \
                 nothing to wait on and reports it ready the instant its container starts"
            ));
        }
    }
    if asked.len() != HOW_EACH_SERVICE_ANSWERS.len() {
        return Err(format!(
            "{} healthcheck(s) were read and this stack declares {} — a `test:` this \
             reader stepped over is a command nothing above judged",
            asked.len(),
            HOW_EACH_SERVICE_ANSWERS.len()
        ));
    }
    Ok(())
}

/// Whether this run asked docker anything at all.
///
/// **`--offline` promises a run that needs no stack, and this file drove docker
/// anyway.** Measured with the binary off `PATH`: three tests here panic on
/// `Os { code: 2, kind: NotFound }`, so `cargo xtask check --offline` exits 1 —
/// while the `justfile` beside it calls `--offline` *the same gate without the
/// steps that need the stack* and `AGENTS.md` says its whole point is to run
/// with no database. A tool that is not installed is not a defect in this
/// repository.
///
/// A test that could not ask its question has not answered it either, so the
/// gate announces this as a skip of its own and the verdict names it — the
/// number is `OFFLINE_SKIPS` and it moved with this.
///
/// **Asked for, never inferred.** `NC_OFFLINE` is what
/// `cargo xtask check --offline` hands the child, the shape
/// `crates/nc-journal/tests/in_postgres.rs` already uses. A developer running
/// `cargo test` on a machine with no docker still gets the panic above, which is
/// the right answer to *"the stack is not here and nobody said so"*.
fn the_stack_was_asked() -> bool {
    if std::env::var_os("NC_OFFLINE").is_some() {
        println!("the stack: skipped, docker was not asked (NC_OFFLINE)");
        return false;
    }
    true
}

/// Every healthcheck docker resolves is one of the two written down.
#[test]
fn every_healthcheck_runs_a_command_this_stack_declares() {
    if !the_stack_was_asked() {
        return;
    }
    if let Err(why) = the_healthchecks_are_the_shapes_this_stack_declares(&declared_stack()) {
        panic!("{why}");
    }
}

/// The healthcheck half of what `docker compose config` prints uninterpolated.
///
/// The rule above reads a projection and docker is what makes one, so a rule
/// driven only through docker is a rule whose refusals nobody has watched —
/// which is how the last attempt at this one shipped an allowlist that a single
/// unmatched bracket switched off. Copied from this tree's own projection,
/// keeping every key from `services:` down to each `image:` so the walk that
/// finds the entries has the same path to follow.
const AS_DOCKER_PRINTS_IT: &str = r#"services:
  logs:
    healthcheck:
      interval: 5s
      retries: 10
      test:
        - CMD
        - /victoria-logs-prod
        - --help
      timeout: 3s
    image: victoriametrics/victoria-logs:v1.52.0@sha256:47b820890d64c4575a2a0a46415dcd8a4fd59a0f1fcd6a377693d7aea639442e
  postgres:
    healthcheck:
      interval: 2s
      retries: 30
      test:
        - CMD-SHELL
        - PGPASSWORD="${POSTGRES_PASSWORD:-noctua}" psql -h "$$(hostname -i | cut -d' ' -f1)" -U ${POSTGRES_USER:-noctua} -d ${POSTGRES_DB:-noctua} -c 'SELECT 1' > /dev/null 2>&1
      timeout: 3s
    image: postgres:18-alpine@sha256:d3e1620b530c944afa6e887d22eb899824da68e19c52024bf98f5220c88a65b2
"#;

/// Each way this shape has been broken is refused.
///
/// The first three fold a no-op onto postgres's command, at the front and at
/// the end; the fourth puts one in place of the log store's; the fifth is
/// docker's own spelling for *no healthcheck at all*, which arrives under the
/// key rather than on it. The last two are the ones no per-command rule can
/// see and only a floor catches: a service that lost the block entirely, and a
/// third service answering with a command that *is* declared.
#[test]
fn a_healthcheck_that_is_not_a_declared_shape_is_refused() {
    assert_eq!(
        the_healthchecks_are_the_shapes_this_stack_declares(AS_DOCKER_PRINTS_IT),
        Ok(()),
        "the projection copied here and `HOW_EACH_SERVICE_ANSWERS` disagree, so one \
         of the two was edited without the other"
    );
    for (was, becomes, what) in [
        (
            "        - PGPASSWORD=",
            "        - exit 0; PGPASSWORD=",
            "a no-op folded onto the front",
        ),
        (
            "        - PGPASSWORD=",
            "        - true # PGPASSWORD=",
            "a shell comment folded onto the front",
        ),
        (
            " > /dev/null 2>&1\n",
            " > /dev/null 2>&1; exit 0\n",
            "a no-op folded onto the end",
        ),
        (
            "        - /victoria-logs-prod\n        - --help\n",
            "        - true\n",
            "the log store's command replaced by a no-op",
        ),
        (
            "        - CMD\n        - /victoria-logs-prod\n        - --help\n",
            "        - NONE\n",
            "docker's spelling for no healthcheck at all",
        ),
        (
            "    healthcheck:\n      interval: 5s\n      retries: 10\n      test:\n        - CMD\n        - /victoria-logs-prod\n        - --help\n      timeout: 3s\n",
            "",
            "a service with no healthcheck block at all",
        ),
        (
            "services:\n",
            "services:\n  extra:\n    healthcheck:\n      test:\n        - CMD\n                     - /victoria-logs-prod\n        - --help\n",
            "a third service, answering with a command that is declared",
        ),
    ] {
        let broken = AS_DOCKER_PRINTS_IT.replace(was, becomes);
        assert_ne!(
            broken, AS_DOCKER_PRINTS_IT,
            "the projection above no longer holds `{was}`, so this case changed nothing"
        );
        assert!(
            the_healthchecks_are_the_shapes_this_stack_declares(&broken).is_err(),
            "{what} is accepted, and what docker resolves is what the stack runs"
        );
    }
}

/// Every value given for one command-line flag in `docker-compose.yml`.
///
/// Read from the `command:` list rather than parsed as YAML: the file is small,
/// the flags are one per line, and a dependency to read six lines is a
/// dependency `AGENTS.md` would ask about.
///
/// **All of them, not the first.** This was `find_map`, and a command line can
/// name a flag twice — the store applies the **last** one, so repeating a line
/// left this test reading `30d` from the line above while the container ran
/// with `1d`. Measured against `victoria-logs:v1.52.0` with both lines present:
///
/// ```text
/// -retentionPeriod="1d"
/// -retention.maxDiskSpaceUsageBytes="100MiB"
/// ```
///
/// and `cargo test -p xtask --test the_stack_enforces_the_declared_bounds`
/// printing `2 passed`. That is exactly the drift this file exists to prevent,
/// reachable by adding one line — so the answer is not "read the last one"
/// either: a flag given twice is a compose file whose reader and whose runtime
/// disagree, and [`only`] refuses it.
fn flags(compose: &str, name: &str) -> Vec<String> {
    compose
        .lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches("- ").trim_matches('"');
            line.strip_prefix(&format!("{name}="))
                .map(|value| value.trim_matches('"').to_owned())
        })
        .collect()
}

/// The one value given for `name`, or why there is not exactly one.
fn only(compose: &str, name: &str) -> Result<String, String> {
    let mut given = flags(compose, name);
    match given.len() {
        1 => Ok(given.remove(0)),
        0 => Err(format!("`{name}` is not given to the log store at all")),
        n => Err(format!(
            "`{name}` is given {n} times, as {given:?}. The store applies the last and this \
             file's readers take the first, so the two numbers in `catalog/limits.toml` and \
             the number the container runs with can differ with every check green"
        )),
    }
}

/// A `GiB`/`MiB`/`KiB`/`B` size as bytes.
fn bytes(value: &str) -> Option<u64> {
    for (suffix, scale) in [
        ("GiB", 1024_u64.pow(3)),
        ("MiB", 1024_u64.pow(2)),
        ("KiB", 1024),
        ("B", 1),
    ] {
        if let Some(number) = value.strip_suffix(suffix) {
            // **`checked_mul`, like its twin.** `xtask/src/store.rs`'s `bytes`
            // was repaired for this and its doc says these are "the same
            // scales"; this copy kept the plain multiply. A bound congruent to
            // the declared one modulo 2^64 — `17179869192GiB` against 8 GiB —
            // then wraps to exactly the right number. It panics today only
            // because the test profile inherits `dev`'s overflow checks, and
            // `Cargo.toml`'s `[profile.release]` sets none.
            return number
                .trim()
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(scale));
        }
    }
    value.parse().ok()
}

#[test]
fn the_log_store_is_started_with_the_bounds_the_catalog_declares() {
    let compose = std::fs::read_to_string(root().join("docker-compose.yml"))
        .expect("the stack definition is there");
    let store = nc_catalog::STORES
        .iter()
        .find(|store| store.name == "central_log")
        .expect("the catalog bounds the log store");

    let retention = only(&compose, "--retentionPeriod").expect("one retention period");
    assert_eq!(
        retention,
        format!("{}d", store.retention_days),
        "docker-compose.yml starts the store with a retention the catalog does not declare"
    );

    let disk = only(&compose, "--retention.maxDiskSpaceUsageBytes").expect("one disk bound");
    assert_eq!(
        bytes(&disk),
        Some(store.max_disk_bytes),
        "docker-compose.yml starts the store with a disk bound the catalog does not declare: \
         `{disk}`"
    );

    // **And where it writes them**, because a retention is a promise about a
    // directory. Measured, `--storageDataPath` alone among the three flags was
    // read by nothing in this file: pointed at `/tmp/planted-outside-the-volume`
    // it left every test here green, `check` green and `cargo xtask dev`
    // printing `the stack is up`, with every log line going to storage that dies
    // with the container. `check`'s `where_the_store_writes` does compare the
    // two, but it asks the *running* store, so it is green against any container
    // started before the edit — the file was only refused after a restart.
    let writes = only(&compose, "--storageDataPath").expect("one data path");
    let mounted = mount_target(&compose, "logs-data")
        .unwrap_or_else(|| panic!("`logs-data` is mounted into the log store"));
    assert_eq!(
        writes, mounted,
        "the log store is started with `--storageDataPath={writes}` and its named volume is \
         mounted at `{mounted}`: everything it writes goes to storage that dies with the \
         container, and the retention the catalog declares becomes a promise about a \
         directory nothing keeps"
    );

    // **And the catalog names this file as the enforcer**, which is the sentence
    // the comparison above makes true rather than merely plausible.
    assert!(
        store.enforced_by.contains("docker-compose.yml"),
        "the catalog names `{}` as the enforcer, and this test checks docker-compose.yml",
        store.enforced_by
    );
}

/// Where `named` is mounted, read from the same file the flags come from.
///
/// The volume line is `- <name>:<target>` under a service's `volumes:`, and the
/// target is what the container sees. Written here rather than taken from the
/// needle census one screen down, because that census pins the pair as one
/// string and this comparison needs the half of it a flag can disagree with.
fn mount_target(compose: &str, named: &str) -> Option<String> {
    compose
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("- "))
        .filter_map(|line| line.strip_prefix(named))
        .find_map(|rest| rest.strip_prefix(':'))
        .map(|target| target.trim_matches('"').to_owned())
}

/// Every published port is bound to loopback.
///
/// **The one line the file calls load-bearing, and nothing read it.** The
/// compose file's own header says `127.0.0.1:` in front of every port is what
/// keeps the database and the log store off the network, and "this is the one
/// line that prevents it". Rewriting both entries to publish on every
/// interface left `cargo xtask check` green:
///
/// ```yaml
/// - "0.0.0.0:${POSTGRES_PORT:-5432}:5432"
/// - "${LOGS_PORT:-9428}:9428"          # the bare form is 0.0.0.0
/// ```
///
/// A database reachable from the network with the password in
/// `.env.example` is not a local development stack. This is the same shape as
/// the defect the rest of this file exists for — the catalog named an enforcer
/// and nothing checked the thing it named.
///
/// The bare `port:port` form is the trap: it looks narrower than the explicit
/// one and is the widest of the three.
///
/// **And it read one file, which is not what docker runs.** This was a text
/// scan of `docker-compose.yml`, exactly the bypass `gates.rs` documents fixing
/// for the image gate in the same tree. Measured with an override file named
/// `docker-compose.override.yml` <!-- allow-reference: `docker-compose.override.yml` is what that measurement created and then removed; this stack ships no override, which is the point --> adding `ports: ["0.0.0.0:5432:5432"]`
/// and a service on `network_mode: host`:
///
/// ```text
/// what docker compose actually resolves:
///   postgres:  host_ip: 127.0.0.1 / published: "5432"
///              host_ip: 0.0.0.0   / published: "5432"
/// what this test said:  ok
/// what `cargo xtask dev` printed: both bind to loopback only
/// ```
///
/// PostgreSQL on every interface with the password from `.env.example`, and a
/// green run. So it asks the tool, like the image gate does.
#[test]
fn every_published_port_is_bound_to_loopback() {
    if !the_stack_was_asked() {
        return;
    }
    // **With every profile.** A service behind `profiles: ["debug"]` is absent
    // from the default projection entirely — measured, one with
    // `network_mode: host` and `ports: ["0.0.0.0:5433:5432"]` left all three
    // assertions below green while `docker compose --profile debug up` publishes
    // it on every interface.
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let resolved = std::process::Command::new("docker")
        .args(["compose", "--profile", "*", "config"])
        .current_dir(root())
        .output()
        .expect(
            "docker is what resolves overrides, `extends`, anchors and variables into the \
             stack that actually runs; a text scan of one file is the bypass this test \
             exists to close",
        );
    assert!(
        resolved.status.success(),
        "`docker compose config` refused the stack: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let resolved = String::from_utf8_lossy(&resolved.stdout);
    let mut published = 0_usize;
    // **How many ports the stack publishes**, counted apart from how many name
    // an address. The rule below is a predicate on `host_ip:` lines and its
    // floor counted those same lines — so a port written in the long syntax
    // with no host address produces `target:`/`published:` and **no `host_ip:`
    // line at all**, and both the rule and its floor step over it. Measured
    // live: `0.0.0.0:5433->5432/tcp`, reachable from another machine on the
    // network, with this test green.
    // **Counted as entries, not as `published:` lines.** The compose short form
    // `- "5432"` produces `target:` and `protocol:` and *neither* a
    // `published:` nor a `host_ip:` — so the rule and its floor both stepped
    // over it, and `docker ps` showed `0.0.0.0:32768->5432/tcp` with an
    // authenticated session reachable over the LAN using the password in
    // `.env.example`, under `check passed` and `both bind to loopback only`.
    // **Every entry of the `ports:` list, whatever key compose writes first.**
    // This counted lines opening `- mode:` on the ground that "every entry in
    // the projection opens with" it — and an entry that carries a `name:` does
    // not: compose emits `name` before `mode`. Measured, a second long-form
    // entry on postgres whose first key is `name`:
    //
    // ```text
    //   - mode: ingress
    //     host_ip: 127.0.0.1
    //     target: 5432
    //     published: "55961"
    //   - name: admin            <- no host_ip:, published on every interface
    //     mode: ingress
    //     published: "15432"
    // ```
    //
    // `published == ports == 2` held, the file half skipped it because `- name:`
    // opens with none of the characters it looks for, and `cargo xtask check`
    // passed with PostgreSQL reachable from the network under the password in
    // `.env.example`. The rule above closed the case and not the class; a list
    // item is a list item, so that is what is counted.
    let mut ports = 0_usize;
    let mut inside_ports = false;
    let mut opened_at = 0_usize;
    for line in resolved.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if trimmed == "ports:" {
            inside_ports = true;
            opened_at = indent;
            continue;
        }
        if inside_ports && !trimmed.is_empty() && indent <= opened_at {
            inside_ports = false;
        }
        if inside_ports && trimmed.starts_with("- ") {
            ports += 1;
            continue;
        }
        // **`network_mode: host` publishes nothing and exposes everything.** It
        // makes the container share the host's stack, so no `ports:` entry
        // appears for it at all and a rule about published ports never sees it.
        assert!(
            trimmed != "network_mode: host",
            "a service shares the host's network, so nothing it listens on is bound by \
             this stack at all"
        );
        let Some(address) = trimmed.strip_prefix("host_ip:") else {
            continue;
        };
        let address = address.trim().trim_matches(['"', '\'']);
        published += 1;
        assert!(
            address == "127.0.0.1" || address == "::1",
            "`{address}` publishes on every interface. The compose file's own header \
             calls the `127.0.0.1:` prefix the one line that keeps this stack off \
             the network"
        );
    }
    every_healthcheck_asks_something(&resolved);
    // **The floor.** A projection with no ports in it, or a reader that stopped
    // matching them, would otherwise pass by inspecting nothing — which is the
    // failure this whole file is about.
    assert_eq!(
        published, ports,
        "the stack publishes {ports} port(s) and names an address for {published}. A \
         port written long-form with no `host_ip` is published on every interface, and \
         the rule above is a predicate on lines that one does not produce"
    );
    assert_eq!(
        published, 2,
        "the stack publishes {published} port(s); this test read the wrong lines, \
         or a service gained one nobody has looked at"
    );

    // **And what the file says, because the projection is the caller's
    // environment.** `docker compose config` interpolates from the running
    // process, so `"${BIND_ADDRESS:-127.0.0.1}:${POSTGRES_PORT:-5432}:5432"` —
    // written in the idiom this file already uses for the port — is loopback
    // here and every interface under `BIND_ADDRESS=0.0.0.0`, with the same tree
    // green or red depending on a variable. The compose header calls the
    // `127.0.0.1:` prefix "the one line that prevents it"; this is what says
    // the line is still there. The image gate reads its files for the same
    // reason.
    let written = std::fs::read_to_string(root().join("docker-compose.yml"))
        .expect("the compose file is there");
    let mut entries = 0_usize;
    for (index, line) in written.lines().enumerate() {
        let trimmed = line.trim();
        let Some(entry) = trimmed.strip_prefix("- ") else {
            continue;
        };
        let entry = entry.trim().trim_matches(['"', '\'']);
        // **A published port is any entry under `ports:`, colon or not.** This
        // required one, so the short form `- "5432"` — which publishes on every
        // interface — was not read as a port at all.
        if !entry
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '$' || c == '[')
        {
            continue;
        }
        entries += 1;
        assert!(
            entry.starts_with("127.0.0.1:") || entry.starts_with("[::1]:"),
            "docker-compose.yml:{} publishes `{entry}`, whose host address is not \
             written down — so what it binds is whatever the environment says",
            index + 1
        );
    }
    assert_eq!(
        entries, 2,
        "this read {entries} published port(s) in the file and the stack has two"
    );
}

/// The example environment says what the stack reads, and it is checked.
///
/// **Its values were read by nothing at all.** The gate refuses a committed
/// `.env`, and `no_unread_variable` refuses a variable in `.env.example` that
/// no file reads — both about *names*. The numbers and words beside them were
/// judged by nobody, and this file makes two claims about them in its own
/// prose: that "every variable `docker-compose.yml` reads is defaulted there to
/// the value below", and that `DATABASE_URL` is "derived from the four above".
/// Neither survives an edit to one side. Change `POSTGRES_PORT` to `5433` and
/// the file ships a `DATABASE_URL` pointing at a port nothing listens on, with
/// a sentence above it saying the two agree — and the first person to read it
/// is a new developer, who has no way to know which half is right.
#[test]
fn the_example_environment_agrees_with_the_compose_defaults() {
    let compose = std::fs::read_to_string(root().join("docker-compose.yml")).expect("readable");
    let example = std::fs::read_to_string(root().join(".env.example")).expect("readable");

    let set: Vec<(String, String)> = example
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let value = |name: &str| {
        set.iter().find(|(set, _)| set == name).map_or_else(
            || panic!("`.env.example` does not set `{name}`"),
            |(_, value)| value.clone(),
        )
    };
    // The floor: a file whose variables stopped parsing would agree with every
    // default by having none to disagree.
    assert!(
        set.len() >= 6,
        "`.env.example` parsed as {} variable(s), so this test compared almost nothing",
        set.len()
    );

    // Claim one: what compose falls back to is what this file says.
    let mut compared = 0_usize;
    for (name, _) in &set {
        let Some((_, after)) = compose.split_once(&format!("${{{name}:-")) else {
            // `DATABASE_URL` is the one compose does not read; the file says so.
            continue;
        };
        let default = after
            .split('}')
            .next()
            .expect("a default ends at its brace")
            .to_owned();
        assert_eq!(
            default,
            value(name),
            "`docker-compose.yml` falls back to `{default}` for `{name}` and \
             `.env.example` says `{}` — the file's own opening says the two agree, and \
             a developer who changes one is told nothing",
            value(name)
        );
        compared += 1;
    }
    assert!(
        compared >= 5,
        "{compared} variable(s) were compared against a compose default, and this stack \
         reads more than that: the `${{NAME:-` form is what this looks for, so a compose \
         file that stopped using it would pass by matching nothing"
    );

    // Claim two: `DATABASE_URL` is the four above it, assembled.
    assert_eq!(
        value("DATABASE_URL"),
        format!(
            "postgres://{}:{}@127.0.0.1:{}/{}",
            value("POSTGRES_USER"),
            value("POSTGRES_PASSWORD"),
            value("POSTGRES_PORT"),
            value("POSTGRES_DB")
        ),
        "`.env.example` says `DATABASE_URL` is derived from the four above it, and it is \
         not: what ships points somewhere the rest of the file does not describe"
    );
}

/// Every load-bearing line of `docker-compose.yml` that nothing else reads.
///
/// **Two flags and the port bindings were checked, and the rest of the file was
/// not.** `restart:`, `POSTGRES_INITDB_ARGS`, the volume's mount point and both
/// healthchecks are each written with a comment recording the incident that put
/// them there — a restart loop that took an afternoon, a healthcheck that
/// reported green while every application connection failed, a sort order that
/// differed between two developers. Every one of them could be deleted and the
/// whole gate stayed green.
///
/// A written list, in the shape of `check.rs`'s `SUITES` and `wall.rs`'s
/// `SHAPES`: removing one is then a two-place edit, and the second place is
/// here, where the reason is written down for whoever is removing it.
#[test]
fn the_compose_file_still_holds_what_it_was_fixed_to_hold() {
    let compose = std::fs::read_to_string(root().join("docker-compose.yml")).expect("readable");
    // **The comments are not the file.** Both spellings refused below are
    // written out in the comment that explains why they are refused, together
    // with the measurements that settled it — so a search of the whole text
    // finds them and reports the explanation as the defect. What is judged is
    // what compose executes.
    let code: String = compose
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .fold(String::new(), |mut kept, line| {
            kept.push_str(line);
            kept.push('\n');
            kept
        });
    // The floor is on what is left, not on a ratio: two thirds of this file is
    // prose recording the incidents behind it, so "most of it survived" is the
    // wrong shape and was measured saying 47 of 153 was too few. A stripper
    // that returned nothing would fail every count below on its own; what needs
    // saying here is that the *negative* checks still have something to look at.
    assert!(
        code.contains("services:") && code.lines().count() >= 30,
        "the comment stripper left {} line(s) and no `services:`, so the two spellings \
         refused below would be absent for having nothing to be absent from",
        code.lines().count()
    );
    for (needle, expected, why) in [
        (
            "restart: unless-stopped",
            2,
            "a service that does not come back after a reboot is a stack that looks up \
             and is not",
        ),
        (
            "POSTGRES_INITDB_ARGS: \"--locale=C --encoding=UTF8\"",
            1,
            "without it the database's sort order comes from the host's locale, and \
             `ORDER BY` answers differently on two developers' machines",
        ),
        (
            "- postgres-data:/var/lib/postgresql\n",
            1,
            "one level up from `/data`: PostgreSQL 18's image restart-loops against the \
             older mount point, and a future `pg_upgrade --link` cannot cross a mount \
             boundary",
        ),
        (
            "- logs-data:/victoria-logs-data",
            1,
            "the log store's retention is a promise about a disk, and an unnamed volume \
             is a disk that goes with the container",
        ),
        (
            "healthcheck:",
            2,
            "`cargo xtask dev` waits on these, and a service with none reports healthy \
             the instant it starts",
        ),
        // **And that each one asks something.** The key was counted and its
        // block was read by nothing: `what_a_service_is_handed`'s `under()`
        // fires only for `environment`, `volumes` and `command`, and the
        // vocabulary walk has arms for depths 0, 1 and 2 while a healthcheck's
        // body is at depth 3 — so `disable: true`, which is docker's own
        // spelling for *no healthcheck at all*, left `cargo xtask check`
        // byte-identical to a clean run. Not one character of the output
        // differed. `dev.rs` refuses it at runtime, so CI is red by another
        // road; the file rule this test states did not hold.
        //
        // Each block names the command it runs, so that is what is counted.
        (
            "      test:",
            2,
            "a healthcheck with no command is a service that reports healthy the \
             instant it starts, and `disable: true` is docker's spelling for one",
        ),
        (
            "disable:",
            0,
            "`disable: true` turns a healthcheck off, and `cargo xtask dev` then waits \
             on a service that was never asked whether it was ready",
        ),
        (
            "psql -h \"$$(hostname -i | cut -d' ' -f1)\"",
            1,
            "the container's own routable address is what `pg_hba.conf`'s scram rule \
             matches; `127.0.0.1` matches the `trust` line above it, so a wrong password \
             left the stack healthy with every application connection failing",
        ),
    ] {
        assert_eq!(
            code.matches(needle).count(),
            expected,
            "`docker-compose.yml` holds `{needle}` a number of times that is not \
             {expected}, and {why}"
        );
    }
    // And the two spellings that were measured to be wrong, which a later edit
    // would most naturally reach for.
    for (wrong, why) in [
        (
            "pg_isready",
            "it pings the postmaster and never authenticates or opens the database it is \
             given a name for — measured answering `accepting` for a role and a database \
             that do not exist",
        ),
        (
            "psql -h 127.0.0.1",
            "`initdb` writes `host all all 127.0.0.1/32 trust` and pg_hba is \
             first-match-wins, so the password is never checked",
        ),
    ] {
        assert!(
            !code.contains(wrong),
            "`docker-compose.yml` uses `{wrong}` again, and {why}"
        );
    }
}

/// What `docker compose config` prints for this tree, with `also` on the end.
///
/// Its own function because the projection is now asked for twice and the two
/// requests differ by one flag; a second copy of a spawn is a second place for
/// the arguments to drift.
fn projection(also: &[&str]) -> String {
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let resolved = std::process::Command::new("docker")
        .args(["compose", "--profile", "*", "config"])
        .args(also)
        .current_dir(root())
        .output()
        .expect(
            "docker is what resolves overrides, `extends`, anchors and variables into the \
             stack that actually runs; a text scan of one file is the bypass this test \
             exists to close",
        );
    assert!(
        resolved.status.success(),
        "`docker compose config` refused the stack: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    String::from_utf8_lossy(&resolved.stdout).into_owned()
}

/// The stack docker actually resolves, as canonical YAML.
///
/// Docker merges up to eight filenames and expands anchors, `extends`, flow
/// collections, variables and the short volume form; a text scan of one file
/// sees none of that. `every_published_port_is_bound_to_loopback` has resolved
/// through this since it was written, for the same reason, and the security
/// vocabulary did not.
fn resolved_stack() -> String {
    projection(&[])
}

/// The same stack with no variable filled in.
///
/// **An interpolated projection carries the caller's environment, and comparing
/// a command against a written shape there is a false failure waiting for a
/// developer.** `docker compose config` fills `${POSTGRES_PASSWORD:-noctua}`
/// from the process that ran it and from any `.env` beside the stack, so the
/// postgres healthcheck's text on one machine is not its text on another.
/// Measured with a password holding a space, a colon and a hash: YAML then
/// quotes the whole entry and doubles every quote inside it, so not one
/// character of that line survives — and the last attempt at
/// [`HOW_EACH_SERVICE_ANSWERS`] read exactly that line, on a machine where
/// nothing was wrong.
///
/// `--no-interpolate` leaves every `${…}` and every `$$` as `docker-compose.yml`
/// writes them, so what is compared depends on the files in this tree and on
/// nothing else — while the merge of a second compose file, which is the bypass
/// the projection exists to close, still happens. Measured, an override file
/// giving postgres `test: ["CMD-SHELL", "exit 0"]`:
/// [`the_healthchecks_are_the_shapes_this_stack_declares`] read `exit 0` out of
/// *this* projection and refused it, while
/// [`every_healthcheck_asks_something`], which judges the interpolated one,
/// printed `ok`.
fn declared_stack() -> String {
    projection(&["--no-interpolate"])
}

/// The path of keys leading to each line of a YAML document, by indentation.
///
/// **The vocabulary was a predicate on two columns**, `indent == 0` and
/// `indent == 4`, and YAML only requires *siblings* to agree. A service whose
/// keys sit at six spaces was therefore never judged at all: measured, a third
/// service carrying `privileged: true`, `user: "0:0"`, `cap_add: [SYS_ADMIN]`,
/// `security_opt`, and bind mounts of `/` and the docker socket, resolved in
/// full by `docker compose config` and reported by nothing —
/// `ok 3 image(s) in the resolved stack … each pinned by digest`, `check
/// passed`. The image gate counted the third service; the security vocabulary
/// never entered it.
///
/// So the reader is structural: a line's parents are whichever keys are less
/// indented than it, nearest first, and the rules below name paths rather than
/// columns. Returns `(path, key, is_item)` per meaningful line, where a list
/// item carries the text after `- ` as its key.
fn by_path(document: &str) -> Vec<(Vec<String>, String, bool)> {
    let mut open: Vec<(usize, String)> = Vec::new();
    let mut found = Vec::new();
    for line in document.lines() {
        let bare = line.trim();
        if bare.is_empty() || bare.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        while open.last().is_some_and(|(at, _)| *at >= indent) {
            open.pop();
        }
        let path: Vec<String> = open.iter().map(|(_, key)| key.clone()).collect();
        if let Some(item) = bare.strip_prefix("- ") {
            found.push((path, item.trim().to_owned(), true));
            continue;
        }
        let Some((key, rest)) = bare.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches(['"', '\'']).to_owned();
        found.push((path, key.clone(), false));
        // A key with nothing after it opens a block; one with a value does not.
        if rest.trim().is_empty() {
            open.push((indent, key));
        }
    }
    found
}

/// What the services are handed inside `environment:` and `volumes:`.
///
/// The two shapes that matter are not service keys at all:
/// `POSTGRES_HOST_AUTH_METHOD: trust` is an entry under `environment:` — one
/// line, and the database then takes any password, measured — and a host bind
/// mount is an item under `volumes:`, which the port reader skips because it
/// begins with a separator.
fn what_a_service_is_handed(compose: &str) {
    const IN_THE_ENVIRONMENT: [&str; 4] = [
        "POSTGRES_USER",
        "POSTGRES_PASSWORD",
        "POSTGRES_DB",
        "POSTGRES_INITDB_ARGS",
    ];
    let mut settings = 0_usize;
    let mut mounts = 0_usize;
    let mut commands = 0_usize;
    for (path, key, is_item) in by_path(compose) {
        let under = |what: &str| path.len() == 3 && path[0] == "services" && path[2] == what;
        if under("environment") && !is_item {
            assert!(
                IN_THE_ENVIRONMENT.contains(&key.as_str()),
                "`docker-compose.yml` passes `{key}` to a service, and nothing here has \
                 decided about it. `POSTGRES_HOST_AUTH_METHOD: trust` is one line and \
                 makes the database accept any password, with every other check in this \
                 file green"
            );
            settings += 1;
        }
        // The resolved form is `- type: volume` / `source:` / `target:`, so the
        // host path — if there is one — is a `source:` under a `volumes` item
        // whose `type` is `bind`. The short form `- /:/host` is what the file
        // may hold and what the resolver expands.
        // **And what a service is told to run.** `command` is allowed per service
        // because the log store's bounds are passed that way, and its *items*
        // were judged by nothing — so
        // `command: ["sh", "-c", "cat /etc/shadow > /tmp/x; exec docker-entrypoint.sh postgres"]`
        // was accepted, which replaces the process the image starts with an
        // arbitrary one. Everything this stack legitimately passes is a flag,
        // and the three the store takes are compared against the catalog
        // elsewhere in this file.
        if under("command") && is_item {
            assert!(
                key.starts_with("--"),
                "a service is told to run `{key}`, and the only thing this stack passes as \
                 a command is a flag to the process the image already starts. A word that \
                 is not a flag replaces that process"
            );
            commands += 1;
        }
        if under("volumes") && is_item {
            let mount = key.trim_matches('"');
            // **The item's own text, which `docker compose config` always writes
            // long-form.** A volume item is `type: volume` or `type: bind`, and
            // the host path — if there is one — is the sibling `source:` line,
            // which `is_item` is false for and no branch read. So
            // `starts_with(['/', '.', '~'])` could never fire: measured, a real
            // `- /:/host` bind on `postgres` was caught by the `mounts == 2`
            // hand count and by nothing else, and that count is two literals
            // away from a stack that gains a service.
            //
            // Every mount this stack has is a named volume, so the type is what
            // is asserted — and a bind arrives as a different type rather than
            // as a path this rule has to recognise.
            assert_eq!(
                mount, "type: volume",
                "`docker-compose.yml` mounts `{mount}` into a service. A named volume is \
                 data this stack owns; a bind is the developer's machine inside the \
                 container, and a bind of `/` or the docker socket is the whole machine"
            );
            mounts += 1;
        }
    }
    assert_eq!(
        settings,
        IN_THE_ENVIRONMENT.len(),
        "{settings} environment entr(ies) were read"
    );
    assert_eq!(
        mounts, 2,
        "{mounts} volume mount(s) were read, and there are two"
    );
    assert_eq!(
        commands, 3,
        "{commands} command flag(s) were read, and the log store is passed three"
    );
}

/// The floors under the vocabulary walk, all four of them.
///
/// A walk that read nothing would refuse nothing; a vocabulary nothing uses is a
/// list that has stopped describing the file; a service the walk did not enter
/// is a service nothing judged — which is exactly how a third one went unread —
/// and a volume definition it did not enter is where a bind mount wears a
/// volume's name.
fn the_walk_entered_the_whole_stack(
    top: usize,
    services: usize,
    volumes: usize,
    service: usize,
    at_the_top: usize,
) {
    // The floors, on all three ends: a walk that read nothing would refuse
    // nothing, a vocabulary nothing uses is a list that has stopped describing
    // the file, and a service the walk did not enter is a service nothing
    // judged — which is exactly how a third one went unread.
    assert_eq!(top, at_the_top, "{top} top-level key(s) were read");
    assert_eq!(
        services, 2,
        "{services} service(s) were read, and this file defines two — a service the walk \
         does not enter is one whose keys nothing judges"
    );
    assert_eq!(
        volumes, 2,
        "{volumes} volume definition(s) were read, and this stack declares two"
    );
    assert!(
        service >= 12,
        "{service} service key(s) were read, and this file gives two services seven \
         kinds of key between them"
    );
}

/// Every region the walk names was actually counted.
///
/// **`networks:` was counted at no depth.** Every key under it fell to a silent
/// `_ => {}`, so a service could be attached to a second, `attachable` network —
/// or an `external:` one this repository does not own — with every other floor
/// holding, because none of them counts anything there.
fn the_regions_are_all_counted(networks: usize, network: usize, named_volumes: usize) {
    // **Two floors, because their sum is one.** `networks + network > 0` is
    // satisfied by `networks` alone — the projection gives one of each — so the
    // counter saying the network *definition* was entered and its keys judged
    // was floored by nothing, which is how the only unheld vocabulary in this
    // file came to be the one inside it.
    assert!(
        networks > 0,
        "the walk read no `networks:` key at all, so nothing there is judged"
    );
    assert!(
        network > 0,
        "the walk entered no top-level network, so `a_top_level_network_key` judged \
         nothing and its list is a description of nothing"
    );
    assert_eq!(
        named_volumes, 2,
        "the stack declares {named_volumes} named volume(s) and it has two — a volume \
         nothing enters is one whose keys nothing judges"
    );
}

/// What a key at depth three or deeper may be, by the block it sits in.
///
/// **A row here that no key can reach is a description of nothing.** The list
/// carried `("services", "depends_on", &[])`, and `depends_on` is not in
/// `PER_SERVICE`, so `a_service_key` refuses it one level up and the depth rule
/// is never reached — measured, the plant failed at `a_service_key`, naming the
/// key as one "nothing here has decided about". `PER_SERVICE` is held to the
/// opposite rule one screen down, on the argument that a vocabulary with
/// entries nothing uses stops being a description; this list had no such loop,
/// which is why the dead row was invisible.
///
/// **The fall-through it replaces could not fail.** It was
/// `path[0] == top && path.len() >= depth` with depth 3 for all three roots —
/// and depth 0 is closed by the top-level list, depths 1 and 2 have arms of
/// their own, so every path reaching the default arm was at depth ≥3 under one
/// of those three roots and every one was accepted. Measured: `x-planted: true`
/// inside `healthcheck:` and `networks: {default: {aliases: [db]}}` on a service
/// both passed all six tests, and docker emits both in the projection. Two of
/// those are escapes the network rule's own docstring claims to have closed.
///
/// So the regions are named with the keys each admits, and a key that is not
/// here is a finding — the `SKIP`/`COVERED` shape this repository uses, where
/// adding one is a line here with the reason rather than a silence.
const AT_DEPTH_THREE: [(&str, &str, &[&str]); 7] = [
    // **A label's own key is arbitrary text, and `a_top_level_network_key`
    // allows the block that holds it.** Without this row the two rules
    // contradict each other: `labels` is a key a top-level network may carry,
    // and every key *inside* it fell to the depth rule — measured,
    // `networks.default.labels.com.noctua.owner: central` was refused with
    // "nothing here has decided about" while the block above it was allowed.
    // A label is a note to a reader and to `docker inspect`; it changes nothing
    // about what can reach the database, which is what that rule is for.
    ("networks", "labels", &[]),
    (
        "services",
        "healthcheck",
        &[
            "test",
            "interval",
            "timeout",
            "retries",
            "start_period",
            "disable",
        ],
    ),
    // A service's own network membership. The key here is the *name* of a
    // network, which is arbitrary text — so what is checked is not the spelling
    // but that the stack declares it, which `a_service_joins_only_a_declared_network`
    // does with the top-level list in front of it.
    ("services", "networks", &[]),
    // The items of a list are indices, not keys; the list itself is judged at
    // depth two.
    ("services", "ports", &[]),
    ("services", "volumes", &[]),
    // The three `what_a_service_is_handed` already reads item by item, with its
    // own rules about a host path, a secret and a shell.
    ("services", "environment", &[]),
    ("services", "command", &[]),
];

/// Every network a service joins is one the stack declares.
///
/// The depth-three key under a service's `networks:` is a network's name, so the
/// vocabulary rule cannot spell it out — what it can say is that the name is one
/// the top-level `networks:` block defines. A service attached to a network this
/// file does not declare is a service reachable by something this file does not
/// describe.
fn a_service_joins_only_a_declared_network(resolved: &str) -> Result<usize, String> {
    let mut declared: Vec<String> = Vec::new();
    let mut joined: Vec<(String, String)> = Vec::new();
    for (path, key, _) in by_path(resolved) {
        if path.len() == 1 && path[0] == "networks" {
            declared.push(key.clone());
        }
        if path.len() == 3 && path[0] == "services" && path[2] == "networks" {
            joined.push((path[1].clone(), key.clone()));
        }
    }
    for (service, network) in &joined {
        if !declared.contains(network) {
            return Err(format!(
                "the service `{service}` joins the network `{network}`, which this stack \
                 does not declare — so what can reach it is described somewhere other than \
                 here"
            ));
        }
    }
    Ok(joined.len())
}

// Per service. Anything that grants a capability, changes the user, mounts a
// host path or relaxes authentication is absent on purpose and stays absent.
const PER_SERVICE: [(&str, &str); 8] = [
    (
        "image",
        "pinned by digest, and the pin is checked against the tag",
    ),
    ("restart", "so the stack comes back after a reboot"),
    (
        "ports",
        "written the long way so every one binds to loopback",
    ),
    ("environment", "the settings `.env.example` documents"),
    (
        "command",
        "the flags the catalog declares and the gate reads back",
    ),
    (
        "volumes",
        "named volumes only — a host path is what this list refuses",
    ),
    ("healthcheck", "what `cargo xtask dev` waits on"),
    // Added by the resolver, not by the file: every service joins the
    // default network unless it says otherwise, and `network_mode: host`
    // would appear here as a key of its own.
    (
        "networks",
        "the default network, which compose attaches by name",
    ),
];

/// The keys a service may carry.
///
/// Its own function for the reason its neighbour is one: the test that calls
/// both is at clippy's line limit.
fn a_service_key(path: &[String], key: &str) {
    assert!(
        PER_SERVICE.iter().any(|(allowed, _)| *allowed == key),
        "`docker-compose.yml` gives the service `{}` the key `{key}`, which nothing here \
         has decided about — and the keys that are absent are absent on purpose: \
         `privileged`, `user`, `cap_add`, `security_opt`, `devices`, `env_file`, \
         `network_mode` and a host path under `volumes` each turn the stack a developer \
         is told to run into something else. Add it to this list with the reason it is \
         allowed, or take it out",
        path[1]
    );
}

/// The keys a top-level `networks:` definition may carry.
///
/// **This region was decided at no depth.** The walk had arms for 0, 1 and 2
/// over `services` and `volumes` and a bare `_ => {}`, so a service's own
/// `networks:` block and the top-level definitions it points at were read by
/// nothing — measured, `attachable: true`, `aliases: ["db"]` and an `x-planted:`
/// key inside `healthcheck:` all passing every test in this file with every
/// floor holding. `network_mode: host` is refused two levels up with a
/// paragraph saying why, while the network a service actually joins was not
/// looked at.
///
/// **And then the list allowed `external`, one line above a refusal that names
/// it.** The sentence below calls `external` a key that changes what can reach
/// the database and the `matches!` let it through, so the two disagreed inside
/// six lines. Measured, an override giving the top-level `default` network
/// `name: bridge` and `external: true` — which points every service in this
/// stack at a network this file neither creates nor owns, and takes the two
/// they share out of this repository's hands: all 8 tests in this target
/// printed `ok`. With it deleted the list says what the sentence says.
///
/// Its own function because the test that calls it is at clippy's line limit.
fn a_top_level_network_key(path: &[String], key: &str) -> Result<(), String> {
    if matches!(key, "name" | "driver" | "labels") {
        return Ok(());
    }
    Err(format!(
        "the top-level network `{}` carries `{key}`, which nothing here has decided \
         about — `attachable` and `external` each change what can reach the database. \
         Add it with the reason it is allowed, or take it out",
        path[1]
    ))
}

/// The rule above, driven by written projections rather than by the stack.
///
/// **Because docker will not let it fail over a real projection.** Asked for one
/// where a service joins a network the stack does not declare, `docker compose
/// config` refuses to produce it at all — measured, `config` exited 1 and this
/// rule was never reached. Over the resolved stack it is a floor on what the
/// walk found; these are what make it a rule.
fn the_network_membership_rule_is_driven() {
    assert!(
        a_service_joins_only_a_declared_network(
            "services:\n  db:\n    networks:\n      backend: null\nnetworks:\n  backend: null\n"
        )
        .is_ok(),
        "a service on a network the stack declares is refused, and this rule is about what \
         is *not* declared"
    );
    for (projection, what) in [
        (
            "services:\n  db:\n    networks:\n      elsewhere: null\nnetworks:\n  backend: null\n",
            "a service on a network nothing declares",
        ),
        (
            "services:\n  db:\n    networks:\n      backend: null\n",
            "a service on a network when the stack declares none at all",
        ),
    ] {
        assert!(
            a_service_joins_only_a_declared_network(projection).is_err(),
            "{what} is accepted, and what can reach a service is then described \
             somewhere other than this file"
        );
    }
}

/// The vocabulary above, driven by written projections rather than by the stack.
///
/// **Because the stack as it ships uses three of the three allowed names**, and
/// so cannot tell an allowlist of three from one of five: `external` and
/// `attachable` were added back to it and all eight tests stayed green, which is
/// the regression the rule's own paragraph records as measured once already.
///
/// It must keep accepting `name:` — the key this stack's own network carries —
/// which is the other way to get this wrong.
fn the_network_vocabulary_is_driven(compose: &str) {
    assert!(
        the_top_level_network_keys_in(compose).expect("the stack this repository ships") > 0,
        "the projection carries no top-level network key, so the two cases below are \
         a statement about a rule nothing reaches"
    );
    assert!(
        the_top_level_network_keys_in("networks:\n  default:\n    name: nc\n").is_ok(),
        "`name:` is refused, and it is the key this stack's own network carries"
    );
    for (projection, what) in [
        (
            "networks:\n  default:\n    external: true\n",
            "a top-level network taken from outside this file, which points every service \
             at something this stack neither creates nor owns",
        ),
        (
            "networks:\n  default:\n    attachable: true\n",
            "a top-level network any container may join, which is what the message beside \
             the list names",
        ),
    ] {
        assert!(
            the_top_level_network_keys_in(projection).is_err(),
            "{what} is accepted"
        );
    }
}

/// Every top-level network key in a projection, judged.
///
/// **The one vocabulary in this file that only the pristine stack ever drove.**
/// `PER_SERVICE`, `AT_THE_TOP`, `IN_THE_ENVIRONMENT` and `HOW_EACH_SERVICE_ANSWERS`
/// each carry a count equality or a reverse loop, and
/// [`a_service_joins_only_a_declared_network`] is driven by written projections.
/// This one was asked only about the stack as it ships, where every key is
/// allowed — so `external` and `attachable` were added back to the list and all
/// eight tests, and the whole gate, stayed green. That is the regression the
/// rule's own paragraph records as already measured once.
fn the_top_level_network_keys_in(resolved: &str) -> Result<usize, String> {
    let mut judged = 0_usize;
    for (path, key, _) in by_path(resolved) {
        if path.len() == 2 && path[0] == "networks" {
            a_top_level_network_key(&path, &key)?;
            judged += 1;
        }
    }
    Ok(judged)
}

/// Every key at depth three or deeper, decided by the block it sits in.
///
/// **The rule this replaces was a pure exclusion, and it excluded nothing.**
/// `DECIDED_ELSEWHERE` read `path[0] == top && path.len() >= depth`, with 4
/// under `services` and 3 under `volumes` and `networks`, so it accepted every
/// non-item key at those depths without looking at it. Counted over the
/// pristine projection, the walk reads 4 keys at depth 0, 5 at depth 1, 17 at
/// depth 2, 28 at depth 3 and **none below any of the three thresholds** — all
/// three rows named a region this stack does not have, and the only thing they
/// could do was admit whatever arrived there. Measured, each of these passing
/// all 8 tests in this target:
///
/// ```text
/// aliases: [db]   under a service's `networks: default:`   -> depth 4
/// an x- block three deep under the same                    -> depths 4, 5, 6
/// labels: on the top-level network                         -> depth 3
/// ```
///
/// The first is one of the two escapes [`AT_DEPTH_THREE`]'s own docstring says
/// it closed, and the third is a region [`a_top_level_network_key`] allows a
/// key into without deciding anything about what the key holds.
///
/// So depth no longer admits anything. A key at three is decided by the block
/// it sits in, and below three this repository names no block at all: the whole
/// resolved stack is three keys deep in every branch it has, so a key further
/// down is inside a shape nobody has read. That is a finding until whoever puts
/// it there writes down what the block is for.
fn a_key_at_depth_three_or_deeper(path: &[String], key: &str) {
    assert!(
        path.len() == 3,
        "`docker-compose.yml` puts `{key}` under {path:?}, which is deeper than three — \
         and three is as deep as anything here names a block. Nothing in the resolved \
         stack goes below it, so this key sits in a shape no rule has read: name the \
         block it is in with what it is for, or take the key out"
    );
    assert!(
        AT_DEPTH_THREE.iter().any(|(top, block, keys)| {
            path[0] == *top && path[2] == *block && (keys.is_empty() || keys.contains(&key))
        }),
        "`docker-compose.yml` puts `{key}` inside {path:?}, and nothing here has decided \
         about that block or about that key in it. Add it with the reason it is allowed, \
         or take it out"
    );
}

/// The compose file uses no key this repository has not decided about.
///
/// **It was judged by a needle census, so every key that is not a needle was
/// free.** The test above counts occurrences of six fixed strings and refuses
/// two — each one a line somebody was bitten by — and there was no "and nothing
/// else" rule anywhere. Measured, four keys added to the file with the whole
/// gate green:
///
/// ```text
/// POSTGRES_HOST_AUTH_METHOD: trust
///   PGPASSWORD=wrong-password-entirely psql -h 127.0.0.1 -U noctua -d noctua
///   ->  authenticated with a wrong password
///
/// privileged: true   user: "0:0"   cap_add: [SYS_ADMIN]   - /etc:/host-etc
///   docker inspect  ->  true  0:0  [CAP_SYS_ADMIN]  /etc->/host-etc
///
/// cargo xtask check  ->  check passed
/// ```
///
/// The stack a developer is told to run, certified `ok` as a root, privileged
/// container with the host's `/etc` inside it and a database that accepts any
/// password. None of it is exotic; each is one line.
///
/// So the vocabulary is closed, in the shape `SKIP`/`COVERED` already uses one
/// directory over: a key is listed here with the reason it is allowed, or it is
/// a finding. That turns "somebody added a key" from silence into a two-place
/// edit, and the second place is here, where whoever adds it has to write down
/// what it is for.
#[test]

fn the_compose_file_uses_only_keys_this_repository_has_decided_about() {
    // Top level. `name:` is deliberately absent and `AGENTS.md` says why.
    const AT_THE_TOP: [(&str, &str); 4] = [
        ("services", "the two services this stage runs"),
        ("volumes", "the named volumes their data lives in"),
        // The two `docker compose config` adds. `name` is the project, derived
        // from the directory because the file deliberately declares none; the
        // default network is what compose gives any stack that asks for no
        // other.
        (
            "name",
            "the compose project, which the file deliberately does not set",
        ),
        ("networks", "the default network compose supplies"),
    ];

    // **The stack docker resolves, not the one file this repository writes.**
    // Docker merges up to eight filenames — `gates.rs`'s `COMPOSE_FILES` already
    // names them all — and the vocabulary was applied to `docker-compose.yml`
    // alone. Measured, an untracked override file of the kind compose reads without being asked, carrying
    // `privileged: true`, `user: root`, `cap_add: [SYS_ADMIN]`, `pid: host`,
    // `volumes: ["/:/host"]` and `POSTGRES_HOST_AUTH_METHOD: trust`:
    // `docker compose config` showed every one of them and this test printed
    // `6 passed`.
    //
    // Resolving also normalises the *shapes* the file-based walk could not read:
    // a sequence entry written with the dash on its own line, a flow collection
    // like `command: ["sh", "-c", …]` whose contents were never descended into,
    // and the short volume form. All three were measured getting a docker socket
    // and an arbitrary root command past this vocabulary; `docker compose
    // config` emits canonical block YAML, so the same walk now sees them.
    if !the_stack_was_asked() {
        return;
    }
    let compose = resolved_stack();
    let mut top = 0_usize;
    let mut service = 0_usize;
    let mut named_volumes = 0_usize;
    let mut networks = 0_usize;
    let mut network = 0_usize;
    let mut services = 0_usize;
    let mut volumes = 0_usize;
    for (path, key, is_item) in by_path(&compose) {
        if is_item {
            continue;
        }
        match path.len() {
            0 => {
                assert!(
                    AT_THE_TOP.iter().any(|(allowed, _)| *allowed == key),
                    "`docker-compose.yml` uses the top-level key `{key}`, which nothing \
                     here has decided about. Add it to this list with the reason it is \
                     allowed, or take it out"
                );
                top += 1;
            }
            // A service's own name, whatever it is.
            1 if path[0] == "services" => services += 1,
            // **A named volume's own keys.** The resolver gives each one a
            // `name:` and nothing else; `driver_opts` is where a "named" volume
            // stops being one. Measured, and accepted by everything else in
            // this file:
            //
            // ```yaml
            // volumes:
            //   postgres-data:
            //     driver: local
            //     driver_opts: {type: none, device: /, o: bind}
            // ```
            //
            // — the host's root filesystem, mounted into the database under a
            // name that reads like storage this stack owns.
            2 if path[0] == "volumes" => {
                assert_eq!(
                    key, "name",
                    "the volume `{}` carries `{key}`, and a named volume compose owns has \
                     only the name compose gives it. `driver_opts` with `type: none` and \
                     a `device` is a bind mount wearing a volume's name",
                    path[1]
                );
                volumes += 1;
            }
            2 if path[0] == "services" => {
                a_service_key(&path, &key);
                service += 1;
            }
            1 if path[0] == "volumes" => named_volumes += 1,
            1 if path[0] == "networks" => networks += 1,
            2 if path[0] == "networks" => {
                a_top_level_network_key(&path, &key)
                    .unwrap_or_else(|why| panic!("the stack this repository ships: {why}"));
                network += 1;
            }
            _ => a_key_at_depth_three_or_deeper(&path, &key),
        }
    }
    the_regions_are_all_counted(networks, network, named_volumes);
    // **And the rule is driven, because docker will not let it fail.** Asked for
    // a projection where a service joins a network the stack does not declare,
    // `docker compose config` refuses to produce one at all — so over the
    // resolved stack this assertion cannot be false, and the two tests that read
    // that projection fail first. Measured: with such a service planted,
    // `config` exited 1 and this rule was never reached. Read against the
    // resolved stack it is a floor on what the walk found; the negative cases
    // below are what make it a rule.
    let joined = a_service_joins_only_a_declared_network(&compose)
        .unwrap_or_else(|why| panic!("the stack this repository ships: {why}"));
    the_network_membership_rule_is_driven();
    the_network_vocabulary_is_driven(&compose);
    // The floor the rule had none of: `joined` and `declared` were both read and
    // neither was counted, so a walk that stopped producing
    // `["services", <service>, "networks"]` would have reported nothing wrong.
    // Two, measured: this file writes no `networks:` block, and docker's
    // projection still puts each of the two services on `default`. That is the
    // number the rule is judging, and a walk that stopped producing those paths
    // would report `ok` without it.
    assert_eq!(
        joined, 2,
        "the walk found {joined} service-to-network joining(s) in the resolved stack, and \
         this stack has two services on docker's `default` — so either a service left or \
         the walk stopped producing the paths this rule reads"
    );
    what_a_service_is_handed(&compose);

    the_walk_entered_the_whole_stack(top, services, volumes, service, AT_THE_TOP.len());

    for (key, why) in PER_SERVICE {
        assert!(
            compose.contains(&format!("{key}:")),
            "`{key}` is written down as allowed because {why}, and the file no longer \
             uses it: a vocabulary with entries nothing uses stops being a description"
        );
    }
}

#[test]
fn a_size_is_read_as_the_bytes_it_means() {
    // The reader itself, or the comparison above could pass by both sides being
    // `None`.
    assert_eq!(bytes("8GiB"), Some(8_589_934_592));
    assert_eq!(bytes("1MiB"), Some(1_048_576));
    // **`KiB` was a row nothing read.** Every other scale here is asserted and
    // this one was not, so `("KiB", 1024)` could become `("KiB", 1000)` in this
    // copy with the whole target green — measured, `8 passed`. A table row no
    // assertion reaches is a row that is not part of the rule.
    assert_eq!(bytes("2KiB"), Some(2048));
    assert_eq!(bytes("512B"), Some(512));
    assert_eq!(bytes("1024"), Some(1024));
    assert_eq!(bytes("eight gibibytes"), None);
    // **And the overflow, which this copy's own comment describes and drove
    // nowhere.** `bytes` is written twice — here and in `xtask/src/store.rs` —
    // and only that copy's test had the case, so `checked_mul` could become a
    // `wrapping_mul` here with everything green. What that admits is the attack
    // the paragraph beside this reader spells out: a bound congruent to the
    // declared one modulo 2^64 wraps to exactly the right number, so the
    // comparison agrees while the container runs with something else. This file
    // is outside `xtask/mutants.py`, so the clause is held by this line alone.
    // Round sixty-six.
    assert_eq!(bytes("17179869192GiB"), None);
    assert_eq!(bytes("999999999999GiB"), None);

    // And the flag reader, on the shape the file actually holds.
    let compose =
        "    command:\n      - \"--storageDataPath=/x\"\n      - \"--retentionPeriod=30d\"\n";
    assert_eq!(only(compose, "--retentionPeriod"), Ok("30d".to_owned()));
    assert!(only(compose, "--nope").is_err());

    // **The hole, not just the rule.** A flag given twice is what defeated the
    // comparison above, and it defeats it silently: the reader agreed with the
    // catalog while the container ran with the second value.
    let twice = format!("{compose}      - \"--retentionPeriod=1d\"\n");
    assert_eq!(flags(&twice, "--retentionPeriod"), ["30d", "1d"]);
    let refused = only(&twice, "--retentionPeriod").expect_err("a flag given twice is refused");
    assert!(
        refused.contains("given 2 times") && refused.contains("1d"),
        "the refusal does not name what it found: {refused}"
    );
}
