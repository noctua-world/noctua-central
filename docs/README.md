# noctua-central

The Noctua platform: the apps, the backend services and the local kernel, in one
cargo workspace.

## Before you clone

This repository lives inside **`noctua-workspace`**, beside the other Noctua
repositories, and the tooling assumes it. Clone it only inside
noctua-workspace/repos/:

```sh
cd path/to/noctua-workspace/repos
git clone https://github.com/noctua-world/noctua-central.git
```

**Start your agents at the workspace root, not here.** From inside this repository
the others are invisible, so an agent cannot read the shared colour contract, edit a
design-system component, or consult the master technical reference — it will guess
instead, and the guesses will be wrong. Every repository here carries a probe that
detects it and says so.

## Requirements

- **Rust**, via rustup. `rust-toolchain.toml` pins the channel, the components
  and the `wasm32-unknown-unknown` target the plugins are built for, so `rustup
  show` installs exactly what this repository wants and no *version* is written
  down twice. CI installs from the same file, with `rustup toolchain install` and
  no toolchain named, so no workflow names a toolchain of its own.
- **Docker**, for the local stack — PostgreSQL and the central log store.
- **A user service manager**, which the gate's own tests register short-lived scratch
  services and a relocated development daemon with, under names that carry this
  checkout, and remove, and which `cargo xtask dev --install` registers your development
  daemon with: on Linux, systemd's, running for your user, so that `systemctl --user`
  answers — the tests write their units only in `$XDG_RUNTIME_DIR/systemd/user`; on
  macOS, a graphical login for the user running the gate, whose launchd domain
  `gui/<uid>` the agents are bootstrapped into — a login over SSH alone has none, and the
  gate fails there; on Windows, Task Scheduler, which is always there, and where a test's
  task that a run killed half way leaves starts at each logon until the next run sweeps
  it. On Linux, for the daemon to start at boot rather than at your first login,
  **lingering** for your user: `cargo xtask dev --install` enables it where logind lets
  you, and otherwise prints the command — `loginctl enable-linger <name>` — and, for
  NixOS, the option `users.users.<name>.linger = true;`; a NixOS configuration that sets
  it to `false` takes lingering away at each switch. No test enables it.
- **cargo-deny**, for the dependency audit the gate runs:
  `cargo install --locked cargo-deny`.

## Quick start

```sh
cargo xtask dev      # PostgreSQL and the central log store, on loopback
cargo xtask dev --install   # this checkout as its development channel
cargo xtask build    # compile the catalog into the crate that carries it
cargo xtask build --plugins   # every plugin, built for wasm32 as a component
cargo xtask check    # the whole gate. CI runs this, `cargo xtask dev` for the
                     # stack it needs, the install for cargo-deny, and one more
                     # job that builds twice and diffs — the only
                     # cross-process determinism check there is
cargo xtask docs     # regenerate docs/ from the registries
cargo xtask release --channel staging --dry-run   # what a channel would release
cargo xtask release --channel staging   # gate, then write its version and changelog
```

`cargo xtask dev --down` removes the stack and its data, which makes the next `dev`
the same test as the first. `just` mirrors every verb (`just check`, `just build`)
and is never the only path: a fresh clone with nothing but rustup can do everything.

**The stack's ports are this checkout's.** Each checkout's stack takes a pair of its
own, derived from the checkout's path, so two checkouts' stacks run at once on one
machine. `cargo xtask dev` prints the pair, and the `DATABASE_URL` it makes;
`cargo xtask check` derives the same, and a tool outside `cargo xtask` — a
`docker compose up` typed by hand, which refuses without them, or a PostgreSQL suite
run alone — is told them by exporting what `dev` prints. `POSTGRES_PORT` and `LOGS_PORT`
move a port, and a port `dev` cannot bind that its own stack does not hold — something else
listening there, or one below 1024, which a user without the privilege may not bind — fails
`dev` with the reason, naming the one to set.

**This checkout as a channel you run.** `cargo xtask dev --install` installs the
checkout as its development channel, on Linux, macOS and Windows alike, stack or no
stack: it builds the CLI, the daemon and the plugins, copies the CLI and the daemon out of
the target directory cargo built them in — `target/`, or wherever `CARGO_TARGET_DIR` or
`build.target-dir` puts it — and the plugins out of the checkout's own `target/`, from its
`plugins` folder, where the plugin build writes them whatever the environment says, into
the install under your base directories — the XDG ones on Linux, the home's Library on macOS,
`%LOCALAPPDATA%` and `%APPDATA%` on Windows — with its manifest, a device, the test world
(`policy/world.json`, as `ana` in `home`) and the plugins, and stops a daemon an earlier
install left running first — once it has reached your service manager: run from an
environment that does not, it stops nothing, installs nothing, and says so. **Then it
registers the daemon with your service manager** — a systemd user unit in your
configuration directory's `systemd` folder, a launch agent in your Library's
`LaunchAgents`, a logon task — which restores it when it dies and starts it at each login,
and starts it, unless the profile carries the stop mark — which your `stop` writes, and
so does an install or a `dev --uninstall` that failed after stopping the daemon; one that
finishes takes away the mark its own stop wrote, and keeps yours — and then the daemon
stays down until `noctua-dev-<checkout> start`. **It does not
change `PATH`**: the CLI is `noctua-dev-<checkout>`, at the path the install prints — run
it by that path or a link you make — and `noctua-dev-<checkout> start` starts the daemon,
`stop` stops it and keeps it down whatever restores it, until `start`, and `status` says
how it is. `cargo xtask dev --uninstall` stops the daemon, unregisters it from the
service manager and removes the binaries and the mark its own stop wrote, and keeps the
profile, your own `stop` with it; on Linux, once your user
manager has read the unit's link at a login or a boot, unregistering it asks for one
`systemctl --user daemon-reload`, the only reload anything here asks for. Run from an
environment that does not reach the service manager — no `systemctl` on `PATH` by an
absolute entry, no `SystemRoot` — it removes nothing, and says so.

## What the gate checks, and where

`cargo xtask check` passes on a checkout of this repository **alone**, **against a
migrated database**: the steps that read the documents living in the workspace above
(`NOCTUA.md §3.1`), and the one resolving the design system's sibling manifest, each
announce what they could not do, and the verdict line names every step that did not
run. `just checkout-shape` runs that as a command: it copies this repository into a
directory with no workspace above it and asserts the announcements are exactly five.
That holds on a machine whose image store holds the two pinned tags; a CI runner,
which pulls neither, makes two more and still passes, and the recipe's own message
names the `docker pull` commands that move it. `warn` lines and no failures is the
intended state there; inside the workspace there are none.

A checkout has no workspace, so no `§` citation is checked in CI at all. The tests
that need the workspace print `skipped:` and return, which counts as passing, so the
test total does not move and only the clause beside it says so.

**An empty database fails.** The migration corroboration, where `pg_temp` sits in
the migrated functions' paths, and the journal's protections each fail and name
`cargo xtask dev`, so no green run is compatible with the journal's forty-four protections never having
been read. A database that answers and holds nothing is a database to migrate.

`cargo xtask check` first runs `cargo fetch --locked`, so a fresh clone holds every
package the lockfile names before any step asks cargo for the graph. Under `--offline` the
fetch does not run and every cargo the gate starts is told to stay off the network, so a
package missing from the machine fails instead of being downloaded, and is named with that
command.

`cargo xtask check` judges the history as well: every commit reachable from `HEAD` has a
conventional subject and credits no agent, and no blob ever reachable is above the size
limit unless its path is listed (`NOCTUA.md §22.5`). It needs the whole history, so a
shallow clone is refused, and a tree with no `.git` says it was not judged.

A push to `staging` or `main` releases that channel: `.github/workflows/release.yml`
runs the gate on the pushed commit, the stack brought up, in a job that holds no secret
and no token; then, in a job that waits for it and enters an environment only that branch
can enter, it builds the release tool, mints the release App's token and runs
`cargo xtask release --channel <branch> --publish --after-the-gate-job`, which runs no
gate of its own. The gate holds that workflow to its two jobs, its steps, its keys and its
expressions; refuses the token, an environment, a secret and a step's output in every
other file GitHub runs (the workflows under `.github/` and a composite action's own file),
and a publish or the release's gate stood aside in every other file CI reaches, the
justfile and the scripts a step runs among them; and pins every action a workflow here
uses to a commit, and every install to one crate, locked, at an exact version. Those rules read a file a line at a time, so the gate also refuses a
line ended anywhere but at a line feed, or spaced with anything but a space or a tab, in
a file CI reaches, and, in a file GitHub runs, a key not written out before its colon, a
quote or a bracket not closed on the line that opens it, and a `#` those rules would take
for a comment where YAML reads on. They read a file GitHub runs as GitHub does, a step's
`name:` as a label, so the gate refuses a word that names one where a step finds a script
to run.

`cargo xtask check --offline` skips **nine** things — the advisory database, which is
fetched (the audit's licence, ban and source checks read only the lockfile and still
run); the PostgreSQL tests; the corroboration of
`migrations/` against what a database has actually applied; where `pg_temp` sits in
the migrated functions' search paths; the journal's own protections, which are
triggers and constraints held by the database and not by any file here; the bounds
the running log store reports, which only the store itself can state; how the
database sorts and encodes, which `POSTGRES_INITDB_ARGS` decides once and only the
database remembers; the comparison of each pinned digest against what its tag
resolves to, which needs the image pulled; and the stack's own tests, which ask
docker what it resolves rather than reading one compose file, and panic where docker
is absent. Each prints a `warn` line. It is a fast loop while editing, and never
what CI runs.

`OFFLINE_SKIPS` in `xtask/src/check.rs` holds that number, and
`the_documents_agree_about_what_offline_skips` holds this list to it.

On macOS and Windows no stack runs (`NOCTUA.md §22.2`), so every one of those steps but
the advisory database says it is not applicable on that system, by name, and that the
Linux job runs it; the verdict names each one, and on Linux it holds the run to none.

## What is here

| Path | What it is |
|---|---|
| `catalog/` | The declarative registries every part of the platform reads: error codes, notifications, emails, limits, commands, log events, the capabilities a plugin can be granted, and both locales |
| `apps/` | The binaries a person's machine runs, each a library and a thin binary. `daemon` is the daemon a channel runs: it serves its kernel at its endpoint, and `noctua-daemon` with no argument is it. `cli` is `noctua`, the command line a channel is reached through: every command read against its schema, the daemon started, stopped and asked how it is, and the installer `cargo xtask dev --install` runs |
| `crates/` | The libraries. `nc-catalog` is generated from `catalog/` and committed; `nc-catalog-compiler` is what generates it; `nc-kernel` is the local kernel, which hosts the plugins and authorizes and journals every command they run |
| `docs/` | This manual, and every error code, notification, email, limit, command, log event and capability, as Markdown — the pages generated from `catalog/` by `cargo xtask docs` and committed. Editing a page by hand fails the gate; edit the registry instead. This file is the one written by hand |
| `migrations/` | The schema, forward-only and applied in numeric order by `cargo xtask dev` |
| `local-migrations/` | The local app database's schema — SQLite, on the user's own machine — applied by `crates/nc-local` when it opens the file, and never mixed with `migrations/` |
| `plugins/` | The WASM plugins, one directory each. `reference` is the component the kernel's suites load, and `cargo xtask build --plugins` builds every plugin for `wasm32` |
| `policy/` | The Cedar pack: `core/` holds the invariants no pack may narrow, `packs/` the ones that may |
| `docker-compose.yml` | The local stack. Both services bind to loopback only |
| `xtask/` | The five verbs |

Of the apps and the services `NOCTUA.md §10.2` describes, the daemon and the CLI are the
ones here; each of the rest arrives with the work that fills it. The policy pack and the migrations are here and gated: `cargo xtask check`
validates the pack, runs its cases, and reads every migration, the local database's
included — those it also applies to a scratch SQLite file and holds to the list
`nc-local` checks every database against.

`AGENTS.md` is the operating manual for working here.

## Branches and releases

`dev` is the default branch, and every feature branch's pull request targets it.
`staging` and `main` are the release channels, each reached by a pull request from the
branch below it, and a merge into either publishes that channel's release on GitHub:
the version and the changelog are computed from the conventional commits, and nothing
else chooses them (`NOCTUA.md §22`). Merges are merge commits, whose title and body are
left as GitHub fills them from the pull request and never edited when merging; every
commit is signed; and no branch's history is rewritten.
`AGENTS.md` says how each of those is held.
