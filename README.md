# noctua-central

The Noctua platform: the apps, the backend services and the local kernel, in one
cargo workspace.

## Before you clone

This repository lives inside **`noctua-workspace`**, beside the other Noctua
repositories, and the tooling assumes it. Clone it only inside
noctua-workspace/repos/:

```sh
cd path/to/noctua-workspace/repos
```

**Start your agents at the workspace root, not here.** From inside this repository
the others are invisible, so an agent cannot read the shared colour contract, edit a
design-system component, or consult the master technical reference — it will guess
instead, and the guesses will be wrong. Every repository here carries a probe that
detects it and says so.

There is nothing to clone yet: this repository gets its git repository in Stage 12
of the plan, and has none until then on purpose (see the last section).

## Requirements

- **Rust**, via rustup. `rust-toolchain.toml` pins the channel, the components
  and the `wasm32-unknown-unknown` target the plugins are built for, so `rustup
  show` installs exactly what this repository wants and no *version* is written
  down twice. CI names the same components and target again on the line that
  installs the toolchain, which is what a GitHub action needs to be told.
- **Docker**, for the local stack — PostgreSQL and the central log store.
- **cargo-deny**, for the dependency audit the gate runs:
  `cargo install --locked cargo-deny`.

## Quick start

```sh
cargo xtask dev      # PostgreSQL and the central log store, on loopback
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
| `crates/` | The libraries. `nc-catalog` is generated from `catalog/` and committed; `nc-catalog-compiler` is what generates it; `nc-kernel` is the local kernel, which hosts the plugins and authorizes and journals every command they run |
| `docs/` | Every error code, notification, email, limit, command, log event and capability, as Markdown — generated from `catalog/` by `cargo xtask docs` and committed. Editing a page by hand fails the gate; edit the registry instead |
| `migrations/` | The schema, forward-only and applied in numeric order by `cargo xtask dev` |
| `local-migrations/` | The local app database's schema — SQLite, on the user's own machine — applied by `crates/nc-local` when it opens the file, and never mixed with `migrations/` |
| `plugins/` | The WASM plugins, one directory each. `reference` is the component the kernel's suites load, and `cargo xtask build --plugins` builds every plugin for `wasm32` |
| `policy/` | The Cedar pack: `core/` holds the invariants no pack may narrow, `packs/` the ones that may |
| `docker-compose.yml` | The local stack. Both services bind to loopback only |
| `xtask/` | The five verbs |

The apps and the services `NOCTUA.md §10.2` describes are not here yet — the daemon
that runs the kernel among them, which is Stage 13's; each arrives with the work that
fills it. The policy pack and the migrations are here and gated: `cargo xtask check`
validates the pack, runs its cases, and reads every migration, the local database's
included — those it also applies to a scratch SQLite file and holds to the list
`nc-local` checks every database against.

`AGENTS.md` is the operating manual for working here.

## History

This repository has no git history yet, on purpose (`NOCTUA.md §5.3`): it takes its
initial commit in Stage 12 of the plan. Until then there is no history to read a
change against and nothing to restore from, so anything reusable belongs in one of
the other Noctua repositories rather than here.
