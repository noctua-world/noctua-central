# Convenience aliases. Every one is a `cargo xtask` verb underneath — `just` is
# never the only path, so a fresh clone with nothing but rustup can still do
# everything.
#
# **The last comment line above a recipe is its description, and only that one.**
# `just --list` prints that one line and no other, so a paragraph written above a
# recipe reaches a reader as whichever clause happens to be last: with the
# explanation last, `check` — THE gate — is listed as "`cargo install` for
# cargo-deny, and two builds it diffs", which names what the workflow runs
# *beside* the gate rather than the gate. So the explanation goes first and the
# last line is a whole sentence that stands alone;
# `the_listing_reads_as_sentences` holds it.

# **Windows runs a recipe in PowerShell**, which every Windows machine has, and
# not in `sh`, which `just` asks for by default and Windows does not ship. Each
# recipe is a `cargo xtask` verb, the listing, or a script that names `sh` itself —
# and the checkout script refuses to run under the `sh` Git ships there, before it
# derives anything, because every guard it has spends a path as POSIX does.
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

default:
    @just --list

# Compile the catalog and write the crate that carries it.
build:
    cargo xtask build

# CI runs this, `cargo xtask dev` for the stack it needs, the `cargo install`
# for cargo-deny, and two builds it diffs.
# THE gate: every check this repository has, against a running stack.
check:
    cargo xtask check

# CI has never run (`NOCTUA.md §5.3`), so this is the only account of that shape
# there is, and it is a command rather than a paragraph because a measurement
# nobody can reproduce decays into prose. It brings up a stack of the copy's own,
# on two other ports, the way CI's `cargo xtask dev` step does, and takes it
# down again.
# Measure what `cargo xtask check` does in a checkout and count what it skips.
checkout-shape:
    sh xtask/the-checkout-shape.sh

# The nine left out are: corroborating the migrations against a database;
# reading where `pg_temp` sits in the migrated functions' paths;
# the journal's own protections, which the database holds and no file here does;
# the bounds only the running log store can state; how a database sorts; the
# journal's PostgreSQL tests; fetching the advisory database; comparing each
# pinned digest against what its tag resolves to; and the stack's own tests,
# which ask docker what it resolves rather than reading one compose file. Nor
# does it run `cargo fetch --locked`, which a run without the flag does first,
# and every cargo it starts is told to stay off the network, so every package the
# lockfile names must already be on the machine. CI never passes this recipe.
# The same gate without the nine steps that need the stack or the network, its cargo kept offline.
check-offline:
    cargo xtask check --offline

# Bring up PostgreSQL and the central log store, and apply the migrations.
dev:
    cargo xtask dev

# Regenerate the self-documentation surfaces under docs/.
docs:
    cargo xtask docs

# A flag after the channel is handed on: `just release staging --dry-run`.
# Release a channel: its version and changelog, computed from the history.
release channel *flags:
    cargo xtask release --channel {{channel}} {{flags}}

# The suite's own fixtures can leave directories in `/tmp` — 9,250 in one
# month, measured, with nothing counting them — and `xtask/src/check.rs` says why
# no gate can hold this: the temporary directory is shared with every other process on the
# machine, so a step inside `check` would fail for somebody else's mess. What a
# recipe can do is watch the difference across one run, which is attributable
# and reproducible. A guard that removes on `Drop` is what makes the answer
# zero; `THE_SCRATCHES_WITH_NO_GUARD` names the tests that still lack one.
# Run the whole suite and refuse any directory it leaves behind in /tmp.
leaves-nothing:
    sh xtask/leaves-nothing.sh
