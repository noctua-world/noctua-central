# noctua-central — Agent Guidelines

> ## Everything you write to disk is in English.
>
> Code, comments, identifiers, documentation, commit messages, CLI help, log messages,
> error strings, test names, `AGENTS.md`, `README.md` — all of it, always, **whatever
> language the developer writes to you in.** You may *reply* in that language; you may not
> *produce artifacts* in it.
>
> **User-facing strings are the one exception**: they are i18n resources and exist in
> **both** English and Brazilian Portuguese, in `catalog/i18n/{en,pt-BR}.toml`. The keys
> and the code around them are English; the values exist in both, and a key missing from
> either locale fails the build (`NOCTUA.md §10.9`). That applies to **backend responses
> too** — a response carries a message key and parameters, never a pre-rendered English
> sentence.

This is the operating manual for `noctua-central`, and every agent working here reads it
before anything else. It holds what binds in this repository and nowhere else: the guard,
the map, the verbs, the steps of a change that are not where you would look, the
invariants, what each guarantee covers and leaves open, how the gates are built, and the
known gaps in the tree's own instruments. What the platform is and by what rules is
`NOCTUA.md` (`§10`, `§11`, `§14`, `§15`, `§19`); a detail a module header already holds is
pointed to here, not repeated. The developer's private notes may sit beside this file;
**Local notes**, at the end, says where.

## Before anything else: are you in the right place?

This repository lives inside **`noctua-workspace`**, and agents must be started at the
workspace root — not here. From inside this repository the other repositories are
invisible, so an agent cannot read the shared color contract, edit a design-system
component, or consult the master technical reference.

Run this **once, at the start of the session, before any other work.** Do not re-run it
per task.

```sh
d=$PWD; found=
while :; do
  [ -f "$d/.noctua-workspace" ] && { found=$d; break; }
  [ "$d" = / ] && break
  d=$(dirname "$d")
done
if [ -z "$found" ]; then
  echo "NOCTUA_WS=outside"
elif [ "$found" = "$PWD" ]; then
  echo "NOCTUA_WS=root"
else
  case $PWD in
    "$found"/repos/*) echo "NOCTUA_WS=inside-repo:$found" ;;
    *)                echo "NOCTUA_WS=inside-workspace:$found" ;;
  esac
fi
```

- **`root`** → proceed silently. Print nothing.
- **`inside-repo:…`** → print **ALARM A** below verbatim, stop, and ask whether to
  continue or abort. Touch nothing until answered.
- **`outside`** → print **ALARM B** below verbatim, and stop the same way.
- **`inside-workspace:…`** → not an alarm. Print the one-line correction, then
  continue, resolving paths against the workspace root.

**ALARM A — wrong starting directory:**

```
🚨🚨🚨 STOP — WRONG DIRECTORY 🚨🚨🚨

⛔ THIS AGENT WAS NOT STARTED FROM THE NOCTUA WORKSPACE. ⛔

YOU ARE INSIDE A SINGLE REPOSITORY. FROM HERE I CANNOT SEE THE OTHER
NOCTUA REPOSITORIES, SO I WILL NOT BE ABLE TO READ THE SHARED COLOR
TOKENS, EDIT A DESIGN-SYSTEM COMPONENT, OR CHECK THE MASTER TECHNICAL
REFERENCE. I WILL GUESS, AND MY GUESSES WILL BE WRONG. 🙈

✅ WHAT TO DO: STOP WHAT YOU ARE DOING, CLOSE THIS AGENT, AND REOPEN IT AT:

    THE WORKSPACE ROOT — THE DIRECTORY HOLDING `.noctua-workspace`,
    WHICH THE PROBE ABOVE JUST FOUND AND NAMED

❓ DO YOU WANT ME TO CONTINUE ANYWAY, OR ABORT THE CURRENT WORK?
   I WILL NOT TOUCH ANYTHING UNTIL YOU ANSWER. 🛑
```

**ALARM B — repository outside the workspace:**

```
🚨🚨🚨 STOP — REPOSITORY IN THE WRONG PLACE 🚨🚨🚨

⛔ THIS REPOSITORY IS NOT INSIDE THE NOCTUA WORKSPACE. ⛔

I SEARCHED EVERY PARENT DIRECTORY AND FOUND NO `.noctua-workspace`
MARKER. THIS CLONE IS SOMEWHERE IT SHOULD NOT BE, SO NOTHING SHARED —
COLOR TOKENS, THE DESIGN SYSTEM, THE MASTER TECHNICAL REFERENCE — IS
REACHABLE FROM HERE. 🙈

✅ WHAT TO DO: STOP, AND RE-CLONE THIS REPOSITORY INSIDE:

    THE `repos/` DIRECTORY OF YOUR NOCTUA WORKSPACE — THE ONE
    DIRECTORY THAT HOLDS THE `.noctua-workspace` MARKER

   THEN REOPEN THE AGENT AT THE WORKSPACE ROOT, NOT INSIDE THE REPOSITORY.

❓ DO YOU WANT ME TO CONTINUE ANYWAY, OR ABORT THE CURRENT WORK?
   I WILL NOT TOUCH ANYTHING UNTIL YOU ANSWER. 🛑
```

**The correction (not an alarm):**

```
⚠️ Started in a workspace subdirectory, not at the workspace root.
   Working from <path> instead. Everything is reachable, so continuing.
```

These texts are authored once, in `NOCTUA.md §3.6`, and copied here verbatim **because the
case they fire in is the one where `NOCTUA.md` is unreachable**. If one changes, change it
there and propagate: `the_alarms_are_the_ones_that_were_copied` pins this copy by digest,
and `the_texts_this_repository_copies_are_the_ones_the_workspace_authored` holds it to the
source wherever the workspace is above.

## What this is

**The platform**: the monorepo that holds the local kernel and will hold the apps and the
backend services — everything `NOCTUA.md §1` describes. What is in it today:

- **The base every later stage stands on**: the catalog compiler and its generated
  registries, identifiers, the hash-chained journal, the central log's type wall, the one
  pruner, the Cedar policy packs, `cargo xtask` and the local stack.
- **The local kernel** (`crates/nc-kernel`): the host of the plugin world. It runs each
  WASM plugin in a store of its own under memory, fuel, epoch and host-call limits, kills
  and restarts one that crosses them, answers `denied` to a host function a plugin was not
  granted, and puts every command — a plugin's or its own — through one decision point
  that authorizes it with Cedar before it runs, journals it before it answers, and keeps
  what it changes in a unit of work a person can undo and redo. Plugins reach blobs, the
  network, events, timers and surfaces only through the world, each judged by the kernel.
- **What the kernel stands on**: the one rule every declared command answers to
  (`crates/nc-schema`), the decision point it embeds (`crates/nc-authz`), the local
  app database its plugins' data is kept in (`crates/nc-local`, SQLite, built by
  `local-migrations/`) — the journal's store on the person's machine in the same file —
  and what makes that file and the blobs beside it one user's alone on each system
  (`crates/nc-platform`).
- **What a plugin is built from**: the world's guest half (`crates/nc-guest`) and the
  reference plugin the kernel is proven with (`plugins/reference`).
- **What every install is named by** (`crates/nc-channel`): the manifest an install names
  itself by, and the one function that derives from it every name the system sees
  (`NOCTUA.md §17.1`).
- **The channel a daemon is reached through** (`crates/nc-ipc`, over the endpoint
  `nc_platform::ipc` makes): frames of canonical JSON, a handshake in which each end
  proves it holds the channel's token without sending it, one request at a time, and a
  heartbeat (`NOCTUA.md §11.1`).
- **The daemon a channel runs** (`apps/daemon`): the install read beside its executable,
  the profile it keeps, the device it mints identifiers on, and the mark that keeps those
  identifiers unique across a crash and a clock moved back (`NOCTUA.md §16.2`). Its
  binary, `noctua-daemon`, answers `--initialize`, the installer's entry.

It is one repository on purpose (`NOCTUA.md §10.1`): those parts share the command
catalog, the policy pack, the generated types and the journal format. `NOCTUA.md §10.2`
says which directories are absent until a stage fills them — the daemon that runs the
kernel among them, which is Stage 13 of the plan.

`NOCTUA.md` is the authority on what this is and by what rules, and the plan
(`NOCTUA-PLAN.md`) says what to build and in what order. Two files under `reference/` —
`identity-and-authorization.md` and `noctua-kernel.wit` — are normative and **outrank
`NOCTUA.md`** (`NOCTUA.md §0.4`); when they disagree, the reference wins and `NOCTUA.md`
is what gets corrected.

## Branches, merges and releases

This repository is `github.com/noctua-world/noctua-central`, public (`NOCTUA.md §10.11`);
`NOCTUA.md §22` is the workflow, and what binds here is:

1. **Three branches, each changed by a pull request.** `dev` is the default branch and
   integrates work: every feature branch's pull request targets it. `staging` holds the
   release candidates and is reached by a pull request from `dev`; `main` is production
   and is reached by a pull request from `staging`. A feature branch is deleted when it
   merges. Nothing is pushed to the three directly but the release commit, which the
   release App makes on `staging` and `main` (item 6).
2. **Merge commits only, with the message GitHub fills in.** Squash and rebase are
   disabled. Whoever merges leaves a merge commit's title and body as GitHub fills them
   from the pull request's, and never edits them in the merge dialog or by `gh pr merge
   --subject/--body`, so **a pull request's title is a commit subject**: conventional.
   The history step judges the pull request's title and body at every run — an edit
   starts one — and every commit on every push. Auto-merge is allowed.
3. **Every commit is signed** — a commit by the developer who makes it, and a merge
   commit and the release commit by GitHub — and a failed signature is a stop, never a
   bypass (`NOCTUA.md §2.6`). **History is never rewritten** on a pushed branch: a
   mistake is fixed by a new commit.
4. **The rulesets** (rulesets, not classic branch protection): on `dev`, `staging` and
   `main`, a pull request with no approval required and the merge method alone, signed
   commits, no force-push, no deletion, and the checks `linux`, `macos`, `windows` and
   `deterministic` of `.github/workflows/ci.yml` passed before a merge. On the tags `v*`,
   creation, update and deletion are blocked. The release App is the one actor that
   bypasses any of them — on `staging`, `main` and the tags; `dev` has no bypass, and
   administrators do not bypass. **A fourth ruleset, "staging and main are never deleted
   or rewritten", holds deletion and force-push on `staging` and `main` with no bypass at
   all.** GitHub enforces every ruleset that applies, so the App bypasses the rules on
   those two branches except deletion and force-push, which nobody bypasses: GitHub's
   delete-branch-on-merge cannot remove `staging` or `main` after the App merges a pull
   request whose head each is (item 6). These are GitHub's settings, read with `gh api`
   and held by nothing in this tree.
5. **The release App**, `noctua-central-release`, belongs to the `noctua-world`
   organization and is installed on this repository alone. It writes contents and pull
   requests and reads metadata, and nothing more: with no workflows permission, GitHub
   refuses any commit of its that changes a workflow, and it receives no webhook events.
   Its client id is the variable `RELEASE_APP_CLIENT_ID` and its key the secret
   `RELEASE_APP_KEY`, held only in the environments `release-staging` and `release-main`,
   each of which admits a job from its own branch alone — so only a push `staging` or
   `main` already took can mint its token.
6. **A merge into `staging` or `main` is that channel's release**, and nothing else makes
   one: `.github/workflows/release.yml` runs the gate on the pushed commit in a job that
   holds no secret and no token, then, in a job that waits for it, mints the release
   App's token, and `cargo xtask release --channel <branch> --publish
   --after-the-gate-job` computes the version and the changelog from the history, and
   makes the release commit, the tag, the GitHub release and the pull request that
   carries it back to the branch below — `staging` to `dev`, `main` to `staging` — with
   auto-merge ("A release is computed from the history,
   and made on GitHub once"; "The release job is what `release.yml` says, and the App's
   token reaches its last step alone"). No tag and no release is made by hand.
   `cargo xtask release --channel staging --dry-run` says what a channel would release.
7. **`cargo xtask check` judges the history** — every commit reachable from `HEAD` has a
   conventional subject and credits no agent, and no blob ever reachable is above the
   size limit unless listed — and, under a pull request, its title and body ("The history
   says what each change is, credits no agent, and stays small").
8. **One thing writes here unasked**: a failing property test appends its case to
   `crates/*/tests/*.proptest-regressions`, which is committed on purpose so the case
   re-runs first — so a red run leaves the tree changed, and `git status` shows it.
9. **There is no `LICENSE` file and no `license` key**, by decision
   (`NOCTUA.md §10.12`). Do not add either.
10. **Nothing publishes to a registry.** Every crate is `publish = false`, and there
    is no registry name: what this repository publishes is its releases, on GitHub.

## Repository map

```
noctua-central/                     <!-- allow-reference: names `noctua-central/` as this repository, not as a path inside it — a clone may sit under any directory name, and a checkout named anything else has no such directory -->
├── AGENTS.md CLAUDE.md README.md   HAND-WRITTEN
├── Cargo.toml                      one cargo workspace
├── rust-toolchain.toml             the channel, components and target, not a version
├── .cargo/config.toml              the xtask alias — why there is no bootstrap step
├── justfile                        thin aliases over the five verbs
├── rustfmt.toml                    STOPS RUSTFMT'S CONFIG WALK LEAVING THIS TREE
├── clippy.toml                     the product names `doc_markdown` must not touch
├── deny.toml                       the advisory and licence audit (§15.5)
├── docker-compose.yml              the local stack: PostgreSQL and the log store
├── .env.example                    what the stack reads. NEVER copied to .env
├── .gitattributes                  every text file checked out with LF, on every system
├── .github/workflows/ci.yml        the gate on Linux, macOS and Windows, and the double build
├── .github/workflows/release.yml   the release, on a push to staging or main
├── catalog/                        HAND-WRITTEN — THE shared-core registries (§15.2)
│   ├── errors.toml                 every error this platform can produce (§15.1)
│   ├── errors.reserved.toml        APPEND-ONLY — which error codes have been used
│   ├── limits.toml                 every row bound, the pruner's order, the kernel's (§15.8)
│   ├── commands.toml               every command the catalog declares — the kernel's own — with its action and its selector (§11)
│   ├── notifications.toml          typed notification handles
│   ├── emails.toml                 typed email handles and their parameters
│   ├── log-events.toml             every event the central log can receive (§15.4)
│   ├── capabilities.toml           every capability a plugin can be granted (§11.3)
│   └── i18n/                       every user-facing string, both locales
├── policy/                         HAND-WRITTEN — the Cedar packs (§14.4)
│   ├── schema.cedarschema          reference/identity-and-authorization.md §4
│   ├── world.json                  the entities every policy case is about
│   ├── core/                       the invariants, and tests/ beside them
│   └── packs/{family,org,collective}/
├── migrations/                     HAND-WRITTEN — forward-only, numbered
├── local-migrations/               HAND-WRITTEN — the local SQLite database's, never mixed
├── docs/                           GENERATED AND COMMITTED — the surfaces of §15.3
│   └── README.md                   HAND-WRITTEN — how to work on this repository; the root README.md presents the project
├── crates/
│   ├── nc-catalog/                 GENERATED AND COMMITTED — the shared core
│   ├── nc-catalog-compiler/        HAND-WRITTEN — what reads catalog/ and writes it
│   ├── nc-canon/                   canonical JSON and hashing (§16.4)
│   │   └── canaries/               NOT COMPILED — a code the §15.1 walk must report
│   ├── nc-ids/                     Snowflake, the machine flake, Denote naming (§16)
│   ├── nc-journal/                 the hash-chained journal (§14.9)
│   │   └── postgres.rs            in src/ — the store that runs
│   ├── nc-log/                     the central log's type wall (§15.4)
│   │   ├── events.rs              in src/ — GENERATED, one struct per event
│   │   └── fixtures/               the files that MUST NOT COMPILE
│   ├── nc-platform/                a path one user's alone, per system (§2.7) — THE ONE `unsafe`
│   ├── nc-channel/                 every name an install gives the system, from one function (§17.1)
│   ├── nc-ipc/                     the protocol a daemon is reached through: frames, the handshake, the heartbeat (§11.1)
│   ├── nc-policy/                  the pack's compiler, validator and case runner
│   ├── nc-authz/                   the decision point: the embedded pack, asked (§14.2)
│   ├── nc-kernel/                  the local kernel: the host of the plugin world (§11)
│   │   └── wit/                    the plugin world: a byte copy of the workspace's reference
│   ├── nc-local/                   the local app database: one SQLite file, private (§4), the journal's store on the person's machine in it
│   ├── nc-guest/                   the world's guest half, generated once for every plugin
│   ├── nc-prune/                   one pruner, for every table (§15.8)
│   └── nc-schema/                  a command's schema subset, selector and judge (§11.4)
├── plugins/                        WASM components, one directory each (§11.3)
│   └── reference/                  the plugin the kernel is proven with
├── apps/                           the binaries a person's machine runs, each a library and a thin binary (§10.2)
│   └── daemon/                     `noctua-daemon`: the install, the profile, the device and the identifiers' mark (§11.1, §16.2)
└── xtask/                          HAND-WRITTEN — the five verbs
    └── canaries/                   NOT COMPILED — what the gates must find, and the review loop
```

The map draws two files relative to their crate — `crates/nc-journal/src/postgres.rs`, the
store that runs, and `crates/nc-log/src/events.rs` — and they are written out here so the
reference gate resolves them. The kernel's `wit/` holds one file,
`crates/nc-kernel/wit/noctua-kernel.wit`: the world every plugin is built against, a byte
copy of the workspace's normative `reference/noctua-kernel.wit` (invariant 21).

**Every directory `NOCTUA.md §10.2` names and this map does not is absent on purpose** —
the other apps, the services and the Lisp systems — and each arrives with the stage that first
puts something in it: an empty directory is not a deliverable, a cargo workspace cannot
hold an empty member, and git cannot carry an empty directory.

**`rustfmt.toml` is not a style preference, and deleting it breaks a guarantee silently.**
`cargo xtask build` pipes every generated file through `rustfmt` from `PATH`, and rustfmt
finds its configuration by walking *up* from where it runs, so without this file the walk
leaves the repository and a configuration above it rewrites generated files while every
gate here stays blind: the in-process determinism check runs both generations in one
process, where anything constant is invisible. `edition` is its only setting,
deliberately.

**Determinism here means the same toolchain twice.** `rust-toolchain.toml` names a
moving channel, so the compiler changes about every six weeks without anybody editing this
tree, and both the in-process comparison and CI's double build compare runs of one
compiler: a new `rustfmt` or a changed derive expansion can move generated output on its
own. After a toolchain change, regenerate and diff — no gate can do that for you.

**Generated, and never edited by hand**: the directories `crates/nc-catalog/src/` and
`docs/`, and one file inside a hand-written crate, `crates/nc-log/src/events.rs`.
`docs/README.md` is the one hand-written file in them, named in `HAND_WRITTEN_FILES` in
`crates/nc-catalog-compiler/src/lib.rs`.
`cargo xtask check` refuses a hand edit to any of them, and a leftover in the first two
that nothing generates any more.

## Commands

```bash
cargo xtask build     # compile catalog/ into the crate that carries it
cargo xtask build --plugins   # every plugin, built for wasm32 as a component
cargo xtask check     # THE gate. CI runs this, `cargo xtask dev` for the stack
                      #   it needs, the `cargo install` for cargo-deny, and
                      #   `cargo xtask build` twice in the determinism job
cargo xtask dev       # bring up PostgreSQL and the central log store
cargo xtask docs      # regenerate the developer surfaces under docs/
cargo xtask release --channel staging --dry-run   # what a channel would release
cargo xtask release --channel staging   # gate, then write its version and changelog
```

`cargo xtask dev` brings the stack up on this checkout's two ports and prints them, with
the `DATABASE_URL` they make: a tool outside `cargo xtask` — a hand-run `docker compose`,
the PostgreSQL suites run alone, `xtask/mutants.py` — is told them by exporting what it
prints. `POSTGRES_PORT` and `LOGS_PORT` move a port.

`cargo xtask check` judges the history too — every commit reachable from `HEAD` and every
blob ever reachable ("The history says what each change is, credits no agent, and stays
small") — so it needs the whole of it: a shallow clone is refused, CI checks out with
`fetch-depth: 0`, and a tree with no `.git` says it was not judged.

**Five verbs, and the surface does not grow** (`NOCTUA.md §10.2`). A new capability goes
behind one of them — a flag, a step inside `check` — and never becomes a sixth.

`cargo xtask check` first runs `cargo fetch --locked` and says so in one `ok` line: a fresh
clone holds only what its first build fetched, and every step that asks cargo for the graph
asks it `--offline`. With `--offline` the gate fetches nothing: the fetch does not run, and
every other cargo the gate starts is handed `CARGO_NET_OFFLINE=true`, so a package missing
from the machine fails the steps that build it instead of being downloaded, and the graph,
which fails first, names that command. The build of this tool, which `cargo xtask` runs
before the gate starts, is cargo's own and not the gate's:
`cargo --offline xtask check --offline` keeps that one off the network too. The fetch
records that it ran and the verdict holds the record to the flag: on a machine that already
holds every package, a run that fetched under `--offline` and one that never fetched
without it differ from one that got it right only by the fetch's own `ok` line, which no
step reads, so the verdict refuses both.

`cargo xtask check --offline` skips **nine** things: the advisory database, which is
fetched — the audit's licence, ban and source checks read only the lockfile and still
run; the PostgreSQL tests; the corroboration of `migrations/`
against what a database has actually applied, the only thing in this tree that remembers a
migration applied and then deleted; where `pg_temp` sits in the migrated functions' search
paths; the journal's own protections, which are triggers and constraints held by the
database and not by any file here; the bounds the running log store reports, which only
the store itself can state; how the database sorts and encodes, which
`POSTGRES_INITDB_ARGS` decides once and only the database remembers; the comparison of
each pinned digest against what its tag resolves to, which needs the image pulled; and the
stack's own tests, which ask docker what it resolves rather than reading one compose file,
and panic where docker is absent. Each prints a `warn` line, so every skip is announced
rather than silent. It exists for a fast loop while editing, and **CI never passes it**
(`NOCTUA.md §19.4`). `OFFLINE_SKIPS` in `xtask/src/check.rs` holds the number, and a test
holds this file, `docs/README.md`, the `justfile` and `--help` to it.

**On macOS and Windows the steps that need the container stack are not applicable**
(`NOCTUA.md §22.2`): every one of those skips but the advisory database prints a line of
its own kind — not applicable on the system, what it is, and that the Linux job runs it —
instead of running, the PostgreSQL suites and the stack's own tests stand aside saying the
same, and `cargo xtask dev` says so and changes nothing. Which parts those are is
`THE_STACKS_PARTS` in `xtask/src/check.rs`, and the verdict holds a run to that list
exactly, which on Linux is empty.

**`build` is not a subset of `check`.** `build` compiles the catalog and writes what
changed in under a second, with first-rate diagnostics, and does not cross-reference the
catalog against anything outside `catalog/`; only `check` reads `migrations/`, `policy/`
and the generated crate against it. So edit, `build`, move on is the wrong loop for
anything the catalog says *about* the rest of the tree — a nullable `order_by`, say, which
`build` accepts and `check` refuses.

## Making a change

Each change below has one step that is not where you would look for it, and missing it
fails a gate a minute or two later.

| To add | Edit | The step that is not obvious |
|---|---|---|
| an error code | `catalog/errors.toml`, `catalog/i18n/en.toml`, `catalog/i18n/pt-BR.toml` | both locales, always; and `cargo xtask build` writes the code into the append-only ledger, permanently, saying so on the line it does it |
| a log event | `catalog/log-events.toml` | `level`, in `tracing`'s spellings (`warn`, not `warning` — that is `errors.toml`'s `severity`); and `id` is a kind the compiler refuses until `§5.1` closes |
| a policy | `policy/core/*.cedar` + `policy/core/tests/*.toml`, or `policy/packs/<profile>/*.cedar` + its `tests/*.toml` | a core file's name goes in `CORE_POLICIES` **and** each core `@id` in `CORE_RULES`, and each pack `@id` in `PACK_RULES` — all in `xtask/src/policy.rs` and all compared against the pack in both directions; every policy needs a **denial** case; every case is a row of `THE_CASES`, every clause a case pins is counted per policy in `PINNED_BY_POLICY`, and every entity added to `policy/world.json` is a row of `THE_WORLD`, in the same file; every case name is also a row of `WHAT_ELSE_THIS_TOOL_SAYS` in `xtask/src/check.rs` (its length and `THE_REST_IT_SAYS` move with it), because `THE_CASES` ships the name as a sentence; every constraint of a permit needs a case whose answer flips when it is removed — a case refused for its verb, or for its kind of principal, says `the_action_is_the_boundary` or `the_principal_is_the_boundary` — and one no request the schema can build could show working is taken out of the permit rather than given a case that cannot exist; a narrowed scope needs `// nc:narrows: <reason>`; and every `@id` is lower-case words joined by dots, `<pack>.<rule>` — its first word `core` under `policy/core/` and the profile under `policy/packs/<profile>/`, which `nc_policy::compile` refuses otherwise (`policy-id-outside-its-pack`) — none of them beginning with `kernel.` (`nc_policy::RESERVED_PREFIX`), which is the decision point's own; and each file is also an `embed!` line in `crates/nc-authz/src/embedded.rs` — in `CORE`, or in its pack's list — because a kernel decides with what it was built with, and `the_embedded_files_are_the_files_on_disk_in_both_directions` refuses a policy on disk that no line embeds, or that a line embeds in another directory's list — `CORE` is compiled into every pack carried |
| a pack's first policy, or its next version | `policy/packs/<profile>/pack.toml` + `policy/world.json` | the manifest holds `version = <n>` alone, and it arrives with the pack's first policy — a manifest beside no policy is refused as well; every realm whose profile is that pack pins `<profile>@<n>`, so raising the version is the same edit as migrating every such realm, and a realm pinning another profile's pack is refused whether or not this tree carries its own. Each `.toml` under `policy/` is a case file inside a `tests/` directory or a manifest directly in its pack's directory, and each `.cedar` sits in `policy/core/` or in `policy/packs/<profile>/` — a decision point compiles the core and its realm's pack, so a policy beside them would pass every case and be enforced nowhere; one anywhere else is refused. A pack's first policy also gives it a list of its own in `crates/nc-authz/src/embedded.rs` and a `CARRIED` row naming its profile and its `pack.toml`: until then no kernel carries it and its realms are denied as not carried, and the same embedded-list test refuses the tree |
| a migration with a table | `migrations/NNNN_*.sql` + `catalog/limits.toml` | `[tables.<name>]` — `strategy` and `order_by` always, plus `max_rows` and `prune_batch` when the strategy is `oldest-first`; a `policy-pack` table carries `pack_reason` instead and **no numbers**, which is what `journal_entries` does; a `refuse-at-limit` table is one whose oldest rows are not the pruner's to take — user state, or a unit's images and notes — and carries `max_rows`, where its writer refuses a new row, and `per = "plugin"` or `per = "unit"` when that writer counts each plugin's or each unit's rows rather than the table's (the compiler refuses `per` on any other strategy, and `docs/limits.md` publishes it as `Counted over`) — an ordering column that is `NOT NULL` and sorts in creation order — or `-- nc:bounded: <reason>` in the comment block above the table. The file's name is also a row of `MIGRATIONS_ON_DISK` in `xtask/src/check.rs` |
| a local migration | `local-migrations/NNNN_*.sql` + `catalog/limits.toml` + `crates/nc-local/src/schema.rs` | SQLite, never in `migrations/`; its tables are bounded like the server's, with `database = "local"` in the entry. The file is a row of `LOCAL_MIGRATIONS_ON_DISK` with the SHA-384 `sqlx` keeps for it, and **a migration a build has carried is never edited**, a comment included — every file that build made records the checksum and is refused at every open once it differs (`NC-DB-0007`) — so a change is a new migration, and a rewrite before any release is an edit of the pin, which `check` holds each file to. Every object it builds is a row of `nc_local::schema::EXPECTED` with the BLAKE3 of the SQL SQLite stores for it and the migration that builds it (`since`): `cargo test -p nc-local` fails printing each digest until the list is updated, and `check` builds a scratch file one migration at a time and holds each step to the list, because the crate judges a file one migration behind by what its own history built. To the gate a trigger's `BEGIN … END` body is one statement, and SQLite's `[name]` and `` `name` `` are names as `"name"` is. `CREATE VIRTUAL TABLE` is refused — its rows live in shadow tables no entry can bound — and so is every statement invariant 18 lists, anywhere in the sources `WRITERS` in `xtask/src/local.rs` names: every member whose manifest takes SQLite (`sqlx` with a `sqlite` or `any` feature, or a crate that hands the file over), read whole or named in `NOT_READ` with its reason (`every_member_that_opens_the_local_database_is_read_or_named`), a SQL comment inside a string read as the gap SQLite reads it as. So is a migrator built when the store runs, which would migrate the user's file with whatever SQL sits in the daemon's working directory: `sqlx::migrate!` of this directory is the one migrator. Then **touch `crates/nc-local/src/store.rs`**: `sqlx::migrate!` embeds the directory when the crate compiles and, on stable Rust, does not tell cargo it read it, so a new file is not in the build until a source changes — `the_embedded_migrations_are_the_ones_on_disk` fails naming both lists until then. And the tests that put a file one migration behind name the newest migration's table and number — `one_migration_behind` and `what_the_file_holds` in `crates/nc-local/tests/opening_refuses_what_it_did_not_make.rs` — while the gate's own tests in `xtask/src/local.rs` count the shipped migrations and plant one numbered past the newest |
| a crate | `crates/<name>/` | more than the `members` glob suggests. The manifest takes `[lints]` with `workspace = true` and `version.workspace = true`, a crate with a `tests/` suite depends on itself under `[dev-dependencies]` or rustdoc cannot resolve the suite's `use`, and the crate root denies printing. Then `cargo test -p xtask --bin xtask` fails naming each list the crate has to join — the error-code scope (`JUDGED_BY_THE_ERROR_CODE_RULE` or `OUTSIDE_THE_ERROR_CODE_SCOPE`), the mutation census (`SOURCES` in `xtask/mutants.py`, or `NOT_MUTATED`), `SUITES`, `DOCUMENTED` and the source-count pins. Three lists in `xtask/src/gates.rs` are joined by hand, because nothing holds them to the crates: `JUDGED_BY_THE_RAISING_RULE`, `READ_BY_THE_HATCH_WALK` and `COVERED` — a crate taken out of all three leaves that command green |
| a member under `apps/` | `apps/<dir>/`: `Cargo.toml` and `src/`, a library and its binary | every list a crate joins (the row above), by its path under `apps/` — the trees are `MEMBER_TREES` in `xtask/src/members.rs`, so the walks read it already, and the hand-kept lists in `xtask/src/gates.rs` are the ones to join. The package is `nc-<dir>` — the mutation runner reads it from the path — and the binary is named in a `[[bin]]` table: what the app does is the library's, and the binary's `main.rs` reads its arguments and prints through the one printing door, the invocation it answers and its exit codes decided by functions its own unit tests call. **A source's mutation command runs its own module's unit tests, and never a suite that spawns a process**: `cargo test -p nc-<dir> -- <module>::` for a module and `--bin <binary>` for `main.rs` (`--lib` is refused outright), so a suite in `tests/` that starts the binary, a daemon or a child, or registers a unit, runs under no operator. `a_suite_that_spawns_a_process_is_run_by_no_mutation_command` in `xtask/src/check.rs` holds every suite of every member to that, but the ones `SPAWNED_BY_THE_RUNNER` writes down, and such a suite's `NOT_MUTATED` row says `spawns a process, so no mutation command runs it`. It makes its scratch, its children and its files under a scratch root, each with a guard that removes it |
| a plugin | `plugins/<dir>/`: `Cargo.toml`, `plugin.toml` and `src/` | the lists a crate joins in `xtask/src/gates.rs` — `JUDGED_BY_THE_ERROR_CODE_RULE`, which the census of every member cargo resolves holds it to, and `JUDGED_BY_THE_RAISING_RULE` and `READ_BY_THE_HATCH_WALK` by hand — because the §15.1, raising and hatch walks read `plugins/` as they read `crates/`. The package is `nc-plugin-<dir>` — the plugin build refuses any other name, and the mutation runner reads the package from the path — with `crate-type = ["cdylib", "rlib"]`: what the plugin does goes in a module the host target tests, and `impl Guest` with `nc_guest::generated::export!` in an `adapter` module whose file opens with `#![cfg(target_arch = "wasm32")]`, because a host `cdylib` holding the exports does not link — the condition inside the file, since one on the `mod` line reads to the ignored-test gate as hiding every test in it. `cargo xtask build --plugins`, and `check` before the tests, builds it for `wasm32` and encodes it, refusing a component that imports anything the world `plugin` does not declare (which is what a dependency with bindings of its own brings in), then lints and documents it for that target — the only run that reads the adapter; a kernel suite loads it through `nc_kernel::component_for_tests`, which refuses a component its sources no longer build. Every `.rs` joins the mutation census, and the adapter's command is the plugin build |
| the plugin world | `reference/noctua-kernel.wit` in the workspace, then `crates/nc-kernel/wit/noctua-kernel.wit` | the workspace's file first — it is normative — then the copy, byte for byte, then `WIT_BLAKE3` in `crates/nc-kernel/src/world.rs`. `the_kernel_world_is_the_reference_world` holds the copy to the reference wherever the workspace is there, and the digest holds it in a checkout; a new `package` version is `WORLD_VERSION` as well |
| a command | `catalog/commands.toml`, both locales | its `args_schema` is held to the closed subset `crates/nc-schema` documents — every object `"additionalProperties": false`, no keyword outside `nc_schema::KEYWORDS`, and no character `nc_schema::unseen` answers for — `INVISIBLE_RANGES` and the default-ignorable `BLANK_RANGES` — but JSON's own whitespace, which the title is held to as well — and an `argument:<name>` selector needs that argument required, a string with `x-entity` naming its Cedar type, and `maxLength` at most 128, and never selects `Realm`, which `realm` alone does; the kernel judges every command a plugin declares at `activate` by the same function (`NOCTUA.md §11.7`), so a rule written there reaches both |
| a keyword the argument schema accepts | `OWNED_BY` in `crates/nc-schema/src/lib.rs`, and the subset table in that file's header | the one type it belongs to — a keyword written on another type is refused rather than read as a no-op — its reading in that type's compile and its refusal in the validator, and a row of `WHAT_EACH_KEYWORD_DOES` beside the tests: a schema using it, a value it refuses, and that refusal gone once the keyword is taken out. `every_keyword_in_the_subset_decides_something_or_is_an_annotation` refuses a keyword with no row, because a keyword the subset admits and nothing reads is one an author believes refuses what it does not. The catalog compiler and the kernel's `activate` both judge through this crate, so the keyword reaches every built-in and every plugin in the same build — and a plugin that uses it is refused by a kernel built before it, which is the closed subset working |
| a command of the kernel's own | `catalog/commands.toml`, in a namespace the kernel keeps, and `crates/nc-kernel/src/builtin.rs` | a `Builtin` variant, in its `ALL`, named as the catalog names the command, with its arm in `builtin` in `crates/nc-kernel/src/dispatch.rs` — `every_command_the_catalog_gives_the_kernel_is_one_it_runs_and_it_runs_no_other` holds the catalog, the registry and the handlers to each other both ways — and a test holding the arguments the handler reads to `Command::arguments`, both ways; one that changes state outside every unit is noted as having left the machine before it runs, so an undo stops at the step that ran it, and changes nothing until its dispatch's entry has landed — `limits.prune` is rehearsed as a dry run, then journaled as allowed by `pruned_once_journaled` in `crates/nc-kernel/src/dispatch.rs`, and only then deletes — since nothing a journal refuses afterwards could put it back |
| a capability | `catalog/capabilities.toml`, both locales | its functions are the world's, written as the world's parser names them — `[method]<resource>.<name>` for a resource's method — and every function of the world sits in exactly one capability or in the `functions` of an `[[ungated]]` interface, beside the reason that covers it: an interface given to every plugin gives only what it lists, so a function the world gains there is refused by the coverage test until someone writes it in, rather than reaching every plugin under a reason written before it. `crates/nc-kernel/tests/capabilities.rs` holds the registry to the world, to the reference plugin's probe (`PROBED` in `plugins/reference/src/behaviour.rs`) and to the capability each host function asks for, so a function the world gains is also a host function in `crates/nc-kernel/src/host/` whose first statement asks for it — or, in an `[[ungated]]` interface, records it with `HostState::ungated` — a row of `PROBED` and an arm of the probe in `plugins/reference/src/adapter.rs`, and, under a capability, a call in `every_gated_function` in `crates/nc-kernel/src/host.rs`, whose test grants one capability at a time and holds what each function asked for, and under which name, to the catalog — the probe cannot, because a method is out of its reach until its constructor is built |
| a rule a plugin's command is held to at `activate` | `crates/nc-kernel/src/registry.rs` | a refusal with the code of its kind — `[NC-PLUG-0008]` for a field left empty, `[NC-PLUG-0009]` for anything else wrong with the declaration, `[NC-KERN-0015]` for a name the plugin may not have — and a unit test in that file; and, where the reference plugin can declare it, a `Variant` in `plugins/reference/src/behaviour.rs`, which the plugin reads from its own store's `activation` key, and a test in `crates/nc-kernel/tests/activation.rs` loading the real component that way. A rule the judge in `crates/nc-schema` can state belongs there instead, where the catalog compiler holds every built-in to it too |
| a refusal the decision point makes | `crates/nc-kernel/src/dispatch.rs` | a code of its own in `catalog/errors.toml` with both locales, a `Reason` — whose sentinel begins `kernel.`, which no policy's `@id` may, and is what the journal records as having decided — with its variant for `kernel.dispatch_refused`, and a case in `crates/nc-kernel/tests/the_decision_point.rs` that dispatches into it through the real component and reads the refusal back from the journal and the log. A denial tells the caller its code and never the policy that decided: that is the journal's. Anything run from inside a host call that measures its own stack — Cedar does — runs on a task of its own (`Frame::dispatch` says why) |
| a rule a plugin's request is held to | `judged` in `crates/nc-kernel/src/net.rs` | a refusal in the order the module header lists the rules, with its `Reason` — logged by its own name in snake case, which the `net.refused` comment in `catalog/log-events.toml` must name and which `Reason::ALL` must hold, as `each_reason_is_logged_by_its_own_name_in_snake_case`, `every_reason_is_logged_by_a_word_the_event_names_and_no_other` and `every_reason_declared_is_one_all_holds` hold — and its code: `[NC-PLUG-0012]` for a domain, `[NC-PLUG-0017]` for a bound, `[NC-PLUG-0013]` for the rest; a unit test in that file, and a case in `crates/nc-kernel/tests/net.rs` sending the reference plugin's request through the decision point, which reads the transport's own record of what reached it. A rule about what is *dialled* belongs to the transport's resolver in `crates/nc-kernel/src/net/ureq.rs` instead, whose tests reach only sockets on this machine |
| a host function that waits | its interface's file under `crates/nc-kernel/src/host/` | its body, after the capability is asked, goes through `supervisor::in_time` under its `interface#function` name with `self.left()`, which holds it to `host_call_deadline_ms` or to what its call tree has left, whichever is nearer, and kills the plugin past it — every host function but `commands.dispatch`, whose command runs under its call tree's deadline and whose waits on the journal the decision point holds to the same bound (`Waits` in `crates/nc-kernel/src/dispatch.rs`) — and a case in `crates/nc-kernel/tests/isolation.rs` makes it never answer, on a paused clock, and reads the kill back as `[NC-PLUG-0006]` naming that function |
| a note a unit keeps | `Note` in `crates/nc-kernel/src/history.rs` | its kind is its `note` field, written as canonical JSON and read back by `Note::read`, the one reader: a note of a kind it does not know, or of a known kind in another shape, is refused as a row the kernel did not write (`NC-DB-0007`), so an undo never acts on a note it misread. A note of a blob made or a request sent is written where it happens (`crate::blobs`, `crate::net`) and read here too. A unit's notes of every kind share `store_notes`' bound, and `every_note_the_history_writes_is_read_back_as_itself` holds each kind's text both ways |
| a call the kernel asks of the store | `StoreBackend` in `crates/nc-kernel/src/backend.rs` | the same method on `nc_local::LocalStore` first — the trait answers as that store does, and is its specification — then on `MemoryBackend` beside the trait, with the same refusals in the same sentences; `crates/nc-kernel/tests/both_stores_answer_alike.rs` runs each script through both and holds the two transcripts equal, and its property test drives both through every operation, in two realms and every kind of unit, so a call the second store answers otherwise fails there — a script or a sequence that never varies what a filter selects on cannot see that filter go |
| a call the kernel asks of the journal store | `nc_journal::Store` in `crates/nc-journal/src/lib.rs` | a **required** method — one provided over `from(0)` would be the slow path, and the blind spot of a row that does not decode, inherited in silence — answered in the same change by all three stores: `InMemory` (`memory.rs`, the specification), `Postgres` (`postgres.rs`, one statement over columns that cannot fail to decode) and `nc_local::LocalJournal` (`crates/nc-local/src/journal.rs`, every statement in that crate), and by every test store that implements the trait — `unit.rs`, `tampering.rs` and the kernel suites' wrappers. Its `# Errors` section names what each implementation builds (`every_errors_section_names_what_its_body_builds`, whose count of sections is written down), the shared scripts of `crates/nc-journal/tests/in_postgres.rs` ask it of the two stores in that crate, and `crates/nc-local/tests/the_local_journal.rs` holds the local journal's transcript to both, over written scripts and generated ones |
| a call the kernel asks of the blobs | `BlobStore` in `crates/nc-kernel/src/blobs.rs` | the same method on `DirectoryBlobs` first — the one a daemon keeps a person's files in — then on `MemoryBlobs` beside it, which `the_two_stores_answer_one_script_alike` holds to one script; the trait is asynchronous because the disk may not answer, so a host function that calls it goes through `supervisor::in_time` like any other that waits, and a writer is moved into a write and handed back, so one a deadline abandons takes its writing with it. What a blob is to the decision point — which unit made it, whether one still holds it — is the store's, in `nc_local`'s `blobs` table, never the blob store's |
| a kind of node a surface shows | `KINDS` in `crates/nc-kernel/src/ui.rs` | the table in that module's header is what a plugin author reads, and changes in the same edit; the kind's keys are a row of `KINDS`, a key holding a person's text is in `TEXTS` and a closed choice in `CHOICES`, so every text of every kind is held to the same bound and characters; and a test in that file keeps a tree of the new kind and refuses each rule it is held to — the interface's renderer (Stage 16) draws only the kinds this set names |
| a name the system sees | a field of `Layout` in `crates/nc-channel/src/lib.rs`, derived in `layout` and in the table of that crate's header | a line of `parts` in `crates/nc-channel/tests/the_names_are_disjoint.rs`, which destructures `Layout` without `..` and does not compile until the field is read — what it reads is held apart from every other install's name, case-folded; a name the whole machine shares carries the user and is a row of `MACHINE_WIDE` there too, unless it is a checkout's stack's, which is the checkout's; its spelling is written out in `each_name_is_spelled_as_the_crate_header_writes_it`; a relocated install's must carry its namespace and lie under its root, or be `None` as the update feed and the stack are; and nothing outside the crate spells one — `cargo xtask dev` names its stack's project and binds its ports through `Checkout::stack`, and `POSTGRES_PORT` or `LOGS_PORT` only moves a port |
| a kernel limit | `[kernel]` in `catalog/limits.toml`, and a field of `KernelLimits` in `crates/nc-catalog-compiler/src/model.rs` with its row in `KernelLimits::each` | every limit is required and above zero, and one that bounds another is held to it where both are written (`check_kernel` in the compiler's `load.rs`); `xtask/tests/the_registry_reaches_the_code.rs` compares every field by name, so a new one is a line there as well, and `docs/limits.md` publishes it with its unit. A plugin's manifest may lower only what `Lowering` in `crates/nc-kernel/src/limits.rs` names, and never raise it |
| an IPC message | `crates/nc-ipc/src/message.rs` — a variant of `Request` or `Answer`, or a message of the handshake — and the module header of `crates/nc-ipc/src/lib.rs` | its encoding and its reading in the same file, the reading **strict**: every field it holds taken by `Fields` and `done()` refusing the rest, so a field an older build does not read is a refusal rather than a value dropped in silence; a message the handshake reads names the protocol first, and a change of what an existing message means is a new `PROTOCOL`, never the same number; a case in `crates/nc-ipc/tests/` that sends it through a real `serve` and `connect`, and one that sends it malformed by hand through `nc_ipc::frame`. Whatever the daemon answers is held to `ipc_frame_max_bytes`, which the compiler holds above the kernel's `payload_bytes` and the envelope of 64 KiB `crates/nc-catalog-compiler/src/load.rs` names, so an answer that may grow without bound — the registry, a status — is refused to the daemon before anything of it is sent (`NC-IPC-0016`), its request still waiting for an answer the daemon can send instead, and never a frame cut short |
| a daemon limit | `[daemon]` in `catalog/limits.toml`, and a field of `DaemonLimits` in `crates/nc-catalog-compiler/src/model.rs` with its row in `DaemonLimits::each` | every limit is required and above zero, and one that must keep a bound outside its table is held to it where it is written (`check_daemon` and `check_daemon_ipc` in the compiler's `load.rs`, with a failing case in `crates/nc-catalog-compiler/tests/the_gates_fire.rs` at the boundary): `mark_lease_ms` below the day past which a minter reads a mark as corrupt, `ipc_frame_max_bytes` at least the kernel's `payload_bytes` and the frame's envelope, `ipc_silent_after_ms` above two `ipc_heartbeat_ms`, and `ipc_connections_max` at most 252, what a Windows pipe's instances hold with one waiting and one past the bound. `xtask/tests/the_registry_reaches_the_code.rs` compares every field by name and counts them, `xtask/tests/every_generated_value_is_compared.rs` counts the generated surface, and `docs/limits.md` publishes it under **Daemon** with its unit, whose prose `xtask/tests/the_pages_reach_the_catalog.rs` pins. The daemon and `crates/nc-ipc` read `nc_catalog::DAEMON` and take no value of the type from a caller |

Two things are true of all of them:

- **The catalog loop is fast and the `check` loop is not.** A registry mistake is a
  sub-second `cargo xtask build` away; anything that relates the catalog to `migrations/`,
  to `policy/` or to the generated crate is `cargo xtask check`, and
  `time cargo xtask check` says how long that is on your machine — a written duration rots
  the way a written count does.
- **Every refusal here ends with the fix** (`NOCTUA.md §15.1`). One that does not is a
  defect in the diagnostic; report it as one.


## Invariants

Breaking any of these is a defect, not a tradeoff. Each says what enforces it, because a
convention nothing enforces decays: 13 and 15 are judgements whose form a gate holds and
whose substance no gate can; 16 is gated in its count and measured by a recipe in its
effect; 22 is gated over the statements written today and by nothing over the next.

1. **The colour system is never named directly.** It reaches this repository through
   `noctua-design`, which vendors `noctua-colors`' generated output (`NOCTUA.md §8.6`); a
   direct dependency here would be a second copy of the colour system at a version nothing
   keeps in step. A manifest naming `noctua-colors` fails `check`.

2. **No environment file is ever committed**, at any depth, whatever it is called: a real
   one holds the database password, and secrets are ephemeral environment variables
   (`NOCTUA.md §15.5`). A name is read as dot-separated parts — an environment file when
   one part is `env` (`.env.local`, `env.production`, `secrets.env.local`), a template,
   which *is* committed, when one is `example`, `sample`, `template` or `dist`, as in
   `.env.local.example`. <!-- allow-reference: names `.env.local.example` — names a shape the rule judges, not a file here -->
   A trailing file type decides the kind unless it is one an environment is written in:
   toml, json, sh or yaml leave it an environment, while
   `env.rs` <!-- allow-reference: names `env.rs` — names a shape the rule judges, not a file here -->
   is a Rust module. `check` walks the whole tree and reports how many paths it read,
   `.gitignore` applies the same rule, and a test drives both over the same names beside
   `git check-ignore`'s verdict. Both are **case-sensitive, because git is**: a `.ENV` is
   refused by the rule that nothing unnamed sits at the root.

3. **Generated output is never hand-edited, and `catalog/errors.reserved.toml` is not
   generated output.** It is the record of which error codes have been used, a removed
   one included, so `build` appends to it, `check` reconciles it with
   `catalog/errors.toml`, and a checksum over it catches an edit made outside either.

4. **A written list is compared with the tree in both directions.** `SUITES`, `SHAPES`,
   `CORE_POLICIES` and the determinism needles are each checked against what exists,
   because a list the gate only reads *from* shrinks with the tree and reports the smaller
   number as `ok`, and one direction misses what the list forgot. **`COUNTABLE` is the
   exception**: each noun must match something real, while the other direction — what a
   document might count that the list does not name — cannot be enumerated, so it is a
   judgement about what documents claim.

5. **A count of zero is not a pass, and neither is a count of one.** A gate that walks
   reports what it inspected and fails at zero (`NOCTUA.md §19.3`), and where the tree can
   say how many there *should* be — the files in a directory, the members cargo builds —
   it compares with that. **The walk's one real number** is every source root cargo
   compiles plus the files this tool resolves itself (the alias, the workflow, the
   justfile, `.env.example`), printed in the type wall's line. **Every step that reads a
   set says how large it was**: `the_formatting_step_read_the_whole_tree` and
   `the_lints_step_read_the_whole_tree` hold formatting (run `--verbose` to be counted)
   and clippy to every source root cargo declares, the audit reports the crates it read
   and fails without licence statistics, and a step that checks one fact has nothing to
   count. **The migration count leans on the database**: `migrations/` is compared with
   `_sqlx_migrations`, the only thing outside this tree that remembers what was applied
   — no step compares the tree with what it held before — since deleting the
   highest-numbered migration leaves no gap in the numbering.

6. **No query is checked against a live database while compiling.** `sqlx`'s `query!`
   family needs a database at build time, so `cargo xtask check --offline` could not
   build. `macros` stays **on** for `sqlx::migrate!`, which needs none, and a feature list
   cannot split the two, so the rule is a source gate over every Rust file
   (`gates::no_compile_time_query`).

7. **Every setting `.env.example` declares is read by something**, or a developer changes
   a line that does nothing. `check` reads the tree with comments stripped and fails on an
   unread name, and a test holds each credential to the default `docker-compose.yml` falls
   back to, and each port to an empty value the compose file requires, since both ports
   are the checkout's — so no `DATABASE_URL` is written there either, carrying one.

8. **A trigger names its own table, and its path searches `pg_temp` last.** PostgreSQL
   resolves a relation in the temporary schema before `search_path`, and every role can
   `CREATE TEMP TABLE`: a planted `journal_entries` stops every append for good, and a
   planted sequence hands an `access_records` row a number the pruner never reaches
   (`migrations/0006_access_records_orders_by_insertion.sql`). **`search_path = ''` does
   not close it**; `pg_temp` listed **last** does, and pinning a path at all stops a
   planted function (`NOCTUA.md §14.9`). `check` fails a `CREATE FUNCTION` in
   `migrations/` that pins no path, then asks the database where `pg_temp` sits in each
   function's `proconfig` — the migrations write `SET search_path FROM CURRENT`, so a test
   migrating into its own schema never aims a trigger at the developer's journal.

9. **A pin is compared against what its tag resolves to, not just its shape.** Docker
   resolves `tag@sha256:…` by the digest alone, so a digest swapped under an untouched tag
   runs another image with every printed line unchanged. `check` asks docker what each
   pinned tag resolves to and counts the comparisons apart from the images it read; an
   image nobody pulled is **announced as a skip**, since a gate failing on a cold cache
   gets turned off, and `every_pin_was_answered_for` requires every pin compared, found
   wrong or announced. **Not covered**: it reads this machine's image store, never a
   registry; compose never stores the tag, so only a `docker pull` of each makes it
   answerable; and a tagless reference passes the digest rule and is inspected as `latest`
   — a finding or a skip, depending on the store.

10. **`.cargo/config.toml` carries an alias and nothing else.** One
    `[build] rustflags = ["--cap-lints=allow"]` there turns off every lint and demotes
    `unsafe_code = "forbid"` with every count unchanged — Stage 10's criterion 1, "no
    failures and no warnings", met by making both unreachable. The rule is a whitelist,
    because the ways cargo hands rustc a flag grow with cargo.

    `check` also refuses to run with `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS` or
    `RUSTDOCFLAGS` set, since cargo joins them with the file and such a run is not the one
    CI makes; and it judges `$CARGO_HOME/config.toml` — merged wherever it sits, and
    reached by no walk up from here — by **what it holds**, a mirror or a linker there
    being ordinary. `CARGO_HOME` carries no flag, so it is not in that family. The rule
    underneath: **ask what the tool reads, not what the gate walks** (`NOCTUA.md §19.5`).

11. **A gate's fixture is planted in the directories the repository really has, and its
    floor counts the same ground its walk does.** A filter on a real subtree —
    `{ judged += 1; reached += 1; continue; }` — pays every counter on its way past, so
    `in_every_real_directory` plants in every directory the tree has and each caller
    asserts on all of them. A floor of "half" is slack: both counts cover identical ground
    and compare by equality, sharing no rule. An exclusion list is asked of the gate's own
    predicate both ways — every directory in scope reached, every one it names not judged
    — and kept once.

12. **A guard added because something broke arrives with the test that would catch it
    breaking again** (`NOCTUA.md §19.5`). Counters cover the walk, not its branches;
    branch instrumentation — a marker per branch, the markers inserted diffed against
    those fired — finds the arms nothing drives. **A predicate written twice is one copy
    driven and one not**, so a copy delegates. **A constant that is a stored value is
    pinned by writing it out**: `unit::OPENS` and `CLOSES` are literals hashed into every
    bracket entry, so their test states the literals, since a rename would let a closed
    unit accept entries with no migration possible for a hashed column.

13. **A test drives the thing it is testing, not a copy of it** — a mirror agrees with
    itself. A private function a test must reach gets a `#[doc(hidden)]` door with its
    reason written on it (`nc-journal`'s `unavailable_for_tests`, `nc-prune`'s
    `waited_for_tests`, the catalog's `fill_for_tests`), and
    `every_hatch_carries_its_reason` refuses a `_for_tests` door without `#[doc(hidden)]`
    or a reason in its prologue. Whether a door reaches the real thing is a judgement:
    `grep -rn '_for_tests' crates` lists the doors to read. The gate's printed sentence is
    `just check`'s to show, never quoted here.

14. **Warnings are errors at the gate — including rustdoc's** (`NOCTUA.md §2.2`): clippy
    runs with `-D warnings`, and `cargo doc --no-deps --document-private-items` with
    `RUSTDOCFLAGS=-D warnings` handed to that one child (the variable itself is refused in
    the environment, invariant 10), while an ordinary `cargo build` still compiles.
    Private items are documented because most of this tree's prose is on them, and the doc
    step is the only one that reads intra-doc links.

15. **A clippy allow arrives with the paragraph that earns it.** The workspace starts with
    none. `every_hatch_carries_its_reason` refuses an `allow` or `expect` with neither a
    `reason = "…"` argument nor a comment above it, over every `.rs` in a member tree but
    the canaries — **a tree the manifest gains joins that walk in the same edit**, because
    the walk reads `MEMBER_TREES` ("Where the platform's code is, is one list"), and an
    allow outside it would never be read. `#[expect]` takes the
    argument, and a crate-level `#![allow]`, which has no item to sit on, a comment above
    (how the generated crates carry theirs). What earns an allow stays a judgement;
    `-D warnings` keeps every allow in a diff, and `#[expect]` fails once its reason stops
    applying.

16. **A run leaves nothing behind but its own output.** A test's scratch directory is
    owned by a guard that removes it on `Drop`, on the failing path as on the passing one.
    `BUILDS_A_SCRATCH` (per file, the scratches built and those guarded) and
    `THE_SCRATCHES_WITH_NO_GUARD` (the rest, by name) are equalities, so a scratch without
    a guard cannot land; the effect, on a temporary directory every process shares, is
    measured by `just leaves-nothing`, which refuses anything one whole suite adds to
    `/tmp`. **Not covered**: the tests `THE_SCRATCHES_WITH_NO_GUARD` names remove their
    scratch only on the path that reaches their end, so a mutation run that kills one
    leaves its scratch behind — `just leaves-nothing` measures a suite that passes. A
    guard on each closes it.

17. **No `unsafe` is written in this repository but in one module — and that is what
    `forbid(unsafe_code)` means here, exactly.** Every member takes
    `unsafe_code = "forbid"`, the plugins included, but `crates/nc-platform`, whose
    Windows half makes the Win32 calls `windows-sys` declares `unsafe` — the security
    calls, the system's random bytes, the processes at either end of a named pipe — and
    creates the daemon's pipe through `tokio`'s `create_with_security_attributes_raw`,
    which is `unsafe` for the descriptor it is handed (decided on 2026-11-02,
    `NOCTUA.md §2.7`): a `forbid` cannot be lifted by anything
    under it, so that crate writes the workspace's lints out with `unsafe_code = "deny"`
    and allows it on `crates/nc-platform/src/windows.rs` alone, by one attribute on its
    `mod` line. Every `unsafe` there sits under the `// SAFETY:` comment — or, on an
    `unsafe fn`, the `# Safety` doc — that says why each pointer it hands the system is
    valid and what is freed where. rustc does not lint an external macro's expansion, so `unsafe` a
    dependency's macro expands into is the dependency's: wasmtime's `bindgen!`
    (`TypedFunc::new_unchecked` in `crates/nc-kernel/src/bindings.rs`), and
    `wit-bindgen`'s `generate!` and `export!` in `crates/nc-guest` and every plugin. The
    lints are never judged with `WASMTIME_DEBUG_BINDGEN` or `WIT_BINDGEN_DEBUG` set, which
    make the expansion a local file; and `export!` is invoked from a plugin while
    `generate!` lives in `nc-guest`, because `export!` in the crate that generated it is
    local code. **Gated three times**: the lint, with
    `every_member_takes_the_workspace_lints` holding `nc-platform`'s written-out table to
    the workspace's entry for entry, that one level apart;
    `no_unsafe_is_written_in_any_rust_file_of_this_tree` in `xtask/src/gates.rs`, over
    every `.rs` the tree holds — fixtures, canaries, the `wasm32`-only adapter — comments
    and strings blanked, which names `windows.rs` as its one exception, fails once that
    file writes none, and holds each `unsafe` in it to its comment; and
    `#![doc(test(attr(forbid(unsafe_code))))]` at every library root, the generated
    catalog's included, which `every_library_root_forbids_unsafe_in_its_doctests` asks
    of cargo's member list, a doctest being a crate the other two never read.

18. **The local database's schema is `local-migrations/` and nothing else.** Every object
    those files build is listed in `crates/nc-local/src/schema.rs` with the BLAKE3 of its
    SQL and the migration that builds it, and the crate holds every database it opens to
    that list — before migrating, to what the file's own history built and to that history
    being this build's; after, to the whole list. **A migration a build has carried is
    never edited**: every file it made records the checksums, so an edit refuses every
    such file for good, and the change is a new migration; `LOCAL_MIGRATIONS_ON_DISK` in
    `xtask/src/check.rs` pins each file's SHA-384.

    **Gated twice, in `xtask/src/local.rs`**, whose header lists every refusal. `check`
    applies the directory to a scratch file one migration at a time and holds each step to
    the list, with `integrity_check`, `foreign_key_check`, the header's `application_id`
    and the SQLite floor. A source gate, over every member whose manifest takes SQLite —
    `sqlx`'s `any` feature included, a test holding the gate's list to them — refuses any
    statement that builds or changes the schema, attaches a database or touches
    `writable_schema`; undoes the connection's hardening, through SQL, the pragma door or
    `sqlx`'s own option, every value judged as SQLite reads it and the list held both ways
    to the pragmas `crates/nc-local` sets; sets a pragma under a key no literal reads as
    one; copies the file out with `VACUUM` or points SQLite's files at a directory; loads
    an extension; gathers the planner's statistics (`ANALYZE`, `PRAGMA optimize`,
    `optimize_on_close`); sets `application_id` or `schema_version`; or writes
    `_sqlx_migrations`. A `#[cfg(test)]` arm, field or item hides only itself: the region
    closes at the comma, semicolon or brace that ends what the attribute governs.

19. **The kernel's bounds on a plugin are the catalog's, and a plugin can only lower
    them.** `[kernel]` in `catalog/limits.toml` is generated as `nc_catalog::KERNEL` and
    read by `Limits::catalog()` in `crates/nc-kernel/src/limits.rs`. `#[non_exhaustive]`
    refuses only a struct literal, so an edited copy of `KERNEL` or of a `CapabilitySpec`
    can be made, and has nowhere to go: the kernel's `Limits` keeps its fields private,
    and no kernel function takes a `KernelLimits` or a `CapabilitySpec`
    (`no_function_of_the_kernel_takes_the_catalogs_limits_or_a_spec_from_its_caller`). A
    manifest may lower what `Lowering` names, its store's quota among them, and
    `Limits::lowered` refuses a value above the catalog's or of zero. The compiler refuses
    a zero limit and a relation between two limits that does not hold — a host call
    allowed longer than its call tree bounds nothing — and
    `xtask/tests/the_registry_reaches_the_code.rs` holds every generated field to the
    registry by name.

20. **A command runs only once it is authorized, every dispatch is journaled before its
    answer is returned, and what a command changed never outlives a journal that did not
    take it.** `Kernel::dispatch` and `commands.dispatch` are one door
    (`crates/nc-kernel/src/dispatch.rs`, whose header lists its steps): the decision
    point, the directory and the journal are each asked before a plugin runs, and a
    refusal from any is a denial, never a default. A command that changes state is staged
    in a unit the journal opened first and committed in one transaction only after its
    entry landed; the pruner's deletions, which no unit can stage, are journaled as
    allowed — after a dry run was refused nothing — before anything is deleted; a close
    the journal answers with an error is asked about, and the unit reversed at once unless
    it landed; a read the journal does not take is withheld.

    **What a stopped process left is recovered before anything else runs**
    (`crates/nc-kernel/src/units.rs`'s header has the order), found by asking the journal's
    store which units of the realm are open (`nc_journal::Store::open_units`) and never by
    reading the history. A store that refuses a
    reversal (`kernel.revert_failed`), and a process stopped between the store's commit
    and the journal's close, each leave a unit committed in the store and open in the
    journal: `Kernel::open` reverts every such unit of its realm, newest first, and closes
    it as aborted (`kernel.recovered`, `[NC-KERN-0016]`), or does not open, and while the
    kernel runs, a prune that would reach the units table reverts it first and is refused
    while it cannot. **Every unit whose close no kernel has heard is held from every
    realm's pruner**: the kernels of one person's realms share the file, so a unit — and
    each unit whose state it moves — is committed with a hold in `store_unit_holds`, which
    the pruner's SQLite target skips, let go when the journal's close is heard or the unit
    reverted. The blobs a unit made go with it. What this leaves open is the decision
    point's **Not answered** below.

    **Enforced by** `crates/nc-kernel/tests/the_decision_point.rs` — the journal refusing
    at the opening, at a top-level and a nested entry, at the close and at a read's entry,
    a close taken with its answer lost, the store refusing a reversal, a prune past a unit
    left open, a journal whose store cannot name a unit it holds open, and a kernel stopped
    in a child process between the store's commit and the journal's close and opened
    again, with the pruner run over the file meanwhile or not, over PostgreSQL and over the
    local journal — by
    `crates/nc-kernel/tests/units_and_history.rs`, over every shape a stop leaves, each
    over the journal in memory and over the local journal, and by the operators over
    `dispatch.rs`, `units.rs`, `builtin.rs` and `pump.rs` in `xtask/mutants.py`.

21. **The world the kernel hosts is the reference world, byte for byte.**
    `crates/nc-kernel/wit/noctua-kernel.wit`, which the host's bindings and every plugin's
    (through `crates/nc-guest`) are generated from, is a copy of the workspace's normative
    `reference/noctua-kernel.wit`, which changes first (the *plugin world* row under
    **Making a change**). `the_kernel_world_is_the_reference_world` in
    `xtask/src/check.rs` holds the two equal wherever the workspace is above and prints
    `skipped:` in a checkout, where `WIT_BLAKE3` in `crates/nc-kernel/src/world.rs` holds
    the copy to its digest; `WORLD_VERSION` is held to the copy's `package` line. **Not
    held**: the copy and its digest rewritten together in a checkout, caught the next time
    the tree runs inside a workspace.

22. **A write to the local database is believed for the rows it changed, not for its
    success.** A trigger can answer success for a statement it silenced — a temporary
    `BEFORE INSERT … RAISE(IGNORE)`, or the same trigger added to `main` by another
    connection after the store opened, which neither schema check sees. So every statement
    `crates/nc-local/src/store.rs` and `crates/nc-local/src/journal.rs` write with goes
    through `write_exactly`, which refuses any count but the one the statement had to
    change (`NC-DB-0007`, and the journal's `NC-JRNL-0009`), every transaction first checks
    that no temporary object sits on its connection, and the pruner's SQLite target refuses
    a `DELETE` that removed more than its batch (`crates/nc-prune/src/sqlite.rs`). **Gated
    over the statements written today**:
    `a_write_a_trigger_silences_is_refused_and_leaves_nothing` in
    `crates/nc-local/tests/a_unit_lands_whole_or_not_at_all.rs` silences each of the store's
    in turn and requires the write refused with nothing left, and
    `an_append_a_trigger_silences_is_refused_and_lands_nothing` in
    `crates/nc-local/tests/the_local_journal.rs` the journal's. Each list is its own, so a new write that
    skips `write_exactly` and is not added there is refused by nothing — no gate reads the
    store's source for an `execute` outside it.

## The security answer §2.4 asks for

`NOCTUA.md §2.4` asks, on every implementation, how it could be made more secure, and
requires the **answer** written down: a consideration thought about and not recorded is
re-litigated by the next agent or silently dropped. This is that answer for the ground
this repository stands on. Each entry says what is **guaranteed**, what **enforces** it —
a guarantee enforced by "a reviewer will notice" is a hope — and what it does **not
answer**; the kernel's entries point to the module header that holds each rule's order and
refusal codes. **What is deliberately not claimed**, at the end, holds what no single
entry owns.

**The kernel's answer, threat by threat.** Each row is answered by the entry it names:

| Threat | What answers it | Where |
|---|---|---|
| A plugin reading or writing another's data, or another realm's | every key is the plugin's and the realm's, both bound by the kernel — the plugin from its manifest, the realm from the one it serves — and no plugin supplies SQL | "The local database is private to its user, and refuses what it did not make" |
| A change nobody authorized | a write lands only in the frame of an authorized command that changes state | "A command runs only when it is decided, and the journal hears of it first" |
| A way around the decision point | `commands.dispatch` is the one door, and a nested dispatch runs as its tree's principal | the same entry |
| A forged principal, confirmation or approval | `context` is read-only; a confirmation and an approval are the kernel's alone, bound to one dispatch — its command by the plugin that holds it — and one whole principal, used once and only on a dispatch the kernel decided, short-lived and never inherited | the same entry, and "What a plugin puts off runs only under an authority decided again when it comes due" |
| A command declared in another plugin's name | a namespace one loaded plugin holds, and the ones the kernel keeps | "A plugin is loaded with the commands it may have, and only those" |
| A plugin that will not stop | the epoch, fuel, memory, a host call's deadline, the tree's deadline, depth and count, the refusal of re-entry, bounded queues, timers, surfaces and log, the restart budget and quarantine | "A plugin that crashes, hangs or exhausts its budget is stopped, and harms nothing else" |
| Half-applied state | writes staged and committed whole, recovery each time a kernel opens, and an undo that is a unit of its own | "A command runs only when it is decided, and the journal hears of it first", and "A person undoes their own steps, with the authority they hold now" |
| A request to somewhere it should not go | names the manifest declares, public addresses only, no redirect and no proxy, bounds, and only `GET` and `HEAD` where nothing changes | "A plugin reaches the network its manifest names, and nothing else" |
| Malformed input | canonical JSON and the closed schema, before the plugin runs | "A command runs only when it is decided, and the journal hears of it first" |
| A decision point that cannot decide | a denial, journaled | the same entry |
| An action that escapes the audit | journaled before its answer is returned, and its effects gone when the journal did not take it | invariant 20 |
| User data in the central log | typed events only, and a plugin's text kept in its own ring | "A plugin's own log stays in its own ring" |
| Undo used to escalate | the asker's own steps, authorized again now, refused on a conflict, compensations dispatched through the decision point | "A person undoes their own steps, with the authority they hold now" |
| A rule swapped under the kernel | packs compiled into the binary, and a realm's pin held to them | "The pack a kernel decides with is the one it was built with" |
| Authorization gone stale | decided again on every dispatch, and when a timer or an event comes due | "What a plugin puts off runs only under an authority decided again when it comes due" |
| A history rewritten on the person's machine | append-only triggers, a row only at the next sequence, the link inside the insert, every row read back only as this code writes it | "The journal on the person's machine refuses to be edited, and says which units are open" |
| Another account reading the local database | `0700` and `0600` on Unix, an access list naming the user alone on Windows, owner and link checks, `trusted_schema = OFF`, `main.`-qualified statements and append-only images | "The local database is private to its user, and refuses what it did not make" |
| Another account reaching the daemon's endpoint, or standing in for it | a socket `0600` in a runtime directory judged `0700`, or a pipe made first with an access list naming the user alone and remote clients rejected; the user and the process at the other end read off the system both ways, before a frame crosses | "A daemon's endpoint lets in its user alone, and each end reads the other off the system" |
| Something that never read the token speaking to the daemon, or answering for it | a proof keyed by the token at each end, over both nonces and a domain of its own, compared in constant time, with the token never sent | "Each end of a connection proves it holds the token, and neither sends it" |
| An identifier handed out twice, across a crash or a clock moved back | a ceiling made durable before a millisecond past it is handed out, and a start that resumes above it and the newest unit it hands in, waiting for the clock | "A daemon starts only on a profile that is its user's alone, and mints nothing it cannot resume above" |
| A sandbox escape through this repository's own code | no `unsafe` written anywhere but the Win32 calls of `nc-platform`'s Windows half, each beside its reason, plugins included, and what `forbid` does not reach written down | invariant 17 |

### A plugin reaches what it was granted, and is told no to the rest

**Guaranteed.** A host function of the world runs for a plugin only when its manifest was
granted the capability that function answers to; a plugin refused one is answered `denied`
(`[NC-PLUG-0001]`) as a value it reads, not killed by a trap. A resource's methods are
reached only through a handle a constructor of the same capability hands out, and a
compensation naming a command not the plugin's own asks for `commands.dispatch` too, since
the undo that runs it dispatches it as the person. Each refusal is logged once per
function per instance, by slot and code: `kernel.capability_denied`, or
`kernel.host_call_refused` for a call refused for where it ran or whose name it used — a
write where nothing may change (`[NC-PLUG-0024]`), an act for a person where no call tree
runs (`[NC-PLUG-0025]`), an event under a topic not the plugin's (`[NC-PLUG-0026]`;
`a_write_refused_where_nothing_may_change_is_logged_with_its_code_once`).

**Enforced by** the first statement of every host function in
`crates/nc-kernel/src/host/`, which asks for its capability by a variant generated from
`catalog/capabilities.toml` — the registry a manifest is read against, so an unknown or
repeated capability, a domain without `net`, or a raised limit refuses the manifest whole
(`[NC-PLUG-0010]`); and by `crates/nc-kernel/tests/capabilities.rs`, which reads the world
with its own parser, runs the reference plugin's probe through every function granted
nothing — and granted everything, as a whole kernel's command, meeting no `denied` — and
holds what each host function answered and asked for to the catalog.

**Not answered:** what a granted function may then do, which the entries below say; who a
call runs for and whether it is authorized, which is the decision point's; and whether a
manifest asks for more than its plugin needs, which is its author's and its installer's.

### A plugin is loaded with the commands it may have, and only those

**Guaranteed.** A plugin's commands enter the registry only when every one it declared at
`activate` can be authorized and is its own to declare — or none does, and the plugin is
not loaded (the rules are `crates/nc-kernel/src/registry.rs`'s header). Each is held to
the judge every built-in is held to (`crates/nc-schema`): an action the decision point's
schema declares, a resource type that action applies to, and the whole realm selected only
by a command that reads it, since a permit for a realm-wide change would authorize every
plugin's. Its name sits in the plugin's own namespace, never one the kernel keeps
(`kernel.`, `journal.`, `limits.`), declared once, and matches the catalog where the
catalog describes it. A namespace and an identity belong to one loaded plugin at a time,
so a second cannot declare commands in the first one's name (`[NC-PLUG-0022]`, before
anything is compiled and again after it activates). `activate` runs in a store of its own
on the activation's fuel and memory bound, in no call tree, so it writes nothing and
dispatches nothing (`[NC-PLUG-0024]`) and reads only its own keys. A refusal names every
problem with its own code and the command — by position when the name is missing
(`NOCTUA.md §11.7`) — and whatever it repeats of the plugin's text is cut to a bound with
what a reader cannot see escaped; a malformed name is never repeated.

**Enforced by** `Registry::admit` in `crates/nc-kernel/src/registry.rs`, with a unit test
per rule; `Kernel::load` in `crates/nc-kernel/src/lib.rs`; and
`crates/nc-kernel/tests/activation.rs`, which loads the real component breaking its first
command each way the reference plugin can.

**Not answered:** whether a registered command acts on the resource it is authorized for
inside its own namespace — the plugin's code; and the dispatch itself, the next entry.

### A command runs only when it is decided, and the journal hears of it first

**Guaranteed** — the order of every check and each refusal's code are
`crates/nc-kernel/src/dispatch.rs`'s header, the unit's half
`crates/nc-kernel/src/units.rs`'s:

- **Decided first, denied when it cannot be.** A command, a plugin's or the kernel's own,
  runs only after the decision point allowed it for the caller, on the resource its
  selector took from arguments held to canonical JSON and to its schema; a pack that did
  not build, a directory silent past a host call's deadline, or a realm pinning a pack not
  carried is a denial.
- **A confirmation is the kernel's, for one dispatch**: issued for that principal (kind
  and identifier), command and arguments, redeemed once at the top of the tree within its
  time on both clocks, and spent only once the decision was evaluated. A nested dispatch
  is never confirmed or approved, runs as the tree's principal in the tree's unit, and is
  held to the tree's depth, deadline and count — every dispatch asked for counts, and one
  asked after the plugin was told the count is spent traps it, killing it and its unit. A
  plugin's command dispatching into its own plugin is refused, not trapped.
- **One journal entry per dispatch**, refused or run: the principal, the action, the
  command, the entity, what decided it (a policy's `@id` or a `kernel.` sentinel), the
  pack, the epoch, the grant and the unit. A denial tells the caller its code, never the
  `@id`, which would say whether a resource they cannot read exists.
- **Kept whole or not at all**: a command that changes state runs in a unit the journal
  opened first, its writes staged and committed in one transaction after its entry landed.
  A failure, a trap, a nested failure that changed anything — a write, an event, a timer,
  a surface — and a refused entry or close leave the store, the queue, the timers and the
  surfaces as they were; a read whose entry is refused answers nothing it read; and what a
  tree with no unit puts off is let go if the journal refused any of its entries.
- **Bounded**: a unit stages at most `store_writes`' row bound of keys and
  `[kernel] unit_bytes` of keys and values (`catalog/limits.toml`), each key's earlier
  value counted, so a step kept is one whose undo fits.
- **Run to its end** on a task of its own whoever stops waiting, so no unit is left open
  and no plugin half-called; **recovered, and held from every realm's prune**, as
  invariant 20 says.

**Enforced by** `crates/nc-kernel/src/dispatch.rs`, `crates/nc-kernel/src/units.rs`,
`crates/nc-kernel/src/builtin.rs`, `crates/nc-kernel/src/pump.rs` and
`crates/nc-kernel/src/host/store.rs` with their unit tests;
`crates/nc-kernel/tests/the_decision_point.rs`, which runs every refusal against the real
component, pack and world, reads each back from the journal and the log, runs the
one-entry case against PostgreSQL too, and stops a kernel in a child process before and
between the store's commit and the journal's close, over PostgreSQL and over the local
journal; and `crates/nc-kernel/tests/units_and_history.rs`, over every shape a stop leaves,
over the journal in memory and the local journal alike.

**Not answered:**

- a confirmation or an approval issued for real — only a test door issues one, so a
  shipped kernel denies every command that cannot be undone and every one a quorum must
  approve;
- an agent acting for a person, which the actor chain is for (Stage 15);
- a store that refuses the reversal of a unit whose close the journal did not take: the
  unit stays committed and readable, open in the journal until the next prune or kernel
  opening reverts it, the caller told and `kernel.revert_failed` logged, the units table
  past its bound meanwhile — and if a later kept unit changes what it wrote, the store
  refuses that reversal for good and the kernel does not open until somebody repairs it;
- a journal that answers a close with an error and cannot then say whether it took it: the
  unit is reverted while the journal may record it as kept, and nothing reconciles the two
  (`kernel.unit_aborted`, cause `journal_unknown`, records the reversal);
- two kernels over one journal, each taking the other's open units for a stopped process's
  — one daemon keeps one profile;
- an opening bracket that names its unit in a spelling this code never writes, which the
  journal's store refuses by name (`NC-JRNL-0001`), so the realm's kernel does not open
  until somebody repairs the journal by hand: a party that can only insert writes one on
  the server, where `journal_entries.work_unit` holds any text, and whoever can write the
  local file writes one there;
- the public-read allowlist `NOCTUA.md §14.2` permits, which this kernel does not
  implement: every read is decided by Cedar in process, a bypass would be a second path
  buying nothing, and the web backend's middleware (Stage 22) is the first surface that
  could want one;
- timing between the commands of one tree, which share its deadline and no clock of their
  own.

### The pack a kernel decides with is the one it was built with

**Guaranteed.** The decision point decides with the core and the realm's pack compiled
from text built into the kernel's binary — nothing on the machine is read for a rule — and
builds every request against the schema built in beside them. A realm is decided only when
its profile and its pin both name the pack the kernel carries, `<profile>@<version>`, and
is denied everything otherwise (`[NC-POL-0004]`); a pack that does not compile, validate
or keep the rules every pack is held to leaves no decision point, so every dispatch is
denied in that pack's code (`[NC-POL-0001]`, `[NC-POL-0002]`, `[NC-POL-0007]`) and no
plugin is loaded. Any evaluation error is a denial whatever Cedar answered
(`[NC-POL-0005]`), because Cedar skips a policy whose condition errs and answers from the
rest. The journal records a policy's `@id` or one of the decision point's own sentinels,
and no `@id` may begin `kernel.` (`nc_policy::RESERVED_PREFIX`), so neither passes for the
other.

**Enforced by** `crates/nc-authz/src/embedded.rs`, whose test holds each embedded list to
its directory on disk both ways; `Pdp::embedded` and `Pdp::decide` in
`crates/nc-authz/src/lib.rs` and `crates/nc-authz/tests/the_embedded_pack_decides.rs`,
where an erring rule denies whether Cedar permitted or not; `cargo xtask check`'s policy
step, which runs every case and refuses a realm in `policy/world.json` pinning another
profile's pack or another version of the one carried; and
`crates/nc-kernel/tests/the_decision_point.rs`, which denies through the real component a
dispatch whose pack cannot load and one in a realm whose pin is not carried.

**Not answered:** a kernel binary replaced on the user's machine, which carries its pack
inside it — a process running as the same user is outside what the kernel defends
(`NOCTUA.md §11.1`); whether a changed pack raised its version, which no gate compares
with the history and is a reviewer's question, asked of the pull request's diff; and a
policy that says other than its author meant, which only its cases check.

### A plugin that crashes, hangs or exhausts its budget is stopped, and harms nothing else

**Guaranteed** — the causes, codes and restart rules are
`crates/nc-kernel/src/supervisor.rs`'s header:

- **A store of its own** for every plugin: its own instance and linear memory.
- **Every call bounded** by the fuel and epoch ticks its manifest leaves it, a memory
  guard that refuses with its own error, and `wasm_stack_bytes`. A host call that waits —
  all but `commands.dispatch`, whose command runs under the tree's deadline — is held to
  `host_call_deadline_ms` or what its tree has left, whichever is nearer, and never
  started with nothing left; the directory's questions and a nested dispatch's waits on
  the journal are held the same way.
- **The epoch counts the plugin's own running**: host time is counted apart and given back
  (the plugin's allocator, run inside a host call answering a string or a list, counting
  as the plugin's), and the whole call stays bounded by the tree's `dispatch_deadline_ms`
  — waiting on a slow answer is not killed, calling the host without end is.
- **Killed in the code of its cause** (`[NC-PLUG-0002]` to `[NC-PLUG-0006]`,
  `[NC-KERN-0007]` for the tree's deadline), its store dropped and its tree's unit doomed;
  **restarted** within the tree's time, or before the next tree or pump runs anything, so
  no tree outlives its deadline waiting for it.
- **Quarantined** when it does not come back as it was or is killed `restart_budget` times
  inside `restart_window_ms` (a kill at the tree's deadline does not count — the host
  spends that time too): `unavailable` (`[NC-PLUG-0007]`) until `Kernel::release` starts
  it once, however many releases arrive, on a task of its own. Nobody calls it back
  meanwhile: its due timers wait, and the events it would hear are dropped and counted.
- A plugin dispatching into itself is refused as a value (`[NC-KERN-0003]`), not killed.

**Enforced by** `crates/nc-kernel/src/supervisor.rs`, `crates/nc-kernel/src/engine.rs`,
`crates/nc-kernel/src/limits.rs` and the call hook and epoch callback
`crates/nc-kernel/src/bindings.rs` gives every store, with their unit tests;
`crates/nc-kernel/tests/isolation.rs`, which kills the reference plugin each way beside
its peer and holds the peer's status, instance and data unchanged, and releases a
quarantined plugin twice at once and once after a release given up on;
`crates/nc-kernel/tests/units_and_history.rs`, where a unit a kill stopped — nested in
another plugin's command, or over a file on disk — leaves nothing; and
`crates/nc-kernel/tests/the_rest_of_the_world.rs`, where a timer's callback on a stalling
journal still lets the pump end within the tree's deadline.

**Not answered:** what a host call was doing when its deadline passed, which goes on
inside the kernel and is then dropped; the kernel's own waits at the top of a tree — the
entry and close of a top-level dispatch, a callback's at the pump — and the store's commit
of a step a nested dispatch opened, each waiting as long as the journal or store takes
with the bus held, since the first is the record that the tree ran and a commit cancelled
half-way may keep what the journal is told was not; a plugin calling the host without end
inside its fuel, killed at its tree's deadline each time and never quarantined for it; a
tick arriving late on a machine too loaded to run the ticker, which only makes a deadline
later; and a plugin within every bound that is merely slow, or slows its neighbours
through the machine they share.

### A blob is said to be where it is, and goes with the unit that made it

**Guaranteed** — the two halves, bytes and rows, are `crates/nc-kernel/src/blobs.rs`'s
header:

- **Named by its bytes** (BLAKE3); any other name is refused before anything is read
  (`[NC-BLOB-0001]`).
- **Said to be where it is**: `blobs.locate` answers `local` only when this machine holds
  both the row — pinned by a committed unit, or made by the asking tree's unit — and the
  bytes, `unavailable` otherwise, and **never `remote`**, no peer being asked before Stage
  25's network; `open` of a blob not here is `unavailable` at once (`[NC-BLOB-0002]`), and
  `announce` says no peer heard it (`[NC-BLOB-0006]`).
- **Bounded**: `blob_read_chunk_bytes` a read, `blob_bytes` a blob (`[NC-BLOB-0003]`),
  `blob_handles` open readers and writers a plugin (`[NC-PLUG-0017]`).
- **Written whole, and gone with its unit**: bytes are written only in a command that
  changes state (`[NC-PLUG-0024]`), synced, and named by their hash once whole; rows are
  made only when the unit commits. A unit let go deletes the bytes it alone made — a
  `finish` reads its row before naming, and bytes a failed or abandoned `finish` named are
  swept — and a unit the journal does not keep has its pins taken off in its reversal (a
  redo's, only those no other unit holds).
- **Undone and redone with its step**: an undo unpins each blob its step made that no
  other committed unit holds; a redo pins each again, refused (`[NC-KERN-0019]`) when the
  sweep took it meanwhile.
- **Swept after a grace**: `Kernel::sweep_blobs`, holding the bus, deletes what no unit
  has held for `blob_orphan_grace_ms` — not a blob the pruner unpinned, which its unit's
  reversal pins again in any realm sharing the file — then unnamed bytes and abandoned
  writings once nobody has named them for that grace; a unit in flight names its bytes, so
  another realm's sweep never takes them.
- **Private, and waited for under a deadline**: the directory follows
  `nc_local::blob_directory`, every file is created private
  (`nc_platform::create_private`: `0600` on Unix, an access list naming the user alone on
  Windows), a directory under a blob's or a
  writing's name stops no sweep, every blob function that reads disk or store waits under
  `host_call_deadline_ms`, and a writer let go is deleted on the blocking pool.

**Enforced by** `crates/nc-kernel/src/blobs.rs` and `crates/nc-kernel/src/host/blobs.rs`
with their unit tests; `crates/nc-kernel/tests/where_a_blob_is.rs`, over a blob directory
on disk, with a sweep held to wait for a call tree at its commit and no bytes named for a
finish whose row was refused; `crates/nc-kernel/tests/units_and_history.rs`, where a
command killed every way or nested and failed leaves no blob, another realm's sweep leaves
the bytes a unit in flight made, and a blob's pin follows undo and redo;
`crates/nc-kernel/tests/isolation.rs`, where a blob host call that never answers kills its
plugin at the deadline; and `crates/nc-kernel/tests/the_decision_point.rs`, where a unit
the journal did not keep has its blobs' pins taken off and a blob another unit holds keeps
its own.

**Not answered:** who may read a blob — whoever holds its hash and `blobs.read`, since
content-addressed bytes carry no owner (`NOCTUA.md §13`); a process running as the same
user, which can replace the bytes under a hash the kernel does not hash again on reading;
two stores of blobs over one directory, each sweep taking the other's writings in progress
for a stopped process's — the kernels of one profile share one; the same bytes made by two
realms' units in flight at once, deleted under one when the other is let go; the moment
between a sweep finding bytes old and deleting them, and a wall clock stepped on by a
grace, which makes bytes in flight old; and the disk a plugin granted `blobs.write` fills,
within `blob_bytes` a blob and the units' bounds.

### A plugin reaches the network its manifest names, and nothing else

**Guaranteed** — the rules in the order they are judged, with each refusal's code, are
`crates/nc-kernel/src/net.rs`'s header, and the transport's
`crates/nc-kernel/src/net/ureq.rs`'s:

- **Declared names over HTTPS, nothing else**: a name the manifest declares under
  `[net] domains`, or one below a `*.` declaration and never its apex, compared lower-case
  in IDNA form as the WHATWG parser writes it. A URL that is not `https`, names an address
  in any spelling or `localhost`, holds a host that is no DNS name, or carries a user, a
  password or a port is refused before any transport is called (`[NC-PLUG-0013]`), and an
  undeclared name is `[NC-PLUG-0012]`.
- **Only what HTTP and the kernel allow**: the methods `GET`, `HEAD`, `POST`, `PUT`,
  `PATCH` and `DELETE` as written; no header the transport writes or one reaching past the
  request (`NOT_SENT` in `crates/nc-kernel/src/net.rs`), none that is not HTTP, no body on
  a `GET`; headers, head and body within `net_header_count`, `net_request_head_bytes` and
  `net_request_body_bytes` (`[NC-PLUG-0017]`).
- **A change leaves a note first**: `activate`, a command that only reads and a callback
  may `GET` and `HEAD`; any other method is sent only from a command that changes state,
  whose unit notes it — method and host only — as having left the machine before it is
  sent, dropping the note only when the transport refused before writing any of it
  (`TransportError::sent_nothing`).
- **Public addresses, and nothing from the environment**: the transport dials only what
  `is_public` accepts — IPv4 less every special-purpose block, IPv6 global unicast less
  the blocks inside it, the well-known NAT64 prefix judged as the IPv4 address it carries
  — reads no proxy, follows no redirect (a `3xx` reaches the plugin, and following it is a
  new request judged again), reads no body past `net_response_body_bytes` and builds no
  decoder, waits no part of a request past `net_timeout_ms`, trusts only its compiled-in
  certificate authorities and presents no certificate, refuses as `[NC-PLUG-0013]` a
  request its HTTP cannot write rather than calling it a network that did not answer, and
  names itself `noctua-kernel` without a version unless the plugin sets a `User-Agent`.
- **Answers bounded, refusals quiet**: answers are held to the catalog's bounds again
  (`[NC-PLUG-0014]`); a request that does not complete is `[NC-PLUG-0015]` in one
  sentence, whatever stopped it, so a plugin learns nothing of which ports answer; a
  transport silent past `host_call_deadline_ms` is abandoned and its plugin killed
  (`[NC-PLUG-0006]`); and `net.refused` is logged once per reason per instance, never with
  the URL, the host or a header.

**Enforced by** `crates/nc-kernel/src/net.rs`, `crates/nc-kernel/src/net/ureq.rs` and
`crates/nc-kernel/src/host/net.rs` with their unit tests — the transport read back as
built (`the_agent_is_built_with_every_bound_and_no_default_that_reads_the_world`), against
a trap on loopback, a name answering public and loopback addresses together, a wire
keeping every byte ureq writes, and a child process given `HTTPS_PROXY`;
`the_kernels_http_client_is_built_with_no_decoder_and_no_cookie_jar` in
`xtask/src/members.rs`, over the features cargo resolves for ureq;
`crates/nc-kernel/tests/net.rs`, which sends the reference plugin's requests through the
decision point to a transport that keeps what it is handed; and
`crates/nc-kernel/tests/isolation.rs`, where a fetch that never answers kills its plugin
at the deadline.

**Not answered:**

- what a host a plugin may reach does with what it is sent — a plugin granted `net` and a
  domain can send that domain anything it can read, which is why both are its manifest's
  and a person's to accept;
- a header that host trusts: headers are refused by name and admitted by none, so
  `Forwarded`, `X-Forwarded-For`, `Via`, a cookie and a plugin's own `User-Agent` are sent
  as the plugin set them;
- what DNS says a declared name is, which its owner decides — this machine's own public
  addresses among it: a global IPv6 address of its interfaces, a neighbour's, or a NAT's
  public IPv4 address reached by hairpin is public to `is_public`, so only the
  certificate's verification keeps what answers there from the plugin; refusing them needs
  the machine's interfaces read as a name resolves, which no crate here does (a
  dependency, the developer's to decide, `NOCTUA.md §2.3`);
- an address under a network-specific NAT64 prefix, which a network's DNS64 may synthesise
  and which may carry a private IPv4 address (RFC 6052 §3.1): `is_public` judges it as the
  IPv6 address it is, and judging it otherwise needs the network's prefixes discovered
  (RFC 7050), which nothing here asks;
- a lookup ureq's own resolver goes on with, on a thread of its own, past its timeout;
- a request that left the machine inside a unit then let go, which stays sent while its
  note goes with the unit;
- what a compensation registered for a step that left the machine does to the host it
  reaches, which is its plugin's — undo dispatches it, and stops at a step with none;
- the transport against a real host or a TLS handshake with one, which no test reaches —
  the gate does not depend on the network — so certificate verification is held by the
  configuration read back, not by a host presenting a bad certificate.

### A person undoes their own steps, with the authority they hold now

**Guaranteed** — the steps in order, with each refusal's code, are
`crates/nc-kernel/src/history.rs`'s header:

- **Ordinary commands, the asker's own steps**: `kernel.undo` and `kernel.redo` are
  decided as `Write` on the realm, journaled, run in a unit of their own, at the top of a
  tree only (`[NC-KERN-0017]`). Undo takes the person's newest committed step among their
  newest `undo_history` steps of the realm (undos and redos push none out of reach); redo,
  their newest undo among their newest `undo_history` units.
- **It puts back what the kernel keeps**: every key the step wrote, refused when one
  changed since (`[NC-KERN-0011]`), and the pin of every blob the step made that no other
  committed unit holds — a unit the pruner took holds what it made for good, and a blob
  the undo's own compensation makes again is held by the undo. It **stops where the step
  left the machine** — a request asking a host to change something, a command that cannot
  be undone, one the kernel runs on the realm — unless a compensation is registered
  (`[NC-KERN-0012]`, counted in `beyond_reach`); a timer or an event the step set going
  stays.
- **Authorized again, now**: every action the step was allowed, its nested commands'
  included, is decided again as the person with what they hold now and were issued for the
  undo (a quorum's approval of a `Delete` among it), and a step the journal does not
  record as kept is not reversed (`[NC-KERN-0022]`).
- **Compensations through the decision point**, last first, nested in the undo's unit,
  never confirmed: one that fails, or lets the undo's unit go, leaves nothing of the undo
  (`[NC-KERN-0014]`), and none runs when one's command is now held by another plugin or by
  none (`[NC-KERN-0023]`), or names another plugin's command whose registrant is no longer
  loaded with `commands.dispatch` (`[NC-KERN-0024]`).
- **A compensation is judged when registered**: a registered command of the plugin's own,
  or another's only with `commands.dispatch` (`[NC-PLUG-0001]`), that changes state, that
  the core holds to no confirmation and no quorum, and that is not the kernel's own, its
  arguments held to its schema (`[NC-KERN-0021]`); at most one fewer than
  `dispatches_per_tree` per unit (`[NC-PLUG-0017]`), and a step keeps two of
  `store_notes`' notes free for its undo.
- **Redo** puts back the newest undo unless a step came after it, the undo dispatched
  compensations (`[NC-KERN-0013]`) or the pruner took the step, re-pinning its blobs
  unless the sweep took one (`[NC-KERN-0019]`). A reversal the kernel commits itself is
  nobody's undo; an undo or redo the journal does not take is reversed with what it moved.
- **A unit a plugin begins is its command's**, flattened: its first label is what the step
  is shown as, made safe and bounded; its abort, a dropped handle or one left open dooms
  the tree's unit, at most `unit_handles` at once; and a unit not kept runs none of its
  compensations and names them in its refusal.

**Enforced by** `crates/nc-kernel/src/history.rs`, `units.rs`, `dispatch.rs` and
`crates/nc-kernel/src/host/commands.rs` with their unit tests;
`crates/nc-kernel/tests/units_and_history.rs`, which steps the reference plugin's notes
back and forward across two plugins, over the database in memory and in a file opened
again; `crates/nc-kernel/tests/the_decision_point.rs`, where the journal does not take an
undo's or a redo's close or cannot say how a step closed, and a unit not kept names its
compensations; `crates/nc-kernel/tests/isolation.rs`, where a unit a kill let go names
them; and `crates/nc-kernel/tests/both_stores_answer_alike.rs`, for which units hold a
blob and which are a person's steps.

**Not answered:**

- a compensation that ran before one that failed, whose effect outside the machine stays,
  and one that does not set right what its step did, which is its plugin's to get right;
- a compensation a pack's rule holds to a quorum on its resource — the `family` pack's on
  a `Share` or a `Grant` of private content — registered and refused at every undo while
  the rule holds (`[NC-KERN-0014]`), so it bars its step's undo (narrowed on purpose;
  **What is deliberately not claimed**);
- a unit forgotten past the history's bound, which no longer holds the blobs it made —
  nothing forgets a person's units in this build;
- a compensation that dispatches commands of its own, each spending the undo's tree's
  count;
- how far back a redo reaches — the newest `undo_history` units of every kind, so more
  than half that many redos in a row leave older undos out of reach, their steps undone;
- which commands are registered, which a plugin without `commands.dispatch` learns by
  being refused one as a compensation;
- an undo whose compensations write more bytes than its step left room for, or note more
  than it did, refused as their failure (`[NC-KERN-0014]`);
- an undo or a redo whose close the journal did not take and whose reversal the store
  refused, committed until the next prune or kernel opening reverts it — a redo of such an
  undo is refused meanwhile.

### What a plugin puts off runs only under an authority decided again when it comes due

**Guaranteed** — the pump's order is `crates/nc-kernel/src/pump.rs`'s header:

- **Shaped and bounded**: a topic is the publisher's own, lower-case words in its
  manifest's namespace (`[NC-PLUG-0026]`); a timer waits at least a millisecond or repeats
  no faster than `timer_min_interval_ms`, never beyond `timer_max_delay_ms`, under a
  bounded tag and an identifier from one counter that never goes back; the queue holds at
  most `event_queue` events and `event_queue_bytes`, a plugin at most `timers_per_plugin`
  timers, what a unit holds counted (`[NC-PLUG-0017]`); a cancel naming a timer the plugin
  does not hold is not found (`[NC-PLUG-0019]`).
- **The unit's until it is kept**: inside a unit, an event or a timer is queued, armed or
  cancelled only when the unit is kept, and a nested command that fails having put one off
  dooms the unit, so nothing hears of or is woken by a change that did not happen; a tree
  with no unit lets them go if the journal refused any of its entries.
- **Decided again when due**: `Kernel::pump` fires due timers, then the events queued when
  it began, each once, oldest first, each only after the decision point decides again —
  for the principal it was set or published for — the authority it was set or published
  under; a denial is journaled and cancels the timer or drops the event.
- **A callback is a call tree of its own** for that principal, never confirmed, every
  command beneath it nested and never undo or redo; an event's callback, and a timer's set
  where only reading was allowed, change nothing and begin no unit, so putting a change
  off escapes no frame that would refuse it.
- **Supervised like a command**: a callback that traps is killed and restarted; a
  quarantined plugin's due timers wait and its events are dropped and counted; an unloaded
  plugin's timers are cancelled, its surfaces closed and its unheard events let go.
- **Who a call runs for is the kernel's to say**: `context` answers the principal's opaque
  identifier, an empty actor chain, and whether a person confirmed the dispatch at the top
  of the tree — never nested, never in a callback — and nothing a plugin writes changes
  them; in `activate`, the context, events, timers and surfaces are refused
  (`[NC-PLUG-0025]`).

**Enforced by** `crates/nc-kernel/src/events.rs`, `crates/nc-kernel/src/timers.rs`,
`crates/nc-kernel/src/pump.rs`, `crates/nc-kernel/src/units.rs` and the host interfaces
over them, with their unit tests, and `crates/nc-kernel/tests/the_rest_of_the_world.rs`,
which publishes, sets and pumps the reference plugin beside its peer — authority withdrawn
in between, a unit let go, a nested command that put something off and failed beneath a
caller that went on, a timer set by a read or an event's callback, a plugin quarantined,
killed or unloaded.

**Not answered:** what a callback does with what it is told, which is its plugin's code; a
callback changing its own plugin's state, which it cannot — a command of that plugin
dispatched from it is a dispatch into itself — so a timer reaches its plugin's data only
through another plugin's command; timers and events across a restart of the kernel, which
keeps none on disk in this build, and when the pump runs, which is the daemon's (Stage
13); an event published or a timer set by a command that only reads and then trapped,
queued or armed when its tree ends because the trap's entry was taken — such a timer only
reads when it fires; and one plugin publishing the queue full, which refuses every other
plugin's events until the next pump: the queue's bounds are the kernel's, with no share a
publisher's own.

### What a plugin shows is a tree the kernel judged, naming commands the registry holds

**Guaranteed.** A surface shows only a tree of the closed set of nodes
`crates/nc-kernel/src/ui.rs` documents — no markup, no script, no style and no address: an
image names a blob by its BLAKE3, and a text is text — held to `ui_tree_nodes`,
`ui_tree_depth` and `ui_text_bytes`, each text showing every character but a line break.
Every interaction names a command the registry holds when the tree is rendered, gives only
arguments its schema names — never one both given and bound — and binds only inputs of the
same tree, each named once. A tree is refused whole (`[NC-PLUG-0016]`), naming the place
in it, and the surface keeps the last tree the kernel kept, which `Kernel::surfaces` lists
for a renderer. A surface opened, a tree rendered and a close are held by the call tree as
an event is — shown when its unit is kept, or when a tree holding none ends with every
entry taken — and only the last render per surface is held, so rendering in a loop holds
at most `payload_bytes` per surface
(`one_plugins_renders_in_one_call_tree_hold_a_tree_per_surface_it_holds`). A plugin holds
at most `surfaces_per_plugin` surfaces, each under a title a person can read, and another
plugin's surface is not found to it.

**Enforced by** `crates/nc-kernel/src/ui.rs`, a test per rule; its interface in
`crates/nc-kernel/src/host/ui.rs`; `crates/nc-kernel/tests/the_rest_of_the_world.rs`; and
`crates/nc-kernel/tests/the_decision_point.rs`, where a read the journal did not take and
a unit it did not keep each show nothing they did.

**Not answered:** the click, which a renderer dispatches through the decision point as the
person who clicked and never as a confirmation — no renderer exists in this build (Stage
16); whether this machine holds the blob an image names, which is the renderer's to ask
when it draws, since a blob can arrive or be swept after the tree is kept; and what a tree
says, which is its plugin's.

### A plugin's own log stays in its own ring

**Guaranteed.** A line a plugin writes with `log.write` is kept in a ring of
`log_ring_lines` lines its slot holds, outliving its kills — cut at a character boundary
to `log_line_bytes`, every character a reader cannot see replaced, the oldest let go and
counted — and read through `Kernel::plugin_log`. Nothing a plugin writes reaches the
central log, whose events carry no text.

**Enforced by** `crates/nc-kernel/src/host/log.rs` and its tests, and
`plugin_log_lines_stay_in_the_ring` in `crates/nc-kernel/tests/the_rest_of_the_world.rs`,
which writes past the ring and holds the central log, and every field it was given — a
kill's among them — to carry none of the plugin's text.

**Not answered:** what a line says, which its plugin chose.

### The local database is private to its user, and refuses what it did not make

`crates/nc-local` keeps the plugin store — what each plugin holds, and the units that
wrote it with their before and after images — in one SQLite file in a directory the daemon
names (`NOCTUA.md §4`, "Local app database"). What opening does, in order, is that crate's
header.

- **Another account on the machine** cannot read or rewrite it: the directory and the
  file are created private by the crate itself, before SQLite sees them, through
  `crates/nc-platform` — the one place each system's answer is written. On Unix the
  directory is `0700` and the file `0600`, and the `-wal` and `-shm` companions take the
  file's mode. On Windows each is created owned by the user the process token names,
  with a protected access list holding one entry, that user with every right — on the
  directory inherited by what is made in it, which is how SQLite's companions are the
  user's alone too — and the file with `CREATE_NEW` and `FILE_FLAG_OPEN_REPARSE_POINT`.
  One that already exists is opened only if it is not a link (on Windows, no reparse
  point of any kind), is owned by the process's user, lets no other user in (no group or
  other bit; an access list, every allow entry of which names the user) and gives its
  owner what the store needs (on Windows also a file not marked read-only, that system's
  `0400`, which SQLite would open read-only); anything else is refused (`NC-DB-0006`) and
  left exactly as it was — loosening it would decide for the user that whatever made it
  was harmless. On Windows the companions SQLite makes are the one thing taken from
  another owner — the one the process token gives what the process creates
  (`nc_platform::judge_made_by_default`) — and the directory, the file and the lock,
  which this crate makes owned by the user, are held to the user alone. The
  same tests stage each refusal on both systems through `nc_platform::testing`, so a Unix
  test and its Windows twin are one body. A path the filesystem cannot answer for — a
  missing parent, a full disk, a read-only mount, a file turned read-only under an open
  store — is `NC-DB-0001` (`a_missing_parent_is_unavailable_and_not_refused`).
- **Two opens of one profile** — a second app instance, a command-line tool beside the
  daemon — each use an owner probe of their own
  (`one_profile_opened_from_several_threads_at_once_opens_every_time`) and take turns
  under an exclusive lock on the directory — on Windows, which locks no directory, on a
  private file beside the database, `nc_local::LOCK_NAME` — while the file is judged and
  migrated, because
  SQLite refuses two connections turning a new file's journal into a write-ahead log at
  once and `sqlx` takes no lock around a migration
  (`a_fresh_profile_opened_from_several_threads_at_once_opens_every_time`,
  `an_unmigrated_file_opened_from_several_threads_at_once_opens_every_time`); the wait is
  on the blocking pool, so opens awaited together on one runtime take turns too
  (`opens_of_one_profile_awaited_together_on_one_runtime_each_open`).
- **A file that is not the one this build expects** is refused (`NC-DB-0007`) — another
  application's header, a history of migrations this build did not write, a damaged page,
  a schema one object or one word away from the list — and **nothing it refuses has been
  written to**: every check runs before the migrator, on a read-only connection of its own
  that leaves the journal mode alone, and a file one migration behind is held to what its
  own history built
  (`a_damaged_file_one_migration_behind_is_refused_before_the_migrator_writes`) and to
  that history being this build's — no version it does not carry, none recorded as other
  SQL, none that did not finish, which is told as the migration that failed (`NC-DB-0002`)
  — before the pool connects
  (`a_newer_builds_file_is_refused_by_its_version_and_left_as_it_was`,
  `a_migration_edited_after_it_ran_is_refused`). The file and its journal are left byte
  for byte (`another_applications_wal_file_is_refused_and_left_as_it_stood`,
  `a_damaged_file_with_frames_in_its_wal_is_refused_and_left_as_it_stood`), and a file
  beside a hot rollback journal, which this build never leaves, is refused with that
  journal not rolled back (`a_file_beside_a_hot_journal_is_refused_and_left_as_it_stood`).
- **Images are append-only and every write is counted**: the triggers hold under
  `trusted_schema = OFF` and their refusal is recognised by SQLite's extended code 1811;
  every statement names `main.`, every write checks the rows it changed, and every
  transaction first checks that no temporary object sits on its connection (invariant 22).
- **A half-written unit** cannot be left: a unit's writes live in the kernel until it
  commits, in one `BEGIN IMMEDIATE` held to what the unit read key by key — proved by a
  child that aborts the process before each statement of a commit in turn and opening the
  file after every one.
- **A plugin reaching another's keys, or another realm's**: every read and write names the
  plugin, which the kernel binds from the manifest, and the realm the kernel serves
  (`NOCTUA.md §10.3`), and no plugin supplies SQL — so what a plugin keeps for one realm
  is never read or overwritten through another's decision point, though the kernels of one
  person's realms share the file
  (`what_a_plugin_keeps_in_one_realm_is_not_read_in_another`,
  `a_write_in_one_realm_does_not_change_what_another_keeps`). Their journals share the
  machine's one minter (`nc_journal::Journal::sharing`), since minters of their own on one
  flake would number two units alike in one millisecond
  (`two_realms_sharing_their_machines_minter_each_keep_their_units`), and a kernel's
  recovery reverts only its own realm's units
  (`a_realms_recovery_leaves_another_realms_unit_of_the_same_number`).

What it does not answer is under **What is deliberately not claimed**.

### The journal on the person's machine refuses to be edited, and says which units are open

`nc_local::LocalJournal` (`crates/nc-local/src/journal.rs`, whose header has the rules)
keeps the hash-chained history of every realm the profile holds in the local file, beside
the plugin store: `local_journal_entries`, built by
`local-migrations/0004_local_journal.sql`, with the server's columns and bounds.

**Guaranteed:**

- **Append-only and dense, by the table.** `BEFORE UPDATE` and `BEFORE DELETE` triggers
  refuse an edit and a deletion from any connection — a `DELETE` with no `WHERE` included,
  since a table with a delete trigger is never truncated — under `trusted_schema = OFF`,
  and a `BEFORE INSERT` trigger refuses a row anywhere but at the next sequence (the
  server's `0010`), so a row planted at the top of the range cannot end the history. The
  sequence, the hash and the link are held to the server's bounds by `CHECK`s too, which a
  connection can switch off with `PRAGMA ignore_check_constraints` and no schema change;
  no pragma switches off a trigger.
- **One statement per append.** The link, the sequence and the unit's brackets are the
  `WHERE` of the `INSERT`, inside a `BEGIN IMMEDIATE` holding the file's one writer, and a
  refused entry is diagnosed in the same transaction — so every refusal is the one the
  store in memory gives, and `Contended` is never this store's answer. The count goes
  through `write_exactly` and the temporary-object check comes first (invariant 22).
- **A row this code did not write is named where it lies.** `nc_journal::Entry` stays
  sealed: each row is read back through `Entry::chain` over its stored link and held to
  its stored hash, and every column the chain hashes only as the text this code writes.
  One that fails is where `from` stops and what `tail` refuses as `Undecodable`
  (`NC-JRNL-0001`), at its own sequence, and `journal.verify` reports it there as a
  tampered row (`NC-JRNL-0002`).
- **Which units are open is asked of four columns** — the realm, the unit, the action
  and the outcome, over the index `(realm, work_unit)` — so no row that fails to decode
  hides a unit opened after it, and a recovery visits the realm's entries inside units,
  never decoded, rather than every realm's history. An opening bracket whose unit is not
  spelled as this code writes one is refused by name, never skipped (`NC-JRNL-0001`).

**Enforced by** `crates/nc-local/tests/the_local_journal.rs` — the triggers from a
connection of its own, rows planted past the next sequence with the checks on and off, a
forged link, an insert a trigger silences, a planted row nothing can decode, one whose
hash is not its contents' and an opening bracket naming its unit as this code never
writes one, a history read back by the next open, and the transcript held to the store in
memory over written and generated scripts and to PostgreSQL's where the stack runs — by
the kernel's recovery suites run over it (`units_and_history.rs`, and in
`the_decision_point.rs` the stopped child, with and without the pruner run over the file
meanwhile, and the kernel that does not open over that bracket), and by the operators over
`journal.rs` in `xtask/mutants.py`.

**Not answered:** a process running as the same user, which can drop the triggers and edit
or delete any row — SQLite has no roles; a shortened history or a rewritten tail, which a
chain cannot show without an anchor off this machine (`NOCTUA.md §14.9`; `nc-journal`'s
header); a party that can only insert choosing how an open unit closed, as on the server;
the same party inserting an opening bracket that names its unit in a spelling this code
never writes, which keeps every kernel of that realm from opening until somebody repairs
the file by hand — refused rather than skipped, because skipped it might be the unit a
stopped process left; and retention, which is the realm's pack's to state and nothing
applies yet (`catalog/limits.toml`, `policy-pack`).

### The central log cannot contain user data

Not "must not". **Cannot.** `LogValue` is implemented for integers, booleans, durations,
error codes, opaque identifier newtypes, Snowflakes, `Variant`, `Literal` and `Option` of
any of them, and for **no string type at all** — not `&'static str` either, since
`String::leak` manufactures one out of anything. Every event struct is generated from
`catalog/log-events.toml`, whose field kinds cannot carry free text (`NOCTUA.md §15.4`).
**The wall is const evaluation**: `Literal::of::<S>()` reads `S::TEXT`, an associated
**const**, which exists before any run-time value —
`const TEXT: &'static str = format!("{email}").leak();` is *cannot call non-const fn in
constants*. Each shape that must stay refused has a compile-fail fixture, named in
`wall.rs`'s `SHAPES` and declaring the diagnostic it must produce
(`ls crates/nc-log/fixtures` says how many), and the source gate at logging call sites is
the second line. A reviewer sees one diff; this holds every line written from here on, the
ones written at three in the morning during an incident included.

**What it does not do**: stop a caller who sets out to encode text into a declared type,
which no arrangement of types can. `Snowflake::from_crockford` takes **one to thirteen**
caller-chosen Crockford base32 characters — the decoder enforces only
`length > MAX_WIDTH`, and `MAX_WIDTH` is thirteen — and gives back a value that encodes to
them (zero-padded to `MIN_WIDTH`, which recovers a shorter input as well): an eleven-digit
CPF fits. The source gate refuses that conversion **at the call**, where it is visible; a
conversion done elsewhere is beyond every wall, and the answer is review and the journal,
which records who did it.

**Who logs.** The kernel is the crate outside `nc-log` that logs, through its `LogSink`
(`crates/nc-kernel/src/logging.rs`), and the wall reads every file it compiles, tests
included. `check` prints how many crates outside `nc-log` may log — saying why beside the
number when that is none — and how many logging call sites it read; `nc-log` is always in
scope, because its own source is where a door beside the wall would be reopened.

**The lines go to stderr unless the store said it took them.** When a batch's fate is
unknown, losing no line and duplicating none pull against each other, and `ship.rs`
answers once: *a duplicate is recoverable and a loss is not*, and `NOCTUA.md §18.5`'s
argument for keeping the data on this machine holds only if the failure path keeps it
here. So a store that stalls, reads part of a request or never replies costs duplicates on
stderr, never a lost line. Whether to write lines out and whether the store has *gone* are
two questions: a silent store keeps its lines **and** is not counted against, and only a
peer that closed is concluded gone (`GIVE_UP_AFTER`). The shutdown shortens the connect
and the write (`bounds`) and the answer (the `abandoning` flag); only the last is reached
in ordinary running, as `bounds`' own doc says.

### The chain is over bytes, not over formatting

A journal entry's hash is over its **canonical** JSON (`NOCTUA.md §16.4`: RFC 8785 key
order and escaping, no floats) with length-prefixed domain separation, so a digest never
changes because a serialiser did, and a crafted domain and payload cannot move the
boundary between them. Every `u64` enters the hashed form **as text** — cast to `i64`,
every value at or above 2⁶³ would hash alike.

### The stored row is the row that was hashed, and the table refuses to be edited

**The table.** `journal_entries` refuses edits through `BEFORE UPDATE`, `BEFORE DELETE`
and `BEFORE TRUNCATE` triggers, each `ENABLE ALWAYS` — `CREATE TRIGGER` leaves them
*origin only*, which one `SET session_replication_role = replica` switches off without a
trace, and a row trigger never fires on `TRUNCATE` — so a second connection, a console, a
migration or a person with `psql` meets the same refusal in any session role. `0010` adds
`journal_entries_sequence_is_the_next_one`, a `BEFORE INSERT` rule that closes
truncate-and-refill (`NOCTUA.md §14.9`), so `pg_trigger` holds one row more than there are
refusals; a query of `pg_trigger` for the table answers which triggers it has.

**The row.** The chain hashes a value and the table stores its text, so a reader accepting
more spellings than the writer emits is a row that can be respelled without moving the
digest — `ALTER TABLE … ALTER COLUMN … TYPE text USING` rewrites the heap with no row
trigger. All three such readers are round-trip checked: `work_unit`, whose Crockford
decoder is lenient; `resource`, whose parser accepts more than it emits; and `actor_chain`
(`NOCTUA.md §14.6`), safe only while `jsonb` normalises it.

**What `check` reads of the table, against the live database**: the protections
`PROTECTIONS` in `xtask/src/check.rs` writes down, compared both ways, the run printing
how many. Each is read as a definition, never as a hand-kept list of fields:

- every trigger as `pg_get_triggerdef` renders it, so a `WHEN` clause, an `UPDATE OF`
  list, deferrability and transition tables are compared — a trigger re-created with a
  `WHEN` that never matches renders identically under a field list;
- the flags the executor consults — `pg_class.relhastriggers`, `relchecks`,
  `pg_index.indisready` — any of which disarms triggers, `CHECK` constraints or a key's
  uniqueness with every definition unchanged;
- inheritance, both ways, expecting no rows in `pg_inherits`: a child made with
  `CREATE TABLE … () INHERITS (journal_entries)` carries none of the parent's triggers,
  constraints or key, its rows are journal rows to every reader (every statement in
  `crates/nc-journal/src/postgres.rs` names the relation unqualified), and one row near
  the top of the sequence range ends the journal for good;
- `relpersistence` (an `UNLOGGED` journal empties on an unclean shutdown and leaves WAL,
  replication and backup), `relacl` (`GRANT ALL … TO PUBLIC`), `pg_attrdef` (`masked`'s
  default, `NOCTUA.md §15.6`'s VPN signal), foreign keys and exclusion constraints, the
  sequence behind `access_records.recorded` with its `seqcache` and every `recorded` value
  distinct, and column-level collation, which `format_type` omits.

**What none of this reaches**: `DROP TABLE`, `ALTER TABLE … DISABLE TRIGGER` and
`DROP TRIGGER` — each leaving a catalog trace a reader can find — and anything done as a
superuser with `pg_catalog` writes enabled. All of them need table ownership or more, and
the answer to them is a role that owns nothing, which is `§18`'s to give and not a
migration's.

### A record the journal refuses is the caller's defect, not the store's

Every rule about what a journal accepts belongs to the journal, not to one store:
`readable_record` holds them for every `Store` implementation — a NUL in `action`,
`deciding_policy` or `policy_pack`; a `recorded_at` or `policy_epoch` above `i64::MAX`; a
resource nested past what the reader follows. The refusal names the field and its own
code, `NC-JRNL-0005`, not retryable — an unstorable record is refused the same way forever
— so "the record cannot be stored" is never "the store did not answer"
(`NOCTUA.md §15.1`). `verify` likewise tells an unreadable row, rewritten where it lies,
from a history that does not begin where it should, so a responder knows whether to look
at one row or at the history above it.

### A refusal names the system that refused

`Error::Unavailable` means the store did not answer; `Error::Refused` (`NC-JRNL-0009`, not
retryable) means it answered no — a grant, a schema, a role, a statement it will refuse
identically next time. The journal fails closed either way, and the difference is the
advice: *try again in a moment* is wrong for a refusal. `nc-prune` makes the same split,
and any classifier added here decides which of the two it is, or is neither and says so.

### What the journal's protection list pins is the property

`writers=0` — no role other than the owner holds `UPDATE`, `DELETE` or `TRUNCATE` — not
who owns the table or whether a grant was ever made: `relacl` never returns to `NULL` once
set, so "no grants ever" would break the first time an application role is given `INSERT`,
and pinning "the connecting role owns this" would mandate the ownership that makes
`ALTER TABLE … USING` possible, which `NOCTUA.md §14` says a deployment must not have. The
row also carries `colwriters=`, `writeall=` and `superusers=` ("A measurement whose only
value is the passing one is untested"). The list holds catalog, not data:
`access_records`'s row order belongs to whoever wrote the rows.

### The pruner takes its bounds from the catalog, not from its caller

`prune_these` takes a `&[TableLimit]` whose fields are `pub`, so it requires **the whole
entry** to be the catalog's (`nc_catalog::TABLES`), refuses a name the catalog does not
declare — a second legal spelling such as `public.journal_entries`, or `_sqlx_migrations`
— and refuses `journal_entries` by name under any authority. `prune_these_declared_by` is
the visible seam for a caller with tables of its own (the integration tests'
`rows_over_time`).

**A statement waits five seconds for a lock, not for ever** (`lock_timeout`): a tick that
never completes holds a pool connection for the life of the lock. **A lock is a skip**,
reported as `Untouched::SomethingElseHeldALock` for the next tick, and the run goes on.
**Every other error is carried to the end of the run, and ends it only when the run did
nothing**: a bad `order_by` is a catalog defect and never swallowed, but the tables that
*were* pruned are still reported, since a scheduler cannot journal what it is not told;
`verdict` returns `Err` only when there is nothing to report at all. A run considers only
its own target's database.

### The pruner prunes one database per target, and leaves user state to its writer

`catalog/limits.toml` bounds tables in PostgreSQL and in the SQLite file on the user's
machine (`database = "local"`). A `Target` says which it prunes — `Target::database`,
required, so no target is PostgreSQL's by default — and `run` considers only that
database's tables; a table of the other database is refused before any statement, at `run`
and at the `DELETE` door alike.

`nc_prune::sqlite::Sqlite` is the second target, over the pool `crates/nc-local` opens,
and its guard refuses what the PostgreSQL one does — a view, a virtual table, a temporary
shadow, a `DELETE` trigger, a reference that rewrites another table's rows — with one
difference its header argues: a declared `ON DELETE CASCADE` runs, because a unit's images
go with the unit and their schema is the one the store checks at every open. So the guard
asks the same questions of every table a cascade reaches, all the way down
(`a_table_a_cascade_reaches_past_is_refused`); a row another table references
`ON DELETE RESTRICT` or `NO ACTION` is held, the oldest rows chosen being the oldest no
reference holds — which is how `store_unit_holds` keeps a unit whose close no kernel has
heard from every realm's prune
(`a_row_another_table_holds_is_left_and_the_next_oldest_go`); a connection with foreign
keys off is refused; and a trigger's event is read as SQLite reads it, the bare words
before its first `ON` with comments, literals and quoted names as gaps, so
`AFTER DELETE/**/ON` is a `DELETE` trigger.

**`refuse-at-limit` is user state, or a unit's images and notes, and the pruner leaves it
alone and says so.** `store_entries` and `blobs` are somebody's data. Their `max_rows` is
where the writer refuses a new row — for `store_entries` over each plugin's keys in each
realm (`per = "plugin"`), a number held for every plugin in every realm the machine's
kernels serve; a manifest may lower its own and never raise it, and one unit writes for
every plugin a nested dispatch reaches, so `nc_local::Quotas` carries a number per plugin
and the commit holds each to its own
(`one_unit_writing_for_two_plugins_holds_each_to_its_own_quota`). The run reports these as
`Untouched::ItsWriterRefusesAtTheLimit` — considered, neither a failure nor work done. **A
unit's images and notes are counted per unit**: `store_writes` and `store_notes` go with
their unit by `ON DELETE CASCADE` and an undo needs every one, so `per = "unit"` makes
`max_rows` what one unit may record, the commit refuses a unit past it before a statement
runs (`a_unit_past_what_one_unit_may_record_is_refused_and_leaves_nothing`) — that, times
`store_units`' bound, is theirs — and the kernel's overlay is held to the same number
(`units::writes_per_unit` in `crates/nc-kernel`).

### The pruner cannot prune the journal by a global constant

`NOCTUA.md §15.8` gives the journal `strategy = "policy-pack"` and no `max_rows`, and the
pruner **skips** every table that is not `oldest-first` rather than falling back to a
default: a global constant would one day delete below what a realm's pack promised. **That
strategy is an escape hatch**: it needs `pack_reason`, which the compiler refuses to go
without, and `check` names every table using it in the line that reports how many were
checked. `nc:bounded:` demands its reason the same way — an exemption nobody counts is how
a carve-out becomes permanent.

**A bound orders by a column whose ascending order is creation order, and that no caller
writes.** `access_records` orders by `recorded`
(`migrations/0006_access_records_orders_by_insertion.sql`), a plain `BIGINT` a
`BEFORE INSERT` trigger assigns from a sequence — not `GENERATED ALWAYS AS IDENTITY`,
which `OVERRIDING SYSTEM VALUE` and `COPY` both set, so the trigger is not redundant. Not
`id`: a Crockford Snowflake is twelve characters below `32^12` and thirteen above, from
about 2034, and text sorts byte by byte. Not `occurred_at`: a caller-written clock deletes
a slow device's rows first, and a future stamp makes the table keep only the bogus ones.
`journal_entries` separates *when* from *what orders it* the same way, with `sequence`.
The gate reads the column's type across every statement (a later `ALTER COLUMN … TYPE`
included), follows a `CREATE DOMAIN` to its base, and refuses a type it cannot read. **And
the column cannot be NULL**: `ORDER BY … ASC` sorts NULLs last, so a nullable `order_by`
makes its rows immortal while the pruner reports success; the gate reads the migration's
own column definitions.

**And the `DELETE` has been watched running**: `crates/nc-prune/tests/in_postgres.rs`
prunes a real table and asserts **which rows survive**, not how many — a pruner taking the
newest reports the same numbers. The identifiers written into the statement come from the
catalog and are checked against a plain-identifier shape first.

### A prunable table has a primary key, and the pruner deletes by it

`ctid` is where a row sits, and a concurrent `UPDATE` moves it, so a delete by `ctid`
skips a row another transaction touched — and since `access_records.recorded` survives an
`UPDATE`, the skipped row heads every later batch, with every run reporting success. A key
does not move: a table in `catalog/limits.toml` without a single-column primary key is
refused by name when the pruner runs. `SELECT … FOR UPDATE` would need the `UPDATE`
privilege, which the journal's append-only design withholds.

### Every name an install gives the system comes from one function, and a test's install never meets a real one's

**Guaranteed.** Every name `NOCTUA.md §17.1` lists — the profile's directories and files,
the runtime directory and the socket in it, the pipe, the units, labels, tasks and mutexes,
the CLI's and the daemon's names, the application's identities, the update feed and its
key, the default Leader, and a checkout's compose project — is derived by
`nc_channel::layout` from an install's manifest and the user it runs as, and by nothing
else; `cargo xtask dev` names its stack through `Checkout::stack`, which `layout` hands a
development install too. A checkout's stack ports are derived there as well, and are the
ones its stack binds unless `POSTGRES_PORT` or `LOGS_PORT` moves one ("The stack belongs
to this checkout"). Names are compared
case-folded, as a pipe and a case-insensitive file system compare them. For one user,
production, staging and two development checkouts, each as a real install and relocated
under each of two namespaces, all under the same bases, share no name but the default
Leader, which is held apart across the three channels and only there. Across two users
they share no pipe, mutex, unit, label or task; the names each user keeps in a home and a
`PATH` of their own, such as the CLI's and the application identities, are the same for
two users' installs of one channel, and so is a checkout's stack, which is the
checkout's. A relocated install — a test's — shares no name with a real one but its
channel's default Leader, whoever each runs as: its namespace is in every other name, so
even under a real install's bases its data, state, socket and device key are its own.
Every path it derives lies under its root, and it follows no update feed and names no
stack. Every name the whole machine shares but a checkout's stack carries the user,
written in letters, digits and `-` with every other character spelled out and joined by a
`.` no leaf holds, so no user can end a name or be read as another. A manifest is read
only as `nc_channel::Install::parse` reads it, failing closed: a key it does not read, a
key missing or of the wrong type, another format, an unknown channel, a checkout outside a
development install or malformed in one, a base that is not a plain absolute path — one
holding a control character or a line break among them — and a namespace outside its
alphabet are each refused with their own code in the `CHAN` area. A refusal quotes at
most 64 characters of the manifest's own text, with every character that ends a line or a
field written as its code point, so no manifest forges a line of the diagnostic that
refuses it — a key written twice included, which `nc-canon` refuses, escapes and cuts at
as many. `nc_channel` reads no environment: every location comes from the bases the
manifest carries. The checkout's digest and its rendering are the ones `cargo xtask dev`
named every stack with before, so no volume is stranded.

**Enforced by** `crates/nc-channel/tests/the_names_are_disjoint.rs` — which destructures
`Layout` without `..`, so a field it does not compare does not compile, compares every
relocated install with real ones under the same bases, so that its namespace alone keeps
them apart, writes every spelling out, asks a value of the wrong type at every key the
manifest reads, holds each refusal to its code and each place a refusal quotes the
manifest to the escaping and the cut, and holds every path a relocated install derives
under the bases it was handed, so a location taken from anywhere else fails it — the unit
tests beside the digest and the user's spelling, which compare users that would be
written alike if either `_` around a code point were dropped,
`a_key_written_twice_is_shown_escaped_and_cut` in `crates/nc-canon/src/parse.rs`,
`the_project_name_does_not_move_under_a_checkout` in `xtask/src/dev.rs`, and the operators
over `crates/nc-channel/src/lib.rs` and over `nc-canon`'s refusal of a key written twice
in `xtask/mutants.py`. `layout` takes an install and a user and nothing else.

**Not answered:** a process running as the same user, which can rewrite a manifest —
judging the file this user's alone is the reader's, and stops other accounts only; and
whether anything uses a name as derived, which is each consumer's — the daemon, the CLI,
the installer and the service manager's registration are what read a layout. What the
names leave narrowed is under **What is deliberately not claimed**.

### A daemon's endpoint lets in its user alone, and each end reads the other off the system

`nc_platform::ipc` (`crates/nc-platform/src/ipc.rs`, whose header has the rules) makes
the endpoint a daemon is reached through, a Unix domain socket on Linux and macOS and a
named pipe on Windows, and connects to one. Each system's half reports what the system
says of the other end of a connection; the comparisons are made once, in code every
system runs.

**Guaranteed:**

- **The runtime directory is this user's alone before anything is bound**, on every
  system: judged by `nc_platform::judge`, each way another user can reach it refused
  (`NC-IPC-0009`) with nothing made in it. The socket must lie in it, and a name longer
  than the system holds — `sun_path`'s 107 bytes on Linux and 103 on macOS, a pipe's 256
  characters — is refused naming the limit (`NC-IPC-0010`), at either end.
- **Unix: the socket is `0600`, and what stands at its name is judged, never removed
  unread.** A socket this user owns that refuses a connection is a dead daemon's and is
  replaced — only by a caller holding the profile's instance lock, which `bind` takes —
  and a socket something answers at is taken (`NC-IPC-0011`); a link, a file and a socket
  of another user are refused and left as they were (`NC-IPC-0009`). A listener removes
  the socket it made, and nothing put at its name since.
- **Windows: the pipe is made first, private, local and bounded.** The first instance
  carries the first-instance flag, so a name another process made first is refused
  (`NC-IPC-0011`) rather than joined; every instance is made with the descriptor a
  private file gets — the user its owner, a protected access list holding one entry, the
  user with every right — so the default list, which lets `Everyone` and `Anonymous`
  read, is never the pipe's; remote clients are rejected; and the pipe holds the
  connections allowed, one instance waiting for the next client and one for the
  connection past the bound, the next made before a refused one goes, so the name never
  stands without one of the listener's instances behind it. The instance waits where the
  listener keeps it, so an accept given up before a client came — the branch of a
  `select!` that lost — leaves it waiting for the next, as a socket's accept does.
- **Each end reads the other off the system, and refuses before anything crosses.** The
  listener reads the user and the process of each client — a socket's peer credentials,
  a pipe's client process and the user its token names, in one synchronous call — and a
  client reads the server's the same way, and on Windows the pipe's owner too, having
  opened the pipe at the identification level so a server may learn who it is and never
  act as it. A user that is not this process's (`NC-IPC-0012`), and a process the
  system does not name, or names zero, are refused and the connection closed; nothing
  listening is told apart (`NC-IPC-0014`). A connection past the bound is closed as it
  is accepted (`NC-IPC-0013`).

**Enforced by** the tests in `crates/nc-platform/src/lib.rs`, cfg-free and run on every
system — both ends reading each other off the system, the process read held to a child's
and never this process's own whichever end the child is, the peer predicate refusing
`nc_platform::testing::someone_else` and a process numbered zero or not at all, each way
another user can reach the runtime directory refused with nothing bound, the endpoint
read back as made (the socket's mode, the pipe's one entry), a name at the limit bound
and one past it refused naming the limit, an endpoint something answers at not taken, a
dead socket replaced and a file or a link at its name left as it was, a dead socket
another user owns refused and left as it was (judged with `testing::someone_else` as the
user a bind runs as), an accept given up before a client came leaving the listener
listening with a connection open and with none, the bound and a connection closed to
make room, each wait of theirs bounded so a lost other end fails rather than hangs — by
`crates/nc-ipc/tests/endpoint.rs`, which binds the
daemon's listener at the catalog's bound and runs the handshake across it, and by the
operators over `ipc.rs` and `unix.rs` in `xtask/mutants.py`.

**Not answered:** a process running as the same user, which can connect, bind first
where nothing answers, or replace a dead socket — authorization answers it (`§14`); root,
and on Windows the Administrators group, `SYSTEM` and a holder of the take-ownership
privilege; another account actually connecting, which no test can be (under **What is
deliberately not claimed**); on Windows, the process at the other end read by its id, so
one that exited may have handed its id to another process by the time its token is
asked — of another user, refused, or of this one; and a Windows listener whose next
instance could not be made, which makes it again as a first instance and so is refused
while its own connections still hold the name.

### Each end of a connection proves it holds the token, and neither sends it

`crates/nc-ipc` (whose module headers have the protocol, in order) is what a daemon serves
and a client asks through, over any stream: the endpoint's, or a test's.

**Guaranteed:**

- **A frame is one canonical value, bounded before it is read.** A four-byte big-endian
  length and that many bytes; a length of zero (`NC-IPC-0002`) or above
  `ipc_frame_max_bytes` (`NC-IPC-0001`) is refused from the prefix alone, before anything
  is allocated for it, and a body that is not exactly the text `nc-canon` writes for its
  value is refused (`NC-IPC-0002`), so there is one spelling of every message. What a
  reader would refuse is never sent: a request or an answer too long for a frame is
  refused to the side that would send it before anything of it is written
  (`NC-IPC-0016`), and the connection goes on, a request still waiting for its answer. A
  connection ending inside a frame was lost (`NC-IPC-0003`); one ending at its boundary
  has closed.
- **The handshake proves both ends, and the token never crosses the wire.** The daemon's
  challenge carries its nonce; the client's proof is BLAKE3 keyed by the token over the
  client's domain and both nonces; the daemon's is keyed the same way over a domain of its
  own. Each is compared in constant time (`blake3::Hash`'s equality), so a replayed hello —
  its proof over another challenge — a replayed challenge and welcome — the welcome's
  proof over another client's nonce — and a client's proof reflected back as the daemon's
  are each refused: a client that cannot prove it is turned away before a request is read
  (`NC-IPC-0005`), and a client sends nothing past its hello to a daemon that cannot
  (`NC-IPC-0006`). Another protocol is refused as that (`NC-IPC-0004`), a message holding
  a field the protocol does not read as malformed, and either end gives up at
  `ipc_handshake_ms` (`NC-IPC-0007`). Before it closes for a reason it can name, a side
  says goodbye with the code, so the other end is told why.
- **One request at a time, and a heartbeat both ways.** A request before the last one was
  answered, and an answer nobody asked for, close the connection (`NC-IPC-0002`); a request
  abandoned before its answer leaves the connection refusing to be asked again, rather
  than handing the late answer to the next request. Each side pings after
  `ipc_heartbeat_ms` of its own silence — while a dispatch runs, too — and drops a peer
  silent for `ipc_silent_after_ms` (`NC-IPC-0008`); and a write the peer does not take in
  that time ends the connection however the peer pings (`NC-IPC-0017`), the writer telling
  the reader, so both halves of the stream are let go. Every bound is the catalog's, read
  by the crate itself.

**Enforced by** `crates/nc-ipc/tests/frames.rs` (round trips and every cut of a frame, a
length past the bound refused with the writer still open, the bound itself taken, each
spelling that is not canonical, a value past the bound refused by its writer with nothing
written, a request and an answer too long for a frame refused to their sender with the
connection going on, a dispatch and its answer at the kernel's whole payload),
`handshake.rs` (a wrong, a missing and a replayed proof, a replayed challenge and
welcome, another token, another protocol, an unread field, a forged and a reflected
daemon proof, a welcome naming no process, both ends' deadline on a paused clock, and
every byte either end wrote held to hold neither the token nor its text), `heartbeat.rs`
(a relay between a real client and a real daemon swallowing either side, passing only its
pings, repeating its next message or reading nothing more of it: a silent peer dropped at
the bound, from either end, after the pings, a peer that pings and takes nothing dropped
at the bound with the daemon's stream let go, a dispatch outlasting the bound waited for,
pipelining and an unasked answer refused), and the operators over the crate's sources in
`xtask/mutants.py`.

**Not answered:** a process running as the same user, which can read the token from the
runtime directory and speak for anybody — authorization re-decides every dispatch (`§14`);
the confidentiality of a connection from such a process, which is the endpoint's: nothing
is encrypted; and a peer slow to speak inside its bounds, which holds one of
`ipc_connections_max` for as long as it pings. Who writes the token and the process it
is read by are the daemon's and the CLI's, under **What is deliberately not claimed**.

### A daemon starts only on a profile that is its user's alone, and mints nothing it cannot resume above

`apps/daemon` (`install`, `profile`, `device` and `mark`, whose headers have the rules)
reads what a start stands on, in order, and refuses with a code of the `CHAN` or `DMN`
area at the first thing it cannot vouch for.

**Guaranteed:**

- **An install names itself, and nothing else does.** The manifest is read beside the
  canonicalized executable, in a directory judged this user's alone by
  `nc_platform::user_in` — the one way to the user a start runs as — each time the install
  is read; it is judged so itself with `nc_platform::judge`, and read only as
  `nc_channel::Install::parse` reads it. A binary with none refuses (`NC-CHAN-0006`) rather
  than fall back to a channel built in, and a manifest another account can reach, or one in
  a directory another account can reach, is not read (`NC-CHAN-0007`).
- **The profile is the user's alone from its first moment.** Its data, configuration,
  cache and state directories, and the log and runtime ones under the state one, are each
  made private one level at a time — never a parent on the way — or judged so, and
  anything else refuses the start with it left as it was (`NC-DMN-0001`); so is every file
  of it a start reads — the seed, the device's record, the world and the mark — judged
  before it is read, and the data and configuration directories `--initialize` writes a
  seed and a world into, judged before anything is written there, with the same code. The
  plugins directory under the data one is not among them: no start makes, judges or reads
  it yet (under **What is deliberately not claimed**).
- **A device is drawn once and never again.** Its seed is 32 bytes from the system's own
  generator, written into a file created private that refuses any name already there, so
  it is never overwritten and its flake, `MachineFlake::candidate(public, 0)`, never
  regenerated; the daemon reads the seed and its record and never makes either
  (`NC-DMN-0003`), refuses a seed of another length and a record that is not the seed's
  (`NC-DMN-0004`), and holds the seed no longer than it takes to derive the public key.
  The journal's keeper is a digest of that key (`nc_ids::OpaqueId::of_device_key`).
- **The world is a file the installer copies in, read with the pack its realm selects**:
  the realm and the person a daemon decides as, each minted by `nc_authz::LocalWorld::parse`
  only for an entity the world holds, the person inside that realm (`NC-DMN-0005`,
  `NC-DMN-0006`). The copy is written beside it and renamed over it; one an interrupted
  copy left there is removed only once it is judged this user's alone (`NC-DMN-0001`).
- **No identifier is handed out past a ceiling that is not yet durable.** The minter asks
  the mark to raise its ceiling `mark_lease_ms` past the clock before it hands out a
  millisecond past the one it holds, and refuses before any identifier exists when the
  write fails (`NC-IDS-0009`). The mark is two checksummed slots overwritten in place and
  synced, so a torn write spoils one and the other holds the ceiling nothing was minted
  past; with neither whole the start refuses (`NC-DMN-0007`). The lease is the catalog's,
  which the mark reads itself, so no caller chooses how far a raise reaches. A start
  resumes above the larger of the mark and the newest unit it hands in, waits for a clock
  behind it — logging `ids.clock_backwards` past the lease — and refuses a mark more than
  a day ahead; settling the ceiling (`nc_ids::Minter::settle_ceiling`) writes the exact
  high-water mark.

**Enforced by** the unit tests of each module in `apps/daemon/src/` — a missing, reachable
and malformed manifest, a manifest in a directory another account can reach refused and
left as it was, a manifest read through a link beside the file that runs, the
profile's data, configuration, cache, state, log and runtime directories each made private
and judged again, a missing base not made on the way, a world copied, replaced and
refused, in the profile as well as on its way in, a copy left beside the world removed and
one another account can reach refused, a configuration and a data directory another
account can reach refused before a world or a seed is written into them, a seed drawn once
and never again, a record lost and written from its seed, each way a record is not the
seed's, a seed, a record, a world and a mark another account can reach refused and left as
they were, a raise read back from the other slot, a mark nothing raised read again as a
first start, a torn and a corrupt slot, the minter a first and a resumed start hands back
raising the mark before it mints past it
(`a_minter_from_a_start_raises_the_mark_before_it_hands_out_past_it`), a clock behind the
mark by less than the lease, by exactly it and by a millisecond more, by more than the
lease and by more than a day, the catalog's lease inside the drift a minter waits out, and
the binary's invocations and exit codes — by
`apps/daemon/tests/the_mark_survives_a_crash.rs`, where a child mints behind the real mark
until it is killed, with its output still read, and every identifier it printed is held
against a start from what it left, with the clock back by more than the lease and by more
than a day, which repeats nothing and refuses respectively, and the mark the restarted
start leaves is above everything it minted, by the ceiling's tests in
`crates/nc-ids/src/snowflake.rs`, and by the operators over those sources in
`xtask/mutants.py`, each judged by its own module's unit tests.

**Not answered:** a process running as the same user, which can rewrite the manifest, the
world or the mark, or delete the mark so that a start resumes from the local journal alone;
a slot spoiled after it answered, which reads as a torn one and falls back to the older
ceiling; a crash inside the mark's very first write, which leaves a file with neither
slot whole, so the profile does not start until it is removed by hand; a crash inside the
seed's first write, which leaves a seed of another length that no start and no
`--initialize` reads (`NC-DMN-0004`), so the profile does not start until it is removed by
hand; a disk that answers a sync before the bytes are on it; and an administrator. What
the daemon leaves undone is under **What is deliberately not claimed**.

### The stack binds to loopback and nothing else

`docker-compose.yml` publishes both of its ports on `127.0.0.1` and nowhere else, never on
`0.0.0.0` or as a bare `5432:5432`, which looks narrower and is the widest. The log store has no
authentication and is not meant to: `NOCTUA.md §18.5` chose a store on the same machine
because **the cheapest way to guarantee no user data leaves it is for there to be nowhere
else for it to go**. `every_published_port_is_bound_to_loopback` reads and counts every
published port, so a service that gains one nobody looked at fails.

**The healthcheck asks the question the caller asks**: `pg_isready` never authenticates or
opens the database, so PostgreSQL's healthcheck runs a query over TCP, as
`cargo xtask dev` is about to.

**The stack belongs to this checkout.** Compose names a project after its directory, so
two checkouts of this repository would share one project and `cargo xtask dev --down` from
either would remove the other's containers *and volumes*; a constant name makes that
unconditional. `dev` derives the name from the canonicalized checkout path, through
`nc_channel::Checkout::stack` ("Every name an install gives the system comes from one
function, and a test's install never meets a real one's"), and prints it
(`docker compose ls` says which stack is whose).

**And so do its ports, so two checkouts' stacks run at once.** `Checkout::stack` derives a
pair from the same digest, in 20000–32765, below every system's ephemeral range;
`dev::port` in `xtask/src/dev.rs` is the one reader, which `dev`, the gate's
`DATABASE_URL`, its image gate and its reader of the log store's bounds all ask. Both places
`cargo xtask` runs `docker compose` — `dev::compose`, which every verb of `dev` goes
through, and the image gate's `the_effective_stack` in `xtask/src/gates.rs` — hand the
pair from that reader to every invocation through the child's environment, and the stack
suite's `projection_of` hands each checkout the pair `nc_channel` derives for it.
The compose file writes no port of its own — each is `${NAME:?…}` — so a
`docker compose up` not handed the pair refuses, saying what to export, rather than
binding a port another checkout's stack may hold. `POSTGRES_PORT` and `LOGS_PORT` move a port, a variable set
empty says nothing, as compose reads it, and `dev` prints the pair with the `DATABASE_URL`
it makes. **Fail closed**: before compose runs, `dev` refuses a port variable that holds no
port, and a port it cannot bind that this project does not publish there, naming the port
and the variable that moves it. It names a holder — two checkouts whose digests fall on one
pair, or anything else listening there — only when the bind says the port is in use; a bind
refused for another reason, a port below 1024 to a user without the privilege among them,
is refused with the bind's own error. Its own stack is told apart by asking
`docker compose port` of the service that publishes the port, at the container port it
publishes from.

**Enforced by** `two_checkouts_resolve_to_stacks_that_share_no_port_and_no_project` in
`xtask/tests/the_stack_enforces_the_declared_bounds.rs`, which resolves two checkouts'
compose configurations with the pairs `nc_channel` derives and holds their projects and
published ports apart; `the_settings_this_tool_reads_are_the_ones_the_stack_declares` and
`the_example_environment_declares_what_this_tool_reads` in `xtask/src/dev.rs`, which hold
the compose file and `.env.example` to the reader both ways — every port required and none
defaulted, and both written empty in the example, which writes no url;
`a_stack_binds_its_checkouts_ports_unless_a_variable_moves_one`,
`a_port_something_else_holds_fails_naming_the_variable_that_moves_it` and
`a_bind_refused_for_another_reason_is_refused_with_that_reason` beside them, and
`each_port_is_asked_of_the_service_that_publishes_it`, which holds the service and the
container port `dev` asks compose about to the compose file; the pinned runner
(`the_block_that_decides_isolation_is_the_one_written_down`);
`every_compose_this_tree_starts_is_handed_the_checkouts_ports` in `xtask/src/gates.rs`,
which reads every Rust file of every member, holds each `docker compose` it starts to that
hand-off with no other call on the child's environment, and names the three places — read
as text, because a run in a shell that exports what `dev` prints cannot see a hand-off
missing, and only one with neither variable, as in the `linux` job, refuses it;
`the_default_url_is_on_this_checkouts_postgres_port` in `xtask/src/database.rs` and
`the_log_store_is_asked_for_its_bounds_on_this_checkouts_port` in `xtask/src/check.rs`,
which hold the gate's two other readers to `dev::port`, each asked for its own port and
asked nothing else, as text and for the same reason;
`every_setting_is_read_through_the_table_that_declares_it` in `xtask/src/dev.rs`, which
refuses a setting or a port read by its name — through `env::var`, `var_os`, `env!`, or a
row of its own handed to `setting` — in every Rust file of every member; and the `linux`
job, which sets no `DATABASE_URL`, so the gate's url is derived from the runner's checkout
as `dev`'s stack was.

**Not answered:** a port taken between `dev`'s look and compose's bind, which compose then
refuses in its own words, naming both variables; a port below 1024 that a rootful Docker
could publish, which `dev` refuses because this user cannot bind it to ask; two accounts in
one checkout, which share its stack (**What is deliberately not claimed**); a
`docker compose` typed by hand, which is handed whatever ports its shell exports; and one
the census cannot read as a compose — a program not written as the literal `"docker"`, or a
verb assembled from pieces — which `dev.rs` refuses in that file, where every process it
starts is counted and its runner is pinned whole, and nothing refuses in another; and a
port read by a name built at run time, as `dev::port` itself reads it, which the census of
names cannot tell from that reader.

### Secrets are environment variables, and `.env` is not a file here

`.env.example` is committed and says **not** to copy it: it lists what the stack reads,
with the values a developer would otherwise be told, and the ports empty, because they
are the checkout's. `docker-compose.yml` defaults every credential it uses and is handed
its ports by `cargo xtask dev`, so the stack comes up with nothing copied. `DATABASE_URL`
is read by nothing in the stack and by everything beside it: `dev` derives it from the
stack it started and prints it, `check` takes it from the environment and derives the same
when it is unset, and hands it to the PostgreSQL tests. A committed environment file is
invariant 2's; `.env.example`'s names and values are invariant 7's.

**The compose file's vocabulary is closed**: every key is written down with the reason it
is allowed, or it is a finding — and so is what a service is handed inside `environment:`
and `volumes:`, because `POSTGRES_HOST_AUTH_METHOD: trust`, `privileged: true`,
`user: "0:0"`, `cap_add: [SYS_ADMIN]` and a bind mount of the host's configuration each
pass any list of forbidden needles.

### The log speaks to this machine, and the endpoint is the last `&str`

`Log::open(origin, endpoint)` takes the endpoint as text (`Origin`'s fields are `Literal`,
being on every line), and `Target::parse` refuses two things a caller could build with
`format!` — refusals, not silent repairs, so the endpoint written and the endpoint posted
to are one:

- **A query string**: VictoriaLogs reads `extra_fields` on `/insert/jsonline`, which would
  write caller text onto every record past the type wall, and `ignore_fields` and
  `_msg_field` would rewrite or discard every record.
- **A host that is not this machine**, decided by parsing the address rather than matching
  text — `starts_with("127.")` accepts `127.0.0.1.example.invalid`, a name somebody else
  owns.

### Deny-by-default is tested from the denial side

Cedar denies by default, which makes `decision = "deny"` **true of an empty pack**
(`NOCTUA.md §19.5`). So a denial case in `policy/**/tests/` names *why*, and `nc-policy`
proves it by mutation: the request re-run with the named `forbid` removed must become
`allow`, and a case still denied is reported as proving nothing. The negative-test
invariant (`NOCTUA.md §14.9.7`) is a **gate**, because a Cedar rule cannot require that
somebody wrote a test for it.

**Every constraint of a permit is pinned by a case**: one token —
`action in [Action::"Grant", Action::"Revoke"]` widened to a bare `action` — leaves the
count of policies with denial tests unchanged while granting a guardian everything, so
`nc-policy` drops each constraint in turn, requires some case to change its answer, and
reports how many it offered for widening. **A gate's own fixture has the shape it will be
handed** — every policy here opens with `@id(…)` (`NOCTUA.md §19.3`).

**The boundary flags.** `the_action_is_the_boundary = true` says the verb puts the request
outside the permit — the pairing rule otherwise asks for an `allow` on the same action,
which an action list makes impossible — and the case must mean it: with a verb the permit
allows, the same request is permitted. `the_principal_is_the_boundary = true` is the same
for a type test on who is asking (`principal is User` refuses a device for being one): it
pairs with an `allow` for another kind of principal, same action, same kind of resource,
and asked again as the principal of **some such `allow`, not each**, it must be permitted
— a control on another realm is refused by membership, which says nothing about who asks.
Without it, a type test like the history permit's is pinned by no case, and deleting it
lets a device or an agent undo a person's history. **Either flag on any case but a
`no_permit_matched` denial is refused** (`boundary-flag-without-a-boundary`), since the
pairing reads those cases alone.

### Error codes are append-only, and a ledger in the tree is what holds them

`NOCTUA.md §15.1` makes `NC-<AREA>-<NNNN>` append-only, and a build reads the tree and not
its history, so the compiler keeps `catalog/errors.reserved.toml`, checksummed, and refuses a
code renumbered, reused or removed. `cargo xtask check` always passes `Bootstrap::Refuse`;
only `cargo xtask build --reserve` may accept a ledger nothing vouches for, and clearing
the checksum by hand forgives an unvouched ledger and nothing more. `cargo xtask build`
announces each code it reserves, on the line it reserves it.

**Undoing a reservation is possible, deliberate, and three steps** — delete the code's
line from the ledger, blank its `checksum`, run `cargo xtask build --reserve` — for a code
an experimental build reserved and no released build carried. Never for a code out in the
world: every log line written with it still refers to it.

**The ledger is TOML**: `ledger.rs`'s `as_toml` writes TOML strings and `crate::quoted`
Rust ones, and they differ exactly at a control character (`\u0007` against `\u{7}`) — a
new output format gets a new escaper.

### A generated type cannot be built from outside the crate

`Command`, `EventSpec`, `FieldSpec`, `RateLimit`, `Budget`, `StoreLimit` and `Argon2Id`
are `#[non_exhaustive]`: a dispatcher and a pruner decide on their fields, and a caller
could forge one that reads as harmless. `TableLimit` is the exception, for the reason in
`emit.rs` — `nc-prune`'s fixtures build one, and they keep its runtime guard honest.
`Entry`, `ParseError` and `FlakeOutOfRange` are sealed likewise: `Entry::chain` computes a
link, and a hash a caller supplies is not one.

### If every verb answers `--locked`, the lockfile is behind the manifest

`.cargo/config.toml` puts `--locked` in the `xtask` alias, so `cargo run` never quietly
rewrites a stale `Cargo.lock` before the step that checks it. The cost: a tree whose
manifest and lockfile disagree cannot run `xtask` at all — every verb answers
`error: cannot update the lock file … because --locked was passed to prevent this`, naming
neither the version nor the release. The one way there is an interrupted
`cargo xtask release`, between the manifest rewrite and `cargo update`; `release` prints
what to do before opening that window, and the recovery is `cargo update --workspace`. No
gate can hold this: in that state nothing here runs.

The gate's own `cargo fetch --locked` is held the same way, and that is its security
answer: it downloads only what the lockfile names, cargo checks each download against the
checksum recorded there, a lockfile behind its manifests is refused rather than rewritten,
and nothing a fetch brings down runs until a build compiles it — so the fetch adds no
package and no code the lockfile and the audit have not already been asked about. Under
`--offline` it does not run at all, and a run in which it did is refused by the verdict.

### A release reads back what it wrote

`cargo xtask release --channel staging|main` runs the whole gate before anything is
written — in CI's release job, the job before it ran that gate on the same commit — then
writes the version in every file `CARRIES_THE_VERSION` names and CHANGELOG.md, relocks,
and reads every one back.

- **It writes only regular files of this tree's.** Before the first write, each manifest
  `CARRIES_THE_VERSION` names, `Cargo.lock` and CHANGELOG.md is judged by what it is, not
  by what it points to (`symlink_metadata`): a link, to a file or to nothing, a
  directory, or anything else but a regular file stops the release with nothing written.
  A write follows a link to wherever it points and creates the file a link to nothing
  names, the changelog's put-back removes a link the tree held in its place, and the
  release commit would carry the bytes read through a link as a file of its own. A
  changelog that is not there is allowed: the release creates it.

- **It reads the version from its own table, and reads it back through another reader.**
  The version is `[workspace.package]`'s or `[package]`'s as TOML writes it — never the
  first line that happens to begin `version = "`, which a comment or a dependency above
  the table can be — and the write is read back with `toml`, which shares nothing with
  the writer's search: the table of every manifest `CARRIES_THE_VERSION` names, every
  package of this workspace in the lockfile, and the changelog byte for byte. The release
  commit carries each of those files, so a file added to that list is published with the
  rest.
- **A version is written the way cargo writes it**: no leading zero, or the manifest
  becomes one nothing in the workspace parses, `xtask` included, and the tool cannot be
  run to undo its own write.
- **It only goes forwards**: a release is refused unless it is ahead of every release tag
  `HEAD` reaches and its numbers are not behind the manifest's, because a pushed tag
  cannot be taken back.
- **A version it cannot order is refused**: it orders `X.Y.Z` and `X.Y.Z-rc.N`, a
  candidate below the release it is a candidate for, and nothing else — ordering more is
  semver's, a dependency this repository has not taken.
- **A failure puts back what it wrote, and names what did not go back.** Every file the
  release writes is read before the first write: each manifest `CARRIES_THE_VERSION`
  names, the lockfile, and the changelog or its absence. A changelog that is there and
  cannot be read is refused then, with nothing written, because one taken as absent is
  one a failure would remove. After the first write, a listed file that cannot be read,
  written or read back puts every listed file back; a changelog that cannot be written, a
  relock or a read-back that refused, and a file the release commit carries that cannot be
  read put back each manifest, the lockfile and the changelog — the bytes each held, a
  changelog the tree had none of removed, and a file still holding what it held left
  alone. A file that cannot be put back is named in the refusal with why, and is not tried
  again. **So does a refusal at the release commit or after it**, under `--publish`: the
  release commit, the tag, the GitHub release or the pull request back refused puts every
  file the run wrote back as `HEAD` holds it, and the refusal says so, whatever GitHub was
  left holding — a release commit made is carried on by the run its push starts.

A document illustrating a version's *spelling* never uses the version this tree is at: the
release walks every file for it, and the illustration would read as a file left behind.

### A release is computed from the history, and made on GitHub once

The version and the changelog are functions of git (`xtask/src/release.rs`,
`NOCTUA.md §22.3`, §22.4): nothing a caller passes chooses them, and the channel is the
branch the release is made from.

- **The version.** The last production release is the highest `vX.Y.Z` tag `HEAD`
  reaches; with none, the first release is the version the manifest carries, its numbers
  unbumped. After one, the commits since it decide: a breaking change (`!`, or a
  `BREAKING CHANGE:` footer) is a major, which is a minor while the major is 0; a `feat`
  a minor; a `fix` or a `perf` a patch; anything else is **nothing to release**, said by
  name, with nothing written. `staging` releases `X.Y.Z-rc.N`, `N` one past the highest
  candidate of those numbers that exists; `main` releases `X.Y.Z`, refused when its tag
  exists. **`staging` refuses while the latest production tag is not one `HEAD`
  reaches**: the back-merge from `main` has not landed, and the version computed would
  repeat what `main` shipped.
- **The changelog** is regenerated whole every time: a section per production release,
  newest first, and above the newest a section per release candidate of numbers not yet
  released, which folds into its release's section once that ships. Each commit sits in
  the section of the first release that reaches it, as its subject, its scope and its
  short hash, in the part its type says; a release's notes are its section without the
  heading. No date and no machine reach it, so the same history gives the same bytes.
- **The assets** are judged against GitHub's limits on a release before anything is
  published: `cargo xtask release --channel staging --dry-run` lists them against those
  limits, which GitHub publishes at
  https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases —
  the page `GITHUBS_LIMITS` in `xtask/src/release.rs` names, and where they are read
  again.

**Published by CI's release job and nowhere else**, which runs no gate of its own: the job
before it ran the gate on the same commit, and the release job passes
`--after-the-gate-job`, which the verb refuses without `--publish` and anywhere but that
job of `release.yml` on the channel's branch ("The release job is what `release.yml` says,
and the App's token reaches its last step alone"). `--publish` asks GitHub first what
exists and judges the answer before anything is written, then makes the release commit
through `createCommitOnBranch` with the commit the job checked out as the branch's
expected head — so a branch that moved meanwhile fails the release — then the tag, the
GitHub release (a pre-release on `staging`, the latest on `main`), and the pull request
that carries it back (`main` to `staging`, `staging` to `dev`) with auto-merge by merge
commit. A run on a release commit — the push that commit causes, or a job run again after
one died — carries that release on, makes only what is missing, and ends `already
released` when nothing is; there, a pull request back whose checks have passed already is
merged by merge commit, because GitHub refuses auto-merge on one it would merge now.

**A release that died part way is finished by the run of its release commit's push.**
`release.yml`'s concurrency keeps every run that waits for a branch, first in first out
(`queue: max`), so a merge into the branch while a release runs waits behind the run its
release commit started, and replaces nothing. If that run was cancelled all the same — by
hand, or past the hundred runs GitHub keeps waiting in a group — re-running it is the
repair: it is a run on the release commit, and makes only what is missing. Re-running the
run that died computes the release again from a head the branch has moved past, and
`createCommitOnBranch` refuses it.

**A pull request back that cannot merge on GitHub is carried back by hand.** Its head is
the released branch, which takes nothing but pull requests and the release App, so a
conflict with the branch below — a member's version line in the lockfile, or a version
and a changelog the branch below changed since — cannot be resolved on it, and GitHub's
conflict editor cannot commit there. When GitHub says it conflicts (`mergeable: false`
where it answers through REST, the merge state `DIRTY` through GraphQL), or refuses to
turn auto-merge on for it, the release stops after the tag and the GitHub release and
says how: make a branch from the branch below, merge the released branch into it, keep
the released branch's version and CHANGELOG.md wherever the two disagree, run
`cargo update --workspace`, commit it signed under a conventional subject such as
`chore(release): carry v0.1.0 back to staging by hand`, and open a pull request into the
branch below titled the same. Git's own subject for the merge (`Merge branch …`) is
refused: the history step judges every commit the pull request's head reaches, merges
included, and its title, and that subject opens with no type. Once that merges, GitHub
marks the App's pull request merged, because its head is reachable from its base, and a
run on the release commit ends `already released`.

**A release the branch below holds already is not carried back again.** A release made
while an earlier one's pull request back is still open is carried by that one, and once it
merges, the branch below holds the release commit with no pull request of this release's
title anywhere. So a run carried on with no pull request carrying the release asks GitHub,
before it makes anything, whether the branch below holds the release commit — GitHub
compares the two, and the commit is that branch's head or behind it — and opens none when
it does: GitHub refuses a pull request between a branch and one already holding all of it,
and that run, and every run after it, would fail over a release carried back whole. A
branch below that took a merge the released branch has not is one GitHub answers as
diverged from the release commit, not holding it, and the release is carried back there.

**A run that makes the release commit never merges now.** That commit moves the head of
the pull request back, and the merge state GitHub gave was of the head before: no check
has run on the commit just made, and into `staging`, whose rules the App alone may pass, a
merge asked then would land the release before any check of it ran. So that run queues
auto-merge, whatever the state it read; and a pull request merged under the release's title
before the commit existed carried none of it, so the release opens its own.

**Only this repository's own pull request carries a release back.** GitHub finds a pull
request by the name of its head branch, and a fork's `staging` has the same name as this
one's: a stranger's pull request from it, taken as the back-merge, would have auto-merge
turned on with the App's token for code nobody reviewed, and one closed under the
release's title would stop the release. So the verb asks GitHub where each pull request's
head is — in a fork or not, and in which repository — passes over every fork's, open or
closed, and asks auto-merge and the merge of this repository's alone.

**Fail closed.** A tree with no `.git`, a shallow or grafted clone, a release tag this
cannot read, a commit whose subject the history step refuses, a tree holding changes no
commit has, a tag or a release on GitHub for a version `HEAD` is not the release commit
of, a tag naming another commit, a pull request that carried the release and was closed
without merging, a release commit whose manifest carries another version or whose
changelog is not the one the history gives, a pull request GitHub describes without saying
whether its head is in a fork, one of this repository's own whose head is elsewhere, one
with no merge state, and an answer from `gh` that is not the shape asked for — a step
answered with success and a tag at another commit, another release, another pull request,
auto-merge by another method or none, or a merge GitHub does not say happened, and a
comparison with the branch below that is not ahead, behind, diverged or identical — are
each a refusal, by name, before the step that would act on it. A pull request's node is
read from `node_id` where GitHub answers through REST, whose `id` there is a number, and
from `id` where it answers through GraphQL. What GitHub holds is judged right
after it is asked, on a run that makes the release commit as on one carried on from it, so
a refusal of it never leaves a release commit with no tag, which every run after it on that
branch would carry on into the same refusal. A fork's pull request whose head repository
is gone is passed over, not refused, or deleting a fork would stop every release.

**The token reaches `gh` through its environment and nowhere else.** `GH_TOKEN`, the
release App's installation token, is never an argument and never a line the verb prints.
Every other variable of `gh`'s own is removed — `GH_HOST`, `GH_REPO`, `GH_DEBUG` — and so
is `GITHUB_TOKEN`, so `gh` cannot fall back to the workflow's own token, whose pull
requests start no checks; the host is set to GitHub's. **No other child is handed it.**
CI's release step puts the token in the environment of `cargo xtask release`, where every
child it starts would find it, and that App may push to `staging` and `main` past their
rules and move a `v*` tag; the cargo that relocks and the git the history is read through
are started without `gh`'s variables. **And no gate runs beside it.** A child cleared of
the token does not keep it from a descendant that reads its ancestors' starting
environment, which on Linux a process of the same user may; so CI's release job runs no
gate at all. `release.yml` runs the gate in a job of its own, which enters no environment
and holds no secret and no token, and the release job waits for it and passes
`--after-the-gate-job`. A gate this verb does run — a release made without that flag — is
a child of its own, cleared and started with every variable but `gh`'s, and that is all it
is. Nothing takes the token out of the release's own environment, which in this edition is
`unsafe`, and this workspace forbids it. **What the token merges is this repository's own
pull request, never another**: auto-merge and the merge are asked only of a pull request
whose head is a branch here, never of a fork's that shares the branch's name. What `gh`
answers is read and never printed, and a refusal quoting it has every character a reader
cannot see written as its escape, a newline among them, so no line of GitHub's can open a
line of CI's log. The release starts git only through the history step's reader
(`crate::history::Own`), which removes every `GIT_*` variable and every one of `gh`'s from
what it inherits, and sets `GIT_DIR` and `GIT_NO_LAZY_FETCH` itself. A test drives the
whole sequence through a fake `gh` first on `PATH`, compiled for the system it runs on,
which records each call — its arguments, the variables of `gh`'s own it was handed, and the
request — and answers from fixtures, a run carried on
after each step among them, a fork's pull request beside the release's own, one whose
checks have passed in each state GitHub merges at once, an earlier release's pull request
merged before the release commit, with the branch below stopped there or moved on since,
and merged since it, a release commit made with such a pull request open, a pull request
answered in REST's shape, its `id` a number, each step answered with success in another
shape, and a pull request back GitHub says conflicts, queued or not, or will not queue,
each refused with the carry-back by hand and nothing asked after it. A second publishes on
`main` a release whose candidate was tagged and merged, and holds the release commit to
`main`, the GitHub release to the latest and not a pre-release, and the pull request back,
and the branch a run carried on asks about, to `staging`. Another runs its own test binary
again with `gh`'s variables exported, and holds the gate and the relock, each started at
the fake, to being handed none of them. And the gate's refusal is held through the verb
itself: one test runs `run_at`, which `run` hands this tool and this process's
environment, over a scratch workspace with the fake as its gate, exiting 1 and then 0 —
the run whose gate refused hands that refusal back and leaves every file of the workspace
as it was given, and the one whose gate passed writes the version, the changelog and the
lockfile. A refusal of the release itself is held through `run`, so what the release
refuses is what the verb hands back. Another makes a release fresh and is refused at the
release commit, at the tag, at a pull request back in conflict and at auto-merge, each
leaving the workspace as it was given. And the gate standing aside is driven through
`run_at` with an environment of its own, never this process's: refused without `--publish`
and outside the release job, with nothing written and nothing started, and accepted in it,
with no gate started.

### The release job is what `release.yml` says, and the App's token reaches its last step alone

`.github/workflows/release.yml` makes a channel's release on a push to its branch, and
`every_workflow_is_bounded` in `xtask/src/workflow.rs` holds the file to this shape: the
gate's `CI` step refuses the tree otherwise, before any run of the release can.

- **It fires on a push to `staging` or `main` and on nothing else** — no pull request, no
  tag, no path filter, no other event. Those branches are the channels `--channel`
  accepts, and a test holds the two lists to one set. Neither workflow runs on
  `pull_request_target` or `workflow_run`, each of which runs with this repository's
  secrets on behalf of a change made in a fork: the name is refused wherever it stands
  outside a comment, however it is written — a key or an entry of a flow sequence, quoted,
  behind a `\x` or `\u` escape, across a backslash's line break, or in capitals.
- **Two jobs, and the second alone enters `release-${{ github.ref_name }}`**, the
  environment of the branch pushed to, which GitHub lets a job enter from that branch
  alone and which holds the App's client id and its key: no other branch and no pull
  request can mint the App's token. `gate` runs the whole gate on the pushed commit; it
  enters no environment, reads no secret, no variable and no step's output, mints no
  token, and neither publishes nor stands the release's gate aside, each refused by name
  wherever it stands from the job's line to the next job's, comments among it. `release`
  waits for it, `needs: gate` and no other value, which no other job holds, and runs no
  gate of its own. Both run on Linux: the gate is the whole gate only where the stack
  runs, and the release builds its tool on the system the gate judged.
- **No other workflow can mint it either.** An environment admits a job by its branch and
  not by its workflow, and `ci.yml` runs on a push to `staging` and `main` too. So every
  file GitHub runs but `release.yml` — `ci.yml`, and the file of a composite action a
  step reaches — is refused if it names the token's action, enters an environment (a key
  `environment`, quoted, in a flow mapping or after `?`), reads a secret (the word
  `secrets` anywhere in its code, a condition and `secrets: inherit` among it), or reads
  a step's output (`steps` in any expression). Every file CI reaches but `release.yml`,
  the justfile and a script a step runs among them, is refused if it holds `--publish` or
  `--after-the-gate-job`, which stands the release's own gate aside. A
  justfile or a script holds no expression GitHub evaluates and no key a runner reads, so
  the first four are not read there, and `docker-compose.yml`, which CI reaches, holds an
  `environment:` of the stack's own. Each line is read on its own, as written and with a
  double-quoted scalar's escapes resolved, and once more joined to the lines a backslash at
  its end continues: a backslash continues a double-quoted scalar and a shell's line, and
  at the end of a comment or a plain scalar it continues nothing, so the line after it is
  read as a line of its own as well. An expression is read in a comment too, because
  inside a block scalar a `#` line is text GitHub evaluates. The token's action is not on
  the actions every file shares (`SETS_UP_AND_JUDGES_NOTHING`): the release job runs it as
  its first step, and no other step anywhere runs it at all.
- **Every key of a file GitHub runs is written out, as a word before its colon on one
  line**, because every rule here reads a key that way. A key after `?` (YAML's explicit
  form, its value after a `:` on the line below), an alias standing for a key, `<<`
  merging a mapping anchored elsewhere, a `{` or `[` left open past its line, inside
  which a key's colon may stand on a later line, and a quoted scalar left open past its
  line, whose following lines are text to YAML and keys to a reader of lines, are each
  refused in both workflows and in a composite action — a double-quoted scalar a backslash
  continues among them. Written any of those ways, `? environment` with `: release-main`
  below it, `{ permissions` with `: write-all }` below it, or `name: "check` above the
  only `permissions: {}` of a file, would pass the rule that reads that key. A quote opens
  a scalar only where YAML starts a node, so `it's` and `echo "a"` are letters of a plain
  scalar; inside a quote a bracket and a `#` are text. **And no `#` the readers end a line
  at where YAML reads on**: they end a line's code at a `#` outside a quote as a shell
  does, with any quote opening a span, so in `{ ke"y: "a # b", permissions: write-all }`
  they stop at the `#`, and YAML reads a string and then a grant. A line YAML ends sooner
  than the readers do is read for more than it holds, and passes. A block scalar's lines
  are a command's text and are not read for braces, quotes or comments; they run to the
  first line indented no further than the key's own column, measured past every `-`
  before the key and all the whitespace after each, or than the `-` a header follows
  with no key. Measured past `- ` alone, the keys of a step written `-   run: |` were
  read as the scalar's text, and a quoted scalar left open on one of them hid the `macos`
  job's gate step from every rule. **And a header opens one wherever YAML writes it**:
  on its key's line or on a line of its own below it, a comment line between them or not,
  with an anchor or a tag before it or not. Read only directly after a key's colon, the
  text under `- run:` with `|` on the line below was read as YAML, where `name:` is a
  label, and `name:() { "$@"; }` with `name: cargo clippy --workspace` after it ran a
  second lints gate with nothing said.
- **Every file CI reaches ends each line at a line feed, and nowhere else**, the justfile
  and a script a step runs among them, because every reader here splits a file there. A
  carriage return with no line feed after it ends a line to YAML, and NEL, LS and PS end
  one to a YAML 1.1 parser, so a key or a step after one on a comment or a step's `name:`
  is read by GitHub and by no rule: each is refused, by name and line. A carriage return
  before a line feed is part of it.
- **And it spaces a line with a space or a tab alone.** Every reader here trims and splits
  a line at whitespace as Rust's `char::is_whitespace` reads it, and YAML indents with a
  space and separates with a space or a tab, so a no-break space, U+3000, or any other
  such character is a letter to YAML and to a shell and a space to every rule. A key
  indented with U+3000 below a block scalar was read as the scalar's text, and the quote
  it left open swallowed the `macos` job's gate step. Each is refused, by code point and
  line.
- **The file and its jobs hold their own keys and no other**: `name`, `on`,
  `permissions`, `concurrency` and `jobs` at the top, the jobs `gate` and `release` and
  no other, `runs-on`, `permissions` and `steps` in the gate, and those with `needs` and
  `environment` in the release (`THE_RELEASE_JOBS`), each once. `env:` or `defaults:`
  would reach every step, the token's among them; `container:` would run each inside an
  image this tree does not build, `services:` would start one beside them, and
  `strategy:` would run the job more than once.
- **Its `concurrency` holds three lines, in order, at one depth**:
  `group: release-${{ github.ref }}`, `cancel-in-progress: false` and `queue: max`
  (`THE_RELEASES_CONCURRENCY`). A run cancelled in progress stops a release part way, and
  under GitHub's default queue a newer run replaces the one waiting, so the next merge
  would cancel the run of a release commit's own push, which is what finishes a release
  that died part way.
- **Each job runs its steps in `THE_RELEASE_STEPS`, in that order, and no other step, and
  each step holds exactly the lines written there for it**: the gate checks out, with
  `fetch-depth: 0` and `persist-credentials: false` and no other input, installs the
  toolchain `rust-toolchain.toml` names and `cargo-deny` at its exact version, and runs
  `cargo xtask dev` and then `cargo xtask check`; the release checks out the same way,
  installs the toolchain, builds the release tool with `cargo xtask release --help`, mints
  the App's token, with `client-id`, `private-key`, `permission-contents: write` and
  `permission-pull-requests: write` as its inputs and no other, and runs
  `cargo xtask release --channel ${{ github.ref_name }} --publish --after-the-gate-job`,
  handed `GH_TOKEN` alone. Each line is held at its depth, so an input moved out of
  `with:` or a value continued on the next line is a line that differs; a step's `name:`
  is its label and is not held. A script, a composite action, a second command on
  a line, another repository or ref checked out, a token asked for more, and a key such as
  `env:`, `shell:` or `working-directory:` on a step are each refused, and a step on a
  condition is too. **The release tool is built in the step before the token is
  minted**, so the build scripts and procedural macros of its dependencies run before any
  step holds the token, and the release step finds it built; and the token, which lives
  an hour, is minted seconds before its first use rather than before a gate.
- **It evaluates six expressions and no other, each on a line of its own where
  `THE_EXPRESSIONS` puts it**: the branch in the concurrency group, the environment and
  the channel; the App's client id and key in the step that mints the token; and
  `GH_TOKEN: ${{ steps.app.outputs.token }}` in the release step. Any other line holding
  `${{` is refused, so the token reaches no other step however an expression names it —
  `steps['app'].outputs.token`, `toJSON(steps)`, `github['token']`, `toJSON(github)` —
  and the file is read the way the other files are: each line on its own and joined to the
  lines a backslash continues, escapes resolved, comments included. The token is minted
  with contents and pull requests to write and nothing more — the two `permission-`
  inputs are what narrow it, and with none the action asks for whatever the App is
  granted — so it stays that narrow if the App is ever granted more, and the action that
  mints it revokes it when the job ends.
- **It restores no cache.** A cache holds what an earlier run saved — `dev`'s are restored
  on every branch — and the release is built from its branch and nothing an earlier run,
  or a pull request, left behind.
- **Both workflows open with `permissions: {}`, and a job asks for `contents: read`** and
  nothing more, in a block, a flow mapping or a word: the release writes with the App's
  token and never with the workflow's, and the gate writes nothing on GitHub. Every
  `permissions` key is read wherever YAML opens one — quoted, behind an escape, after an
  anchor or a tag (a verbatim tag such as `!<tag:yaml.org,2002:str>` runs to its `>`), on
  the line after a backslash that continues nothing, or in a job written whole as a flow
  mapping — and a grant other than `contents: read`, its quotes taken off, is refused.
- **Every action is pinned to a full commit, with its release in a comment** — in either
  workflow, and in a composite action a step reaches — so a tag its owner moves changes
  nothing that runs here. Moving a pin is the commit and the comment, together. Every
  `uses` key of a line is judged wherever YAML opens one: first or after another key of a
  flow mapping, quoted or escaped, after an anchor or a tag, a verbatim tag among them, and
  inside a flow sequence.
  A `uses` whose action is not written plainly on its own line — a value on the line
  below, a block scalar, a value carrying an anchor or a tag, an alias, a collection, or a
  key a backslash carries across a line break — is refused, and the allow-list counts it
  as an action off the list.

### The history says what each change is, credits no agent, and stays small

The `history` step of `cargo xtask check` (`xtask/src/history.rs`) judges every commit
reachable from `HEAD`, merges included, and every blob ever reachable — not the tree at
`HEAD`, because a file added and then removed is still in what every clone downloads
(`NOCTUA.md §22.5`, §17.8):

- **A conventional subject**: one of the ten types, an optional lower-case scope, an
  optional `!`, one space after the colon, and a description that ends neither with a
  period nor in whitespace, on one line with a blank line after it. GitHub's ` (#N)` on a
  merge commit's title is read off first. No character `nc_schema::unseen` answers for,
  because the changelog and the release notes print subjects (`NOCTUA.md §22.4`).
- **No agent credit** in a subject or a body: a `<Key>-by:` or `<Key>-with:` trailer
  naming an agent or a `[bot]`, by a name, by an address under an agent's own site, or by
  an address at the domain of a vendor whose business is agents (`AGENT_VENDORS`: any name
  there, `noreply@` among them, so a person signing from such a domain is refused too; a
  link to one is no credit, since a plugin names the interface it reaches), read as git
  reads a trailer — spaces or a tab before its colon, and its value folded onto the lines
  below it that open with whitespace; a line saying the change was generated, written or
  made with an agent, named right after those words, past an article and a maker's name
  ("generated by GitHub …"); a link to an agent's own site, in any line; or the robot
  emoji. In such a line `cursor` is read only written as a name, `Cursor`, because this
  tree's prose means the word as the terminal's. Naming `AGENTS.md` is not a credit.
- **No blob above `LIMIT`**, one mebibyte, unless `ABOVE_THE_LIMIT` lists its path, with
  a ceiling of its own and its reason. The paths are read from the commits
  (`git diff-tree -z`) and not from the blob, so every path a blob sits at is judged, and
  a listed path is its own bytes: a space at either end, or a carriage return or a
  newline after it, makes another path, held to the limit.
- **Under a pull request, what its event names**: the history of `pull_request.head.sha`
  and `pull_request.base.sha` in `GITHUB_EVENT_PATH`, the title held to both rules and
  the body to the second — never the message of the merge commit the checkout is at,
  which GitHub makes for the run and no branch carries. **That merge's blobs are measured
  with theirs**, and the paths of every commit it reaches read: a three-way merge writes a
  blob neither side holds, so a file under the limit on the head and on the base can be
  above it merged, and nothing makes the base the event names the merge's first parent, so
  the merge can reach commits the base branch gained after it. Those commits' messages are
  judged by the run of the push that brought them, not here.

**Fail closed.** A shallow clone, a grafted one, a partial one missing an object, an
event that names no head, base, title or body, a blob above the limit that no commit puts
at a path, and an answer from git that is not the one asked for are refusals, because
each would judge less history than there is. A tree with no `.git` — a
tarball — is the one shape that stands aside, with a `skipped:` line, asked before git is
started.

**What git reads is this repository's own.** Every `GIT_` variable this process holds is
removed from the child, in any case, since git on Windows reads `git_dir` as `GIT_DIR`.
`GIT_DIR` is then this tree's `.git`, so git searches for no repository: a `.git` that
holds none is a refusal and never the repository around it judged in its place. A list of
directories to stop a search at would not hold, since git splits one at the path separator
a directory's name may carry and drops what is not absolute. `--no-replace-objects` keeps
a replacement ref from showing it another commit, and a graft file, which that flag does
not reach and which git does not call shallow, is refused. `GIT_NO_LAZY_FETCH` keeps it on
this machine, so `--offline` changes nothing here. A test holds each: the child's
environment is read back from the command git is started with; a `GIT_DIR` and a
`GIT_OBJECT_DIRECTORY` naming another repository, handed in as inherited and exported to
the test binary run again, leave this repository the one judged through the entry the gate
calls; and a `.git` that holds no repository is refused under a directory whose name
carries the separator. Messages are read raw through `git cat-file`, never through
`git log`, whose output follows the configuration. What a finding quotes from a commit or
a pull request — a message, a title, a body, or the path a blob sits at, all of it text a
fork's author wrote — has every unseen character written as its escape and is cut short,
and the finding's line opens with the commit's or the blob's hash, so the quotation cannot
move the terminal or become a command the runner obeys. Git's own refusals are escaped the
same way, since git names in them what it could not read.

**The tests write no credit into this history.** The must-fail cases are commits made at
run time in a scratch repository, where `-c commit.gpgsign=false` is set and nowhere
else, and the words they are built from are assembled from parts. A comment that shows
what a credit line looks like leaves the product's name out ("generated by GitHub …"), and
a test runs the step's own reading of credits over `xtask/src/history.rs`,
`xtask/mutants.py` and this file and finds none.

### What is deliberately not claimed

What no entry above owns, grouped by where it lives.

**Narrowed on purpose.** Each is a contract narrowed, or a step deliberately not taken,
rather than anything decided for the project, and each is a maintainer's to reopen:

- **Undo reverses what a step kept, not what it set going**: a timer a step armed stays
  armed and an event it published stays delivered or queued (`NOCTUA.md §11.5`'s scope is
  `store` writes and blobs). Not taken: such a step made a barrier to undo, or its unfired
  timers cancelled by the undo.
- **A compensation a pack's rule holds to a quorum is registered**, and bars its step's
  undo while the rule holds. Not taken: the pack's rules on the compensation's resource
  judged at registration, as the core's are.
- **A plugin's `User-Agent` is sent in place of `noctua-kernel`.** Not taken: `user-agent`
  among the names `NOT_SENT` refuses.
- **This machine's own public addresses are dialled** when a declared name resolves to
  one; refusing them means reading the machine's interfaces as a name resolves — a
  dependency (`NOCTUA.md §2.3`).
- **A plugin killed stays `retryable`** — `NC-PLUG-0002` to `NC-PLUG-0006` in
  `catalog/errors.toml`, whose message says that what it had already sent outside the
  computer may have arrived, so check before trying again. Not taken: `retryable = false`.
- **An open of the local database neither judges its file inside one read transaction nor
  retries the migrator.** Every open through `nc_local` holds the profile's lock while it
  judges and migrates, so none meets either race; a process opening the file another way
  runs as the same user, whom this database does not answer anyway. Not taken: both, as
  defence in depth.
- **`tokio` is a direct dependency of `nc-local`**, for its blocking pool (the
  Dependencies table): already in the graph, and still the developer's to confirm.

**The kernel and the local database**

- **The kernel decides as one person at a time, through test doors for the rest.** No
  agent is handled — `via_agent` false and an empty actor chain on every dispatch until
  agents act (Stage 15) — and the directory is the world held in memory. A confirmation
  and an approval come only from `confirmation_for_tests` and `approval_for_tests`, so a
  shipped kernel denies every command that cannot be undone and every one a quorum must
  approve until the interfaces that issue them exist (a confirmation, the desktop
  interface's, Stage 16). A confirmation is bound to the whole principal, kind and
  identifier, and to its command by the plugin that held it, so one that took an unloaded
  plugin's namespace redeems none issued before. A person's history is read by identifier
  alone, so the directory is what keeps two principals of different kinds from sharing
  one.
- **Plugins can time one another**: they share one process, one machine and one bus, one
  top-level call tree at a time, and nothing pads, batches or randomises a wait — the
  reason the kernel's crate header names timing between plugins as unanswered.
- **Recovery finds what was left open, and not what is broken.** A kernel opening asks
  the journal's store which units of its realm are open (`nc_journal::Store::open_units`):
  the columns that bracket a unit, never a read of the history, so a row nothing can
  decode hides no unit opened after it. Its cost still grows with the history, and not
  only with what was left open: four columns of each entry visited are compared and none
  is decoded, but the local journal's index `(realm, work_unit)` visits every entry of
  the realm inside a unit, whatever the data, since invariant 18 leaves the planner's
  statistics ungathered there; and PostgreSQL, which indexes the realm and the unit apart
  (`journal_entries_realm`, `journal_entries_work_unit`) and the two together nowhere,
  reads the opening brackets and their closes each through those indexes or over the
  whole table, by the cost its planner estimates, so a recovery there may visit every
  entry of every realm. It does not look for a broken row, and logs nothing about a
  history it never reads: `journal.verify` still stops at it and reports it
  (`journal.broken`), and nothing runs that verification on a schedule. An opening
  bracket it cannot name keeps the kernel shut, as the decision point's **Not answered**
  says. Two kernels over one journal would each take the other's open units for a stopped
  process's; one daemon keeps one profile.
- **Nothing runs on a schedule, and nothing put off outlives the kernel.** Timers and
  events are kept on no disk. `nc-prune` is a library the pump runs over the local
  database and `limits.prune` runs when a guardian holding a quorum's approval dispatches
  it, and `Kernel::sweep_blobs` deletes what is due when called; calling them from time to
  time is the daemon's, in Stage 13 (`NOCTUA.md §15.8` asks for a schedule). What is owed
  and gated today is that each is correct, matches its declaration, and is run by the pump
  and the command.
- **`deactivate` is never called**: `Kernel::unload` removes a plugin's commands, timers,
  surfaces and unheard events without calling it, so a plugin cannot count on it.
- **A blob is never fetched, and never announced** — reaching another machine's blobs is
  Stage 25's network; the world's own comment on `open`, saying the kernel fetches what is
  not local, describes the surface as it will be.
- **A capability denied is answered, recorded and logged once per function.** The kernel
  records each function refused, with its code, capability and count, in the plugin's
  store — the record `crates/nc-kernel/tests/capabilities.rs` reads — at most one entry
  per function the world has (`a_million_refusals_keep_one_entry_per_function_refused`),
  and writes `kernel.capability_denied` for each function's first refusal once the call
  that met it returns: a command, a callback
  (`what_a_callback_is_refused_is_logged_once_it_returns_for_the_plugin_it_called`), or
  `activate` once the plugin has a slot, at load and at every restart
  (`a_capability_refused_in_activate_is_logged_once_the_plugin_is_loaded`). A call refused
  for where it ran is recorded and logged the same way, as `kernel.host_call_refused`. Not
  logged: the `activate` refusals of a plugin whose load is then refused, there being no
  slot to name it by — the load's own refusal is what its caller is told.
- **The local database stops other accounts, not the account it belongs to.** SQLite has
  no roles: a process running as the same user can `DROP TRIGGER` and rewrite the images,
  or race the owner and mode checks between check and open, and the next open's refusal of
  a changed schema is detection, not prevention. An administrator, and a disk read offline
  without full-disk encryption, are not answered — the file is not encrypted. SQLite's
  `DEFENSIVE` flag and an authorizer are out of `sqlx`'s safe API's reach, and nothing
  here runs SQL this repository did not write — the premise the bundled SQLite's floor
  (3.50.2, CVE-2025-6965) and the later FTS5 advisories rest on. The blobs' directory
  stops the same people and no more. An open leaves a `-shm` it gave a `-wal` that had
  none (an index holding no data) beside a file it then refuses, and leaves what it made
  before it judged the path it refuses: the database file, when it was missing and a
  companion is then refused, and on Windows the profile's lock file
  (`nc_local::LOCK_NAME`), made before the database file is judged. It waits without
  bound for another open of the profile, so a process stopped mid-open holds every other.
- **On Windows, the system's own can read or take anything** — the Administrators group,
  `SYSTEM`, and any process holding the backup or the take-ownership privilege —
  whatever the access list says: Windows' counterpart of root, which no access list
  answers. And a companion SQLite made beside the database — its `-wal`, `-shm` or
  `-journal`, and nothing else — owned by the group the process token gives what this
  process creates (the Administrators group, in an elevated process) is taken as the
  user's: SQLite creates them with the token's default owner, and that group's members
  are the ones who can take any file already. The directory, the database file, the lock
  file and the blobs' directory are this crate's, created owned by the user, and one owned
  by anybody else is refused — the group above included, at each call that judges one:
  `what_this_crate_made_owned_as_the_system_owns_what_it_creates_is_refused` and
  `a_lock_file_that_is_not_its_own_is_refused_where_a_file_holds_the_directory` stage that
  owner where the process is elevated, and say there is nothing to stage where it is the
  user. A deny entry is read as this user's whoever it names — which groups a user is in
  is the token's to say, and reading every deny as this user's refuses more, never less. A path past Windows' 260-character limit
  is refused by the system, since nothing here writes the `\\?\` form for it.
- **A journal entry's authorization fields are the kernel's to fill, and one stays
  empty**: `actor_chain`, until agents act (Stage 15); an entry another writer makes
  carries what that writer gives it.

**Channels and installs**

- **A checkout is 32 bits of its path's digest**, kept because it already names every
  developer's compose volumes: two checkouts whose paths share a digest are one checkout
  to every name, and two whose digests fall on one pair of the port block
  (`nc_channel::PORT_PAIRS`) are derived the same stack ports, which the second
  `cargo xtask dev` refuses by name until one of them is moved.
- **The stack is the checkout's, not its user's**: two accounts working in one checkout
  directory share one compose project and its ports, on a Docker daemon and a loopback
  the machine shares, and none of the three carries the user.
- **No process runs the daemon's start yet**: the library holds the start up to the mark —
  the install, the profile, the device, the world and a minter behind its ceiling — and
  `noctua-daemon` answers `--initialize` and nothing else, so no channel's daemon keeps
  running or serves its kernel. The local journal's newest unit is the start's to read and
  hand to `nc_daemon::mark::minter`, and nothing reads it yet, so no start resumes above a
  unit the mark does not cover; no stop calls `nc_ids::Minter::settle_ceiling`, so none
  writes the exact high-water mark yet; and no start makes, judges or reads the plugins
  directory under the data one (`nc_channel::Layout::plugins`), because nothing loads a
  plugin from it yet. Nor does any daemon bind its endpoint or serve the protocol:
  `nc_ipc::listen`, `nc_ipc::serve` and `nc_ipc::connect` are what a running daemon and
  a client speak through, and nothing writes the channel's token into the runtime
  directory or reads it back, so neither the token file's privacy nor a client's
  holding of the welcome's process to the system's and to the PID record is made yet.
- **Another account is kept off the endpoint by what no test can be**: no hosted runner
  has a second account and an unprivileged test cannot make one, so a connection from
  another user is never made. What stands in for it, on every system, is the endpoint read back
  as made (`0600`, or the pipe's one entry), each way another user can reach the runtime
  directory refused, the peer predicate refusing a forged user
  (`nc_platform::testing::someone_else`), and the process read held to a child's rather
  than this process's own — so a reader answering this process's credentials is caught,
  and one answering a third process's would be caught only where that process ran as
  another user.
- **A device's seed is wiped as far as safe Rust can say**: the key `ed25519-dalek` holds
  zeroizes itself, and the copies read from the file are overwritten and passed through
  `core::hint::black_box`, which no compiler promises to honour.
- **Two development checkouts share the development Leader** until one is configured, and
  a relocated install shares its channel's — the default is one chord per channel
  (`NOCTUA.md §12.8`), and the configuration that refuses the collision is the
  interface's (Stage 16).

**The journal and the central log**

- **The chain has no anchor outside this machine.** A chain proves only that no entry was
  changed or removed *without also rewriting every entry after it*, so it is silent about
  a history shorter, longer or rewritten whole (`NOCTUA.md §14.9`); Stage 10's criterion 3
  holds against every single-point mutation of an earlier entry — `ALTER TABLE … USING`
  included, which fires no row trigger — because its quantifier is over *earlier* entries.
  An actor owning the table can rewrite the history and recompute every hash after it,
  with no `UPDATE`, `DELETE`, replication mode or disabled trigger, and `verify` answers
  clean — and so it does for a raw `INSERT` of a forged tail, which `0001_journal.sql`
  calls a privilege question, and for a history cut short. The chain carries no secret, so
  an anchor off this machine is what closes it, which `lib.rs` defers and
  `NOCTUA.md §14.9` lists in cost order for a later stage. An `INSERT`-only party can also
  write a `unit-committed` bracket for a unit still open, locking its writer out while the
  history reads clean and making `Store::unit_closing` report the forger's outcome;
  closing that needs the digest **keyed**. And it can write a `unit-begun` bracket naming
  its unit in a spelling this code never writes — `journal_entries.work_unit` holds any
  text — which `Store::open_units` refuses by name, so no kernel of that realm opens until
  the row is repaired by hand.
- **The append-only triggers rest on a function their owner can replace**: all three call
  `journal_is_append_only()`, and one `CREATE OR REPLACE FUNCTION` with an empty body
  needs **function ownership only** — every trigger still there and still
  `tgenabled = 'A'` while `UPDATE`, `DELETE` and `TRUNCATE` pass. A test reads
  `pg_trigger` and `pg_proc` rather than assuming them; a role that owns nothing closes
  it, `NOCTUA.md §18`'s to give.
- **An identifier reaches the central log as it is** (`NOCTUA.md §15.4`), and
  `Snowflake::from_crockford` accepts one to thirteen Crockford base32 characters (all
  else, bar the three documented confusables, is `NotBase32`), so one minted *out of*
  personal data would reach the log through it. `OpaqueId` is the same shape, and
  `type = "id"` is refused outright by the catalog compiler, naming `NOCTUA.md §5.1`,
  which is `[OPEN]` and owns the residual; the logging call site is where this repository
  closes the part it can reach.
- **The kernel and the daemon are the two things that write to the central log** — a
  dispatch refused, a capability denied, a plugin killed, restarted or stopped, a unit
  aborted, a reversal refused (`kernel.revert_failed`), a unit recovered
  (`kernel.recovered`), a step undone or redone (`kernel.step_reversed`), a fetch refused
  (`net.refused`), the chain verified or broken (`journal.verified`, `journal.broken`),
  and a start whose clock is behind its mark by more than the lease
  (`ids.clock_backwards`) — each a struct `catalog/log-events.toml` generates; nothing else
  of the platform logs yet. A daemon hands the kernel `nc_log::Log` behind a mutex, the
  one `nc_kernel::logging::LogSink` it writes its own events to too; the kernel's suites
  log into a capture,
  and `the_central_log_sink_ships_what_the_kernel_records` in
  `crates/nc-kernel/src/logging.rs` drives the real shipper to a store it stands up.
- **A backwards clock is logged where a daemon starts, and not where a minter refuses mid-run**:
  a start whose clock is behind the mark by more than `mark_lease_ms` writes
  `ids.clock_backwards` once, with how far, and waits (`apps/daemon/src/mark.rs`); a clock
  that steps back while the daemon runs is refused by the minter with
  `MintError::ClockMovedBackwards`, which is `nc-ids`', and `nc-ids` cannot reach the log,
  so an operator sees `kernel.dispatch_refused` with that code.
- **A prune is not logged**: `prune.pruned` and `prune.within_limit` carry the table as a
  `literal`, a type whose constant is the text, and the table is chosen at run time, so
  nothing constructs either; what a prune removed is in `limits.prune`'s answer and
  `Pumped::pruned`. Logging one needs the table as an `enum` over the bounded tables,
  which `catalog/log-events.toml`'s comment on `prune.pruned` names.
- **A shutdown's bound is per shipper, not per process**: one `Shipper` costs
  `FLUSH + GRACE` of network and one `GRACE` of stderr on `Drop`, and `Log::open` is
  public, so a process holding two pays twice.
- **`NC-VAL-0001` publishes a template nothing fills**: the only code with `params`, a
  response being a key *plus parameters* (`NOCTUA.md §10.9`), and no consumer interpolates
  yet, so a caller supplying no values shows `{field} must be between {min} and {max}.` in
  both locales. `MachineFlake::from_value` supplies them (`bounds()` the minimum and
  maximum, `with_label()` all three); the surfaces that show a person a message arrive
  with the CLI and the interface (Stages 13 and 16).
- **Some error codes are declared and nothing raises them**: `raising = "reserved"` in
  `catalog/errors.toml`, published on `docs/errors.md`, held by a gate that looks for each
  code's own `ErrorCode::` variant in `crates/` outside test modules — a `reserved` code
  that gains a producer, or a `raised` one that loses its last, fails the build.
  `grep -c '^raising = "reserved"' catalog/errors.toml` answers how many (anchored: a
  comment quotes the key). `NC-LOG-0001` and `NC-LOG-0002` are never constructed, because
  `nc-log` writes its own failures to stderr.
- **`masked` in `access_records` is a risk signal, never a control**: free VPN and
  datacenter lists miss residential proxies, so nothing may be gated on it
  (`NOCTUA.md §15.6`).

**The policy packs**

- **`org/` and `collective/` hold a README and no policy.** Each says what it is for and
  carries no `pack.toml`, since a manifest stands only beside policies — so `office`'s pin
  in `policy/world.json` names a pack this tree does not carry, held to its own profile
  (`org@…`, refused otherwise as `realm-pins-another-profile`) with nothing on disk
  holding the version. They exist because `NOCTUA.md §14.4`'s indirection is what stops
  them being a rewrite. `ls policy/packs/*/README.md` says how many packs there are, and
  two counts `check` prints read alike: `core policy file(s) are on disk` counts **files**
  under `policy/core/`, `policy(ies) parse and validate` the `@id` policies in them and in
  `family`.

**The gate and its run**

- **`cargo xtask check` passes on a checkout of this repository alone, and says what it
  did not check.** Against a **migrated** database a checkout makes
  **five** announcements and a pass: the steps that read the workspace documents above it
  (`NOCTUA.md §3.1`) — the written-count gate, the citation gate, the section gate, the
  step holding `policy/schema.cedarschema` against the reference — and the step resolving
  the design system's sibling manifest, which `<workspace>/repos/noctua-design` supplies.
  The number is written once, in the `expected=` of `xtask/the-checkout-shape.sh`;
  `just checkout-shape` copies this repository without its target directory to where no
  workspace marker is above it, runs the gate and asserts the count as an equality, and
  this file, `docs/README.md` and `xtask/canaries/README.md` are held to it. A hand-made copy
  must touch its sources first, because `xtask` bakes `CARGO_MANIFEST_DIR` in at compile
  time. Against an **empty** database the copy also fails the migration corroboration, the
  search paths and the journal's protections, and exits 1; `what_the_record_says`'s own
  tests cover the migration comparison everywhere, needing no database.

  **Every announcement carries the `skipped:` prefix, and the prefix is the whole rule**:
  `ui::warn` counts a warning as an announced skip only when the line starts with it, and
  `finish` fails the run on any other
  (`error a step was skipped without being asked to be`); `not run:` is what the verdict
  prints after `shortened()` takes the prefix off, and written at a call site it is the
  spelling that fails. In a checkout the written-count gate still judges the documents
  here and the reference gate every path into this tree, and the tests about the workspace
  print `skipped:` and return — counted as passed, so only the test line's clause says so
  (`STANDS_ASIDE_WITHOUT_A_WORKSPACE` in `xtask/src/check.rs` lists them). These `warn`
  lines are the intended state of a checkout under Stage 10's criterion 1 — the gate
  naming what it cannot answer, the verdict line carrying their number — and inside a
  workspace laid out as `NOCTUA.md §3.1` describes they all go.

  **On an image store without the two pinned tags**, the image-pin step announces one more
  per tag (invariant 9), so a CI runner, which pulls neither, makes two more and still
  passes; the recipe's failure message says so and names the two `docker pull` commands.
  **No `§` citation is checked in CI at all**: a checkout has no workspace for one to
  point into, so the section gate is local-only — by decision, since the alternative is a
  second copy of the document that outranks (`NOCTUA.md §0.4`). **A workspace is named by
  its `.noctua-workspace` marker** (`NOCTUA.md §3.6`), and only when this repository sits
  inside that workspace's `repos/`; a marker that does not hold this tree is somebody
  else's, and the run says so. One function, `what_is_above` in `xtask/src/sections.rs`,
  answers for the whole tool; if a workspace gate reports something you cannot find, run
  `ls -a` up the parents and see which marker it found.
- **A skipped step is announced, and the verdict names it**:
  `check passed, with N warn line(s) above` is followed by one `not run:` line per skip,
  and `--offline` adds its own **nine** on top — the one figure here read from a constant,
  `OFFLINE_SKIPS`. A run with no workspace above it is not distinguishable *by the number*
  from one where the section gate broke, which is why the names are printed.
- **`--offline` keeps the gate off the network, not the build of the gate**: under the
  flag the fetch does not run and every cargo the gate starts is handed
  `CARGO_NET_OFFLINE=true`, but `cargo xtask` builds this tool before the gate starts,
  and that build fetches what `xtask` itself lacks unless cargo is asked as well
  (`cargo --offline xtask check --offline`). And the flag reaches each cargo through the
  steps that start one, as a parameter: a step handed the wrong value would be seen only
  by a run under `--offline` on a machine missing a package, which neither the local full
  check nor CI makes. `every_cargo_the_gate_starts_under_the_flag_stays_off_the_network`
  holds the place the child is built, and not every way to it.
- **On macOS and Windows the stack's steps do not run at all**: the journal's protections,
  the migration record, where `pg_temp` sits in the migrated functions' paths, the log
  store's bounds, the database's sort order, the digest comparison, the PostgreSQL suites
  and the stack's own tests are the Linux job's alone, and each says so on those systems,
  named beside the verdict as `not applicable here:`. A defect that shows only with the
  stack on macOS or Windows is caught by nothing, and no stack is run there by decision
  (`NOCTUA.md §22.2`).
- **On Windows the checkout script refuses to run, and its guards are proven elsewhere**:
  each guard before its `rm -rf` is a claim about how a POSIX shell spends a path —
  `cd -P` resolving a link before the `..` after it, two leading slashes being one
  directory, an absolute path opening with `/` — and the `sh` Git ships for Windows keeps
  none of them. Measured on a Windows runner, a decoy reaching the script through a
  junction passed every guard as the repository the script belongs to. So the script asks
  `uname -s` before it derives anything from a path, and a shell naming a POSIX layer over
  Windows (`MINGW*`, `MSYS*`, `CYGWIN*`, or anything with `_NT` in it) is refused, with
  exit 2 and the reason; a `uname` that fails stops the script under `set -e` instead of
  reading as no answer. `the_script_refuses_a_system_its_guards_do_not_hold_on` holds the
  refusal on every system — on Windows by the system's own answer, elsewhere through a
  `uname` planted on the path, with nothing removed or copied either way — and the failing
  `uname` on Linux and macOS; the drives of the guards themselves stand aside on Windows,
  named by the gate there as not applicable, while the Linux and macOS jobs run them. A
  layer over Windows that names itself some other way to `uname` is not refused, and runs
  guards no test holds.
- **That the catalog's output does not follow a directory's order is measured on Linux
  alone**: macOS lists a directory by a hash of each name and Windows by the name, so two
  layouts of the locale files come back in one order there and the suite holding the
  output to both stands aside, named by the gate as not applicable. The sort it holds is
  the same code on every system.
- **Which suites stand aside off Linux is the system's answer, not what the run saw**: the
  gate names a suite as not applicable from `nc_platform::system`'s constants, the ones its
  guard asks, and the census of guards holds each guard to deciding on that alone. A test
  that returns early by any other path is counted by cargo as passed and named by
  nothing.
- **An answer a reset overtakes is lost on Windows**: a store that closes the connection
  with the request unread resets it, and Windows discards what of the answer the shipper
  had not read yet. What the shipper had read decides the rest. Part of the status line,
  and the batch is an answer it could not read (`Delivery::Unreadable`): a store that
  answered, its batch kept on stderr as a possible duplicate. None of it, and the batch is
  `Delivery::Unreachable`: a store that answered, read as one that has gone, its batch on
  stderr as a possible duplicate — and during a shutdown it counts toward `GIVE_UP_AFTER`,
  so two in a row send what is left of the queue to stderr without trying the store again.
  The second is what a Windows runner measured (`os error 10054`). macOS refuses a new read
  bound on such a socket and still holds the answer, so the shipper reads it under the
  bound already set (`an_answer_a_reset_follows_is_not_read_as_silence`).
- **What a CI run supplies is GitHub's**: the runner's image, and whether
  `cargo xtask dev` comes up on it, are pinned by nothing here; the gate reads
  `.github/workflows/ci.yml` as a document ("CI's one gate reaches every file CI can
  reach"), so CI and a developer cannot drift into two gates. It carries no `services:` on
  purpose ("CI runs the stack, not a copy of it").
- **A required check is a job's name, and a pull request runs the workflow it brings**:
  the rulesets require jobs named `linux`, `macos`, `windows` and `deterministic` from
  GitHub Actions, and a pull request's run reads `ci.yml` from the merge made for it, so
  one that rewrites `ci.yml` so those jobs pass without the gate meets the rulesets. The
  workflow reader runs inside the gate such a file would skip; what holds a change to the
  workflows is its diff, read by whoever merges.
- **A pinned action is the code at its commit, not code anyone here has read**: the pin
  keeps a moved tag from changing what runs, and nothing reads what the action does or
  fetches when it runs. The release written beside a pin is a comment no gate checks
  against the action's own tags, which would take the network.
- **The release's rules read the workflow, not GitHub**: the environments' branch
  policies, their variable and their secret, the rulesets and the App's permissions are
  GitHub's settings, read with `gh api` and held by nothing in this tree.
- **The release tool's own dependencies are the boundary of the release job.** That job
  holds the App's key from its start — a runner is handed a job's secrets when it starts
  the job — and the token from the step that mints it, and it runs the release tool,
  built from this tree and its locked dependencies in the step before the mint, and `gh`.
  A build script, a procedural macro or the library code of one of those dependencies, or
  a process an earlier step of the job left running, runs as the same user on a runner
  where that user has passwordless sudo, and could read the key from the runner's memory
  or the token from the release's environment. What keeps such code out is the lockfile,
  the audit and review: what runs is what `staging` or `main` holds, after every check.
  The gate's own build scripts, procedural macros and tests, of the tree and of every
  dependency, and the auditor it installs, run in the gate job, which holds neither.
- **`--after-the-gate-job` is held to its job by what that job's environment says**:
  `GITHUB_ACTIONS`, `GITHUB_JOB`, `GITHUB_WORKFLOW_REF` and `GITHUB_REPOSITORY`, which
  anyone running the verb can set. It stops the flag passed by mistake, or copied where
  no gate runs first; on a person's own machine whether their own gate runs is theirs to
  decide, and with no token of the App's `--publish` has nothing to publish with. That the
  release job's gate ran is held by `needs: gate`, which the reader holds `release.yml`
  to, and by GitHub, which never starts a job whose `needs` did not succeed.
- **`persist-credentials: false` is held only in `release.yml`**: every checkout's
  `persist-credentials: false` in `ci.yml` is in the file and held by no rule, and a
  credential a checkout of `ci.yml` left behind reads the tree and nothing more.
- **GitHub keeps up to a hundred runs waiting in a concurrency group**: a run past that
  is not kept waiting, and when it is the run of a release commit's own push, re-running
  it is the repair. Two releases racing on one branch are stopped by the group, by
  `createCommitOnBranch`'s expected head and by a tag that exists.
- **A cargo command, and a name for one, is read from the words a line writes, not from
  the word a shell builds or reads as it runs**: `"car""go"`, `car"go"`, `$'cargo'`, a
  parameter expansion such as `${C:-cargo}` or `${C:=cargo}`, a brace expansion such as
  `{cargo,}`, a format's parts, as `printf -v C %s%s car go` joins them, a word split
  from a longer one at `IFS`, as `IFS=, read -r A C <<< "x,cargo"` does, and a value read
  from a file, as `read -r C < tool.txt` and `C=$(cat tool.txt)` read one, each run cargo,
  and no reader here joins the parts or opens the file, so a second gate spelled that way
  in a file CI reaches is read by review and by no rule. A reader that did would be a
  shell. A name a command assigns from a word the line writes whole is read: `printf -v
  C cargo`, `read C <<< cargo`, `for C in cargo`, `set -- cargo` and a step's `env:`
  holding `C: cargo` each name `cargo` as a word, which is read as a cargo command.
- **What only the release may hold is refused elsewhere by the words it is written with**:
  a shell that builds `--publish` from parts a reader of words cannot join, such as a
  variable or a command substitution, passes it — and a publish with no token of the
  App's has nothing to write with, because the App's key is in an environment, the
  workflows' own tokens may read the tree and nothing more, and the key, the environment
  and the token's action are what the rule refuses. The workflow's own token, by
  `github.token` or `toJSON(github)`, is not refused outside the release: it reads the
  tree, which every job that checks out already does.
- **The release's top-level `name:` holds what it likes**: it is a label, and an expression
  in it is refused like any other.
- **The token action is handed the App's client id from an environment's variable**,
  `RELEASE_APP_CLIENT_ID`, whose value is GitHub's setting: the reader holds the line that
  names it, never what it holds. No variable holds the App's numeric id.
- **The toolchain is whatever `stable` is on the day**, installed by the runner image's
  rustup, which reads `rust-toolchain.toml` because no toolchain is named; a rustup that
  needs one named fails that step.
- **The readers of keys read YAML as lines, not as a parsed document**: a key is a word,
  plain or quoted, where YAML may open one on a line, after any anchor or tag of its own,
  with its colon on that line. Each way YAML has of putting a key's text elsewhere is
  refused rather than parsed: `?`, an alias, `<<`, a flow collection or a quoted scalar
  left open past its line, a `#` the readers end a line at where YAML reads on, a line
  ended at a carriage return, NEL, LS or PS, and a line spaced with any whitespace but a
  space and a tab. A spelling outside those, if YAML has
  one, is read by none of the rules that read keys; the word `secrets`, which is refused
  wherever it stands in a file GitHub runs, is then what keeps the App's key out of a job
  whatever environment it enters. A double-quoted scalar a backslash continues is
  refused, though YAML reads it as one line, because its second line read on its own is
  text read as keys; and a `{` or `[` outside a quote is counted wherever it stands, a
  plain scalar's own among them, so `run: echo {` is refused as a collection left open.
  Both refuse more than YAML holds, never less. A line inside a block scalar is text to
  YAML and is not read for braces, quotes or comments, and is read as a command, its
  `name:` among them. A block scalar is recognised wherever YAML 1.2 lets its header
  stand: after its key's colon or a sequence entry's `-`, or on a line of its own below
  either, a comment line between them or not, with an anchor or a tag before it or not.
  It ends at the first line indented no further than that key's own column, measured
  past every `-` before the key and all the whitespace after each, or than the `-`
  itself. A line indented further than a key or a `-` whose value is already written
  runs that value on, as a plain scalar's next line does, and opens nothing, whatever it
  holds: `run: echo` with `- |`, `cargo xtask check` and `--offline` below it is the one
  command `echo - | cargo xtask check --offline`, and is said to run the gate where it
  is not the gate. **A header this reader did not know would refuse less, not more**:
  its text would be read as YAML, where a `name:` is a label, and a line of YAML read as a
  block's text has its quotes and brackets unread. Each shape is measured with `yq 3.4.3`
  or the PyYAML it wraps; this reader is not a YAML parser, and a shape outside them, if
  YAML has one, is one it reads wrong.
- **A directory a command runs in is read from `working-directory`, `cd` and `pushd`
  alone**: a script named relative to a directory entered another way, such as `env -C`,
  `make -C`, `git -C` or a variable, is not looked for there, and is read only if a word
  names it from the root or from one of those directories. And the walk refuses more than
  a shell runs in four places: `x=cargo` inside a quoted word is read as a name for
  cargo; a word naming a workflow or a composite action's file where a step finds a
  script is refused though the command only reads it, as `cat` does; on a line GitHub
  reads as YAML, a `#` glued to a double-quoted scalar's closing quote is read as a shell
  reads it, though a YAML parser ends the line there; and an anchor on such a line, as in
  `run: &a |`, is read as a command sent to the background with `&`.
- **The workflow reader reads a file written to run the gate, not one written to hide a
  command from it**: a file CI reaches that is not UTF-8 is passed over, not refused; a
  path a step names is joined as written, so an absolute one, or one that climbs above the
  root, is read where it leads; and of a shell's globs only `*` is expanded, so a file
  named by `?` or `[…]` is not read. A shape built to slip past it is held by the diff, as
  any change to the workflows is ("A required check is a job's name, and a pull request
  runs the workflow it brings").
- **The comparison gate's subject is public fields and accessors, not everything the
  compiler writes**: `pub const` arrays (`AREAS`, `KEYS`, `COMMANDS`, `TABLES`, `STORES`,
  `RATE_LIMITS`, `EVENTS`, each `ALL`), enum variants, functions taking an argument, and
  other `pub const`s (`ARGON2ID_PASSWORD`, `ARGON2ID_DEVICE_PIN`) are outside it — each
  compared somewhere today, by no rule. The blind spot is the **shape** — not a `pub`
  field, not a `pub fn(self)` — which `emitted()` writes down; the names illustrate it.
- **The history step reads what a message says, never who wrote it.** A commit authored
  under an agent's own name passes when its message credits nobody: GitHub commits and
  signs each merge commit, whose author is whoever merged; the release App authors its
  own; and a rule over identities would have to name the App's account exactly — a wrong
  name there is a permanent failure over a history that is never rewritten.
- **A credit in a shape the history step's lists do not hold passes**: a product missing
  from `AGENT_NAMES` or `AGENT_SITES` in `xtask/src/history.rs`, signing from a vendor
  `AGENT_VENDORS` does not hold — one that makes more than agents, such as GitHub, Google
  or Microsoft, is left out on purpose, so a person's `users.noreply.github.com` address
  passes and an agent signing from such a domain is read by its name or its `[bot]` alone;
  a product that shares a person's first name, which is left out of the names on purpose
  and is read only by its site, `claude` being the one first name kept (below); a sentence
  that names the agent past more than an article and a maker's name, or past a maker
  `MAKERS` does not hold ("made with the help of …", "generated by GitHub's …"); `cursor`
  in lower case in a sentence, which this tree's prose means as the terminal's; a trailer
  under a separator other than the colon, which git reads only where it is configured to;
  or prose that credits an agent in other words.
  The lists are the shapes tools write ("A needle list cannot enumerate a language").
- **A commit the history step refuses on a shared branch stays refused**: the history is
  never rewritten, so every run after it fails until the step changes. What keeps one from
  landing is that the step runs on the pull request, whose checks the rulesets require
  before a merge — but it judges the title and the body as the event carried them at the
  pull request's last run, and a re-run reads that same event. `ci.yml` runs on
  `edited` as well as on GitHub's default types, so an edited title or body starts a run
  that judges it; a merge message edited in GitHub's merge dialog, or by `gh pr merge
  --subject/--body`, is judged first on the push to the branch: after the merge has
  landed.
- **A trailer naming a person called Claude is refused**: `claude` is the one person's
  first name `AGENT_NAMES` keeps, because tools sign with it with no address under an
  agent's site, so an honest co-author or reviewer of that first name is refused as a
  credit, and so is a Jean-Claude, since a hyphen ends a word. Such a trailer is written
  with the person's other names and an address that does not hold that first name as a
  word, since the address is read word by word too. A test holds the refusal and the way
  out, so a change to either changes this sentence too.
- **A base that moved after the pull request's last run is not covered**: the rulesets
  require the checks with strict status checks off, so a pull request merges on a run
  made against its base as the base was then, and a blob the real merge writes, from a
  base that gained a commit since, is judged first on the push to the branch.
- **The size limit is the one rule about binaries**: a small binary passes the history
  step, and `NOCTUA.md §17.8` is held there by review. A blob above the limit is judged at
  every path a commit puts it at; one below it is measured once, whatever its paths — one
  object in every clone either way.
- **Three of the history step's refusals have no case**: a `HEAD` git names with
  something other than a hash, an answer other than `true` or `false` to whether the clone
  is shallow, and a line of `rev-list` that is not a hash are each refused by name, and no
  test reaches them, because a working git gives none of those answers. They stop the step
  where git answers something it cannot read.
- **A release commit is known by its subject and its one parent**, never by who made it:
  on `staging` and `main` every commit a person brings is a merge, and only the release App
  may push to either directly, so a commit there titled `chore(release): v…` with one
  parent is the App's. The release does not ask GitHub who authored it.
- **CHANGELOG.md on `dev` and on a feature branch is the last release's**: only the
  release writes it, and no step holds it to the history in between.
- **CHANGELOG.md is read by the citation gates as any document is**: a path or a `§` a
  commit's subject cites is held to the tree there for as long as that release is listed,
  so a subject citing a file later renamed fails the citation step on every branch the
  changelog has reached, and the history is never rewritten. Review of the subject is what
  keeps one from landing; the counts gate alone stands aside, since a number in a subject
  is the tree as it was.
- **A breaking change with no `BREAKING CHANGE:` footer is listed with nothing to do**:
  the history step does not require the footer, and the changelog says what the commit
  says.
- **GitHub's limits on a release's assets are the ones read on the date
  `xtask/src/release.rs` gives**: nothing fetches the page, so a limit GitHub moves is
  moved by a person who reads it again.
- **The pull request carrying a release back is looked for among the newest hundred**
  whose head branch has the released branch's name, forks' among them. A hundred forks'
  opened after it hide it, and the release then asks GitHub for a second, which GitHub
  refuses while the first is open: the release stops, by name, until they are closed. No
  page past the first is read.
- **Whether that pull request is merged now or queued is GitHub's answer when the release
  asked**, on a run carried on from the release commit: checks that finish between the
  question and the request make GitHub refuse auto-merge, and that run fails; the next run
  reads them passed and merges. A run that made the release commit never merges now.
- **Whether the branch below holds the release commit is GitHub's answer when the release
  asked**, the same way: an earlier release's pull request that merges between that
  question and the one the release opens makes GitHub refuse the second, and that run
  fails; the next run finds the branch below holding it and ends `already released`.
- **A release stopped between a write and its put-back leaves what it wrote**: a kill or a
  power cut there runs no put-back, and one that fails is named and not tried again. The
  release begins on a clean tree, so `git status` names what is left, and the relock's
  window is the one it warns about as that window opens. A listed file that cannot be read
  back right after it was written is put back by the same path as one that cannot be
  written, and no test can make that read fail.
- **The release's own process holds the App's token for its whole run**, in the
  environment CI's step gives it, and so does the `cargo run` the alias starts it with:
  every child but `gh` is started without it, and a dependency of this tool's own running
  code inside this process could still read it. That cargo builds nothing there only
  because the step before the mint, `cargo xtask release --help`, built this tool through
  the same alias; anything that made cargo build it again in the release step would run
  every build script of its dependencies with the token in its environment.
- **A file the release writes is judged before the first write, not held while it
  writes**: a process that replaced one with a link between the judgement and the write
  is not stopped. The runner is the job's alone.
- **A pull request back GitHub has not yet judged is queued**: GitHub works out whether a
  pull request conflicts after it is opened, so the one the release opens reads as no
  conflict, and auto-merge refused is what says otherwise then. One GitHub finds in
  conflict after auto-merge is on waits until a run on the release commit asks again —
  the run its push starts is one — or a person carries the release back.
- **A migration's comments are frozen with it**: sqlx's migrator keeps a checksum of each
  migration it applied and refuses to run against a database whose applied file has
  changed since, so `migrations/0002_access_records.sql`'s comment on the identifier
  column, which says what it said before, stays as written.
- **`cargo xtask dev` from a clean machine, anywhere but CI's Linux runner** — Stage 10's
  criterion 10 runs in the `linux` job on every push and pull request and in
  `release.yml`'s gate job, each on a fresh Ubuntu runner that pulls both pinned images;
  macOS and Windows run no stack (`NOCTUA.md §22.2`), and a local `dev --down` empties the
  database and leaves the image store, so it is not that run.

## How the gates are built

These rules bind anyone writing or changing a gate, a test or a census here. Each names a
way a gate in this tree can pass while blind, and what stops it; the general forms are
`NOCTUA.md §19.3` and `§19.5`.

### What cargo builds is what the gates read

Three gates are scoped by cargo's answer to "which crates are here, and which reach the
log" — the type wall's source gate, the workspace lints and the source-root floor
(invariant 5) — and **a resolution question is cargo's to answer**: a dependency renamed
in `[workspace.dependencies]` never spells `nc-log` in its consumer's manifest, a path
dependency inside the workspace directory is a member without `members` listing it, and a
re-export (`pub use nc_log;`) hands the log to every dependant, so the question is
*reachability*, which only the resolved graph has (`cargo metadata` without `--no-deps`).
A nested workspace is no member at all, so every `Cargo.toml` in this tree must be the
root's or a member's. **A path dependency outside the tree is refused**:
`crate::members::members` keeps only packages under the root, so one outside would escape
`unsafe_code = "forbid"`, the type wall and every floor, and the build would need a
directory no clone has; `check` reads the resolved graph for path packages outside the
root and fails on any it does not name. The one it names is the design system, with
`NOCTUA.md §10.11`'s reason written in the gate, since a manifest is what an edit changes.

### Where the platform's code is, is one list

`MEMBER_TREES` in `xtask/src/members.rs` is every tree `Cargo.toml`'s `members` names,
held to it both ways (`the_member_trees_are_the_globs_cargo_reads`), with the naming
that turns a member's directory into its package. Every walk and predicate that asks
where the platform's code is reads it — the §15.1 walk and its floor, the raising walk
and its floor, the hatch walk, the doors' feature, the static-string floor, the
database suites the mutation runner must name, the repository map's test, the
source-count walk, the package a mutated file belongs to, and `TOUCH` in
`xtask/mutants.py` — so a tree the manifest gains is read by all of them in the same
edit, and a tree not on disk is skipped rather than a panic. Spelled in each, a tree
added beside `crates/` and `plugins/` would sit outside every one of them with each
floor balanced, because the floor recounts the same spelling.
`every_gate_reads_every_member_tree` plants each gate's target in every tree the list
names, one that holds no member yet included.

### A scanner narrower than the language it scans

A hand-kept list tied to the tree by a scanner reading less of Rust than Rust writes
passes with a large, plausible count while its subject walks past: a `pub(crate)` field
read as private, and so left outside a hash — Stage 10's criterion 3 false by one word; a
signature rustfmt wrapped past one line, which is how this tree writes long ones; a `cfg`
key the list did not name, `#[cfg(debug_assertions)]` being a *profile*; a YAML sibling at
an unexpected indentation. Read the language, not a convenient prefix of it: every
visibility, the whole signature, the attribute form as well as the macro, a structural
walk by path rather than by column.

### A needle list cannot enumerate a language

The type wall's second line is a list of spellings — `Box::leak`, `String::leak`,
`Vec::leak`, `.leak()`, `into_boxed_str`, `Box::into_raw` — and ordinary Rust walks
through any such list: a turbofish between type and method, removed before the list is
consulted, or a function handing back a `&'static str` kept in a `OnceLock`. So the rule
is the shape, not the call: outside `nc-log`, a non-`const` function returning
`&'static str` is a finding. A function returning a literal is a `const fn`; anything else
returns `String` or carries a hatch naming what it hands back.

### A comparison relates two things, and one of them is the registry

`xtask/tests/every_generated_value_is_compared.rs` requires every public field and
accessor of the generated crate to be named by a comparison. Coverage may be indirect —
`Locale::index()` through `nc_catalog::message` — so there is an `INDIRECT` list whose
covering calls must be compared too, and `crates/nc-log/src/events.rs` is covered by
`BY_ITERATION`, because its comparison walks `catalog/log-events.toml` and builds each
name: a new event is compared the moment it is declared, so that comparison is no second
pair of eyes on that registry.

- **A comparison is code**: comments are stripped before the match, and a `//` inside a
  string is not a comment.
- **Naming a value is not comparing it**: the unit is the assertion, and it must **relate
  two things** — `assert_eq!(generated.k, entry.k)`,
  `severity_matches(entry.severity, generated.severity())` and
  `entry.code.starts_with(…generated.area()…)` do, `!x.is_empty()` does not, and a lookup
  that panics counts, since half the comparisons find the generated entry *by* the value.
- **One of the two is sometimes not the registry**: `commands.toml`'s `action` names a
  Cedar action, so `check` takes the names from the *parsed* schema through
  `nc_policy::Pack::actions`, never from the schema file's text.

### A declared command is one something implements

An entry in `catalog/commands.toml` is a promise to a dispatcher and to an AI agent. Every
command it declares carries a test, in the crate that implements it, comparing what the
code takes with `Command::arguments` — emitted from the same parse the schema is validated
with — in **both** directions: `nc-prune` and `nc-journal` for what the pruner and the
journal take, `crates/nc-kernel/src/builtin.rs` and `history.rs` for what the kernel's
handlers read. The decision point dispatches each (`NOCTUA.md §11.4`), and
`every_command_the_catalog_gives_the_kernel_is_one_it_runs_and_it_runs_no_other` holds the
catalog's commands for the kernel and the ones it runs to each other.

### A service that is running is not a service that is serving

The log store's image has no shell, `curl` or `wget`, so its compose healthcheck
(`/victoria-logs-prod --help`) exits 0 while the process lives, whatever the server does:
a **liveness** check, and its comment says so. `cargo xtask dev` asks from outside —
`GET /health` — and prints the store's URL only on a 2xx; its tests cover a store
answering 503, a port with nothing behind it, a body that merely begins with a number, and
the positive control (`grep -c '#\[test\]' xtask/src/dev.rs` counts the tests; nothing
counts the cases, so they are named here). **A `200` is not a receipt**: `check` posts one
tagged line to the running store and reads it back through `/select/logsql/query`, and the
store's reserved field names — the whole `_` prefix, `_time` being a record's own
timestamp — are refused by the rule whose subject they are, not left to a lint. **Serving
is not being bounded as declared**: an override file compose reads unasked, a `-f`, or a
container from before an edit can put a third number between `catalog/limits.toml` and
`docker-compose.yml`, so `check` compares the catalog with what the store answers on
`GET /flags`; and because `initdb` reads `POSTGRES_INITDB_ARGS` only when the data
directory is empty, it asks `pg_database` how the database sorts and encodes. Both are
among what `--offline` skips.

### A sentence is shown rendered or not at all

`nc_catalog::i18n::message` returns a key's sentence braces and all, so `render` is the
call: it refuses (`Err(name)`) a placeholder no value was supplied for, because a
half-rendered sentence is broken to a reader and silently fine to the caller
(`NOCTUA.md §9.9`). Every registry entry declares its placeholders — `ErrorCode::params`,
`Notification::params`, `Email::params` — and the compiler refuses one that disagrees with
either locale's sentence, both ways;
`every_declared_key_renders_with_nothing_left_in_braces` drives every key through both
locales. `field` is deliberately the caller's — a localised label, not a column — so
`FlakeOutOfRange::bounds` supplies only `min` and `max`.

### A citation names a real section, in every document that binds

`xtask/src/sections.rs` reads `NOCTUA.md`, `NOCTUA-PLAN.md` and
`reference/identity-and-authorization.md` for the sections that exist, and judges their
citations as well as this repository's. A numbered item under a heading is a section a
citation can name — `§14.9.7` is the seventh invariant of `### 14.9` — read with its
emphasis stripped. `#` is a heading in Markdown and a comment in TOML, YAML and shell, so
a `§` in such a comment is judged
(`a_hash_hides_a_citation_in_markdown_and_nowhere_else`). Paths in the workspace documents
are deliberately not checked: a `§` means the same thing wherever it is written and a path
in documents about several repositories does not, so that check is the workspace's to
write.

### Generated output cannot depend on the day it was generated

The two in-process generations run milliseconds apart, so a day-granular stamp is
identical in both. So `xtask/src/determinism.rs` reads the emitting crates,
`nc-catalog-compiler` and `nc-canon`, for a clock, a random number, an unordered map, the
environment or the process's identity, and reports the line; a safe one carries
`allow-nondeterminism:` with its reason (`grep -rn allow-nondeterminism: crates/ xtask/`
lists them). **A marker is a claim that needs a test**: the `read_dir` of `catalog/i18n/`
is marked as sorted by its caller, and
`crates/nc-catalog-compiler/tests/the_order_on_disk_stays_on_disk.rs` holds that by laying
the catalog out in two orders on tmpfs — after asserting the two orders differ — since
tmpfs and ZFS return different orders and no in-process comparison can see it. CI's
determinism job builds the second generation in a second directory, copied in reverse
order.

### CI's one gate reaches every file CI can reach

`ci.yml` runs the gate, an install and two builds, and no second gate: every cargo
invocation CI can reach must be `cargo xtask <verb>` or the `cargo install` the gate
needs, however a line writes it — through a script a step runs, up to `HOPS` scripts deep
in `xtask/src/workflow.rs` and a finding past that; through a `working-directory`, an
alias, a composite action or an action whose job is to run cargo; and in the justfile. It
reads the words a line writes: a word a shell builds from parts or splits from a longer
one, and a value a command reads from a file as it runs, are not read ("What is
deliberately not claimed").

- **A file a developer or CI can run is read because it is there**, not because something
  names it: a `just` module (`mod extra` loads `extra.just`), a committed `Makefile`, and
  `GNUmakefile`, which GNU make reads first.
- **A word in a workflow is not always a path**: `$GITHUB_WORKSPACE` is expanded, a glob
  such as `ci/*.sh` is matched against the tree, and the tools that judge this tree are
  needles whatever binary they are — `rustfmt --check` as well as `cargo fmt`.
- **A step's local action and its directory are read wherever they are written.** A local
  action is walked wherever the pins read its `uses` key: after a `-` with any whitespace
  after it, quoted, escaped, or in a flow mapping. A script is looked for in the value of
  every `working-directory` key, wherever YAML opens one or on the line below it, and in
  the word after every `cd` and `pushd`, wherever on a line it stands. Read where a line
  opened on them, each of those spellings left a second gate in the action or the script
  unread.
- **A name for `cargo` is read at every `=` whose name is a word of its own**, whatever
  the line opens with and whatever follows the value: after a wide `-`, in a quoted or a
  flow-mapping `run`, behind another assignment, or before a comment. An append (`C+=`),
  an element (`C[0]=`) and every element of an array (`C=(x cargo)`, `([a]=cargo)`) are
  read as `=` is, and `<<<cargo` as `<<< cargo` is. Where GitHub reads a line as YAML, a double-quoted value of one line is read
  with its escapes resolved, which is the text a shell is handed, so `\x63argo` is
  `cargo`, and with what follows its closing quote kept, which the shell's own reading of
  a comment then judges. A script's line, and a block scalar's, is read as it is written,
  because a shell resolves none of those escapes: resolved, `echo :"\x20#"#; cargo clippy
  --workspace` held a ` #` that ended the line before the command a shell runs after it.
- **A `name:` is a label only where GitHub reads it as YAML**: on a line of a workflow or
  of a composite action's file that is no block scalar's text, wherever the scalar's
  header stands: on its key's line or below it, behind an anchor or a tag or not. In a
  block scalar, a script, the justfile or a Makefile, a line opening on `name:` is a
  command, which a shell runs with what follows it once `name:` is a function or a
  hashed path: `name:() { "$@"; }` or `hash -p /usr/bin/env name:`, then `name: cargo
  clippy --workspace`, ran a second lints gate there while a label holding no separator
  was skipped. What a job must run still passes over every label.
- **And no step runs a file GitHub reads as YAML**: a word that names a workflow or a
  composite action's file where a step finds a script, through the checkout root or an
  escape as well, is refused, because a shell running it reads every line as a command,
  and its labels with them: `bash ci/tools/action.yml` ran `name: cargo clippy
  --workspace` past every reader here.
- **`.github/` holds `ci.yml` and `release.yml` alone**, every action either
  uses is pinned to a commit, every job that builds installs the toolchain with
  `rustup toolchain install` and no toolchain named, so what it installs is what
  `rust-toolchain.toml` says, no file GitHub runs but `release.yml` mints the release
  App's token, enters an environment, or reads a secret or a step's output, and no file
  CI reaches but `release.yml` publishes or stands the release's gate aside ("The release
  job is what `release.yml` says, and the App's token reaches its last step alone").
- **Every `cargo install` is one crate, locked, at an exact version**,
  `cargo install --locked name@X.Y.Z` and no other word, in every file CI reaches, read
  as a command is however its line is written: a crate named with no version, or with a
  range such as `@0.20`, is whatever crates.io holds on the day, and its code runs in the
  job that installs it, the release's gate among them; a second crate on the line is one
  no version names, and `--git`, `--path`, `--registry` or `--index` takes the code from
  where no version on crates.io describes it. `cargo-deny`'s version is written in six
  places and moved in one commit: the three installs of `ci.yml`, the gate job of
  `release.yml`, `THE_RELEASE_STEPS` in `xtask/src/workflow.rs`, which holds that line
  for line, and that line's row in `WHAT_ELSE_THIS_TOOL_SAYS` in `xtask/src/check.rs`,
  the census of the sentences the tool ships. The rule's test takes its install from
  `THE_RELEASE_STEPS` and fails when `ci.yml` holds no such line, as the release's own rule
  fails when `release.yml`'s gate does not; the three lines of `ci.yml` are not held to one
  another. `deny.toml`'s comment on `unsound` names the version whose default it read, and
  is read again against the new one.
- **Actions are an allowlist, not a needle**: an action judging anything of ours is a
  second gate whatever it is called — a linting action such as
  `giraffate/clippy-action@v1` <!-- allow-reference: names `giraffate/clippy-action@v1` — an action this repository does not use -->
  is one — so only the steps that set the run up and decide nothing are allowed, each with
  its reason. Every `uses` key of a line is read, wherever YAML opens one, and an action
  the reader cannot read on its line is one off the list.
- **`ci.yml` fires on every push, and on every pull request as it is opened, pushed to,
  reopened or edited**: `types: [opened, synchronize, reopened, edited]` is the one line
  under `pull_request:`, and it is required. That list widens the events, never narrows
  them: without `edited`, a title or a body changed after the last run starts no run, and
  the merge commit takes what the history step never judged. Any other line under either
  event is a filter, and is refused.
- The gate refuses a workflow that fires on nothing and a step whose verdict does not
  count, and checks that each job still runs what its name says: the determinism job
  builds **twice**, in the order that makes the second generation a second process. A
  condition in either workflow, and a step's own `shell` and `continue-on-error` in any
  file GitHub runs, are read as keys wherever YAML opens one — quoted, escaped, after an
  anchor or a tag, in a flow mapping, and a condition written on the lines below its key —
  as well as by the words a line spells.

### A compile-fail fixture proves what rustc refused, not what it wrote

A gate asking whether the build's output *holds a string* is satisfied by the fixture's
own words echoed back — a `fn accepts<T: LogValue>` line, an `EXPECT:` written into the
failing line, a `compile_error!("…")`. What is read is the diagnostic minus its `rendered`
block, its spans' `text` and its `file_name`, and a sentence the fixture's own code holds,
its `//!` header stripped, is not rustc's. An error code is not required: `literal!`
refuses a non-literal with `no rules expected`, which carries none. **The fixtures prove
what rustc refuses today**, not a constructor added tomorrow; `xtask/src/wall.rs` holds
that half — `no_public_constructor_takes_a_value` reads `nc-log` for a public associated
function on `Literal` or `Variant` that takes a parameter, and
`nothing_manufactures_a_static_string` sweeps every other crate, but the generated
`nc-catalog` sources and the fixtures, for a function handing back a `&'static str` it was
not given.

### An escape hatch is a comment in the language the file is written in

Every gate has a same-line hatch demanding a written reason — `allow-reference:`,
`allow-logging:`, `allow-nondeterminism:`, `allow-count:`, `nc:bounded:` in a migration,
`nc:narrows:` in a Cedar policy — and **the marker counts only inside a comment of the
file's own language**. `#` opens a comment in a manifest, a workflow, a shell script and a
justfile, not in Rust (`r#"…"#`, `#[derive]`, `r#type`) nor in Markdown (`## x`, `#42`),
where only an HTML comment is one; `--` is SQL's alone. A marker in a string literal
excuses nothing — `wall.rs` builds its own needles with `concat!` for that reason — and a
character literal such as `'"'` opens no string.

### An escape hatch is a comment, and using one does not lower a count

**An excused line is still a line the gate read**: findings are what it judged and its
count what it read, so a hatch never lowers the number that proves the gate ran. The
census of every marker has a floor of its own — `0 line(s) carry a gate's escape hatch`
means it stopped matching — and `EXCUSED_WHERE` in `xtask/src/check.rs` pins how many of
each marker each file carries, this one included, so adding or removing a hatch is a
two-place edit whose second place is that list.

### Prose in a generated file is output, and is pinned

Every value a `docs/*.md` page interpolates is compared with its registry, and every
sentence the emitter *authors* — a page's opening, its section leads, the header of
`catalog/errors.reserved.toml` — with a written list in
`xtask/tests/the_pages_reach_the_catalog.rs`, because nothing derives it. Changing that
prose is a two-place edit on purpose.

### The generated crate is read by a consumer, not by a reviewer

What a doc comment on a generated type says is what the author of a consumer believes, and
the page beside it is read by nobody who is writing code, so a claim that appears in both
has to agree in both: `no_mutating_command_claims_more_undo_than_the_kernel_has` reads the
crate as well as the page. Every enum answers `as_str()` with the word the registry
writes, so a consumer never needs a second vocabulary; the pages test keeps its own list
on purpose, because a comparison relates two things and one of them has to be free of the
emitter.

### CI runs the stack, not a copy of it

The `linux` job declares no `services:`: it runs `cargo xtask dev`, the stack a developer
runs, with the log store's retention flags, the health command that authenticates and the
migrations applied. A service container cannot carry a command, so its store would report
no flags, and nothing would apply the migrations. `ci_supplies_what_the_gate_needs`
refuses an image named in the workflow and requires `cargo xtask dev` before
`cargo xtask check`, reading the job's commands rather than its prose. The `macos` and
`windows` jobs run the gate with no stack, which says which of its steps are not applicable
there, and `no_stack_where_none_runs` refuses `cargo xtask dev` in either; every job named
for a system runs on that system's runner (`RUNS_ON` in `xtask/src/workflow.rs`).

### An empty database fails the gate, and `--offline` is how to ask

A database that answers and has applied no migration is a failure, not a skip: the
migration corroboration, the `pg_temp` resolution and the journal's protections each say
so and name `cargo xtask dev`. A database *behind* `migrations/` fails too — the record is
read both ways. `--offline` is the one way to ask for those steps not to run; it announces
each skip, and CI never passes it.

### The log's type wall is scoped by the module tree

`Literal`'s field is private to `nc-log`, so "no public function hands one out" covers
every module of that crate, a file another crate's directory holds and `#[path]` compiles
in included. Scope comes from the resolved module tree, never a path prefix, and its
reader matches `path =` anywhere on a line: `#[cfg_attr(all(), path = "…")]` is the same
attribute.

### A registry value is judged as the thing it becomes

A log event's field name is judged as the struct field — snake-cased, so `max-rows` and
`max_rows` collide, and lower-case, since clippy runs with `-D warnings` — a locale tag as
the enum variant, an event name as the type, against the prelude and the generated file's
imports. Prose is judged as what it lands in: `doc()` escapes what rustdoc reads as
markup, `cell()` what a Markdown table *and* an HTML renderer do, `quoted` writes Rust and
`as_toml` TOML — one escaper per output language, none shared.

### A test that creates a cluster-global object sweeps every one, not its own

A role outlives the process that made it, and the protection row's `writeall=` and
`owners=` are cluster-wide, so one left by a red run fails `check` for good while naming
migrations, and a teardown keyed to the running pid never clears an older leak. The sweep
is `LIKE 'nc_writers_probe_%'`, run before the first measurement.

### The excuse and the danger are the same thing

`NOT_HASHED_DIRECTLY` names the `Entry` fields the hash may discard, and adding one is a
single token. A field is dangerous because the store persists it, so the store's own
`INSERT` is the second reader: a column it names must be under the digest, and no excuse
can buy one.

### A door is watched by moving it

`writers=`, `colwriters=`, `writeall=`, `owners=` and `superusers=` all read `0` on a
correct database, so a clause that measures nothing — one misspelt role name — reads like
one that does. Each door is granted, in a scratch schema to a role named for the process,
and its number must move; `superusers=` cannot be given a control safely and has a second
reader instead.

### A test that needs a database reads `NC_NO_STACK`, not a port probe

`url()` falls back to a default and probes the port, so a test guarded by it would connect
under `--offline` and run its DDL against the developer's own stack. The flag is the only
thing that means "do not touch a database", and no database is a failure here, not a skip:
`eprintln!` is captured, so a skipped control is one nobody saw not run. The gate sets it
under `--offline` and on a system that runs no stack, and its value is the reason —
`--offline`, or `not applicable on <system>` — so a suite says in its own output which.

### Not applicable is a property of the system, never of the machine

`RUNS_THE_STACK` in `xtask/src/check.rs` asks the target `xtask` was compiled for, and
nothing else in this tool decides by which system it is on (`NOCTUA.md §2.7`): `ui::SYSTEM`
names the system in what the gate prints, and which suites stand aside is
`nc_platform::system`'s answer. That is the
security answer for this kind of line: a probe for docker would make a Linux machine with
its stack down read as a system the stack does not apply to, and every step that needs it
would stand aside there quietly — so the answer is fixed when the tool is built, a flag
cannot ask for it, and `finish` refuses any not-applicable line on Linux and, elsewhere,
any list but `THE_STACKS_PARTS`, in order. A part added to the stack's steps is a row
there and a sentence at its call site, which `every_part_of_the_stack_says_why_it_stands_aside`
holds to each other.

**A suite that needs what a system lacks asks `nc_platform::system`, and the gate names
it.** The system's answer is a constant of that crate, fixed per system as
`RUNS_THE_STACK` is — `LISTED_AS_MADE`, a directory listed in the order its entries were
made, and `A_POSIX_SHELL` — never a probe of the machine, so a Linux machine cannot stand a
suite aside, and `linux_lacks_nothing_a_suite_stands_aside_for` holds every answer on every
system. That module is compiled in every build, not behind `test-support`, because the
gate reads it too. Each guard prints `skipped, not applicable on <system>:` and why — and
cargo counts the test as passed and captures that line with the rest of a passing test's
output, which the gate prints none of. So the gate's test step says it instead, from the
same constants: before the tests, a `not applicable` line for each suite this system stands
aside (`THE_SUITES_A_SYSTEM_STANDS_ASIDE`); after them, `the ones that need what this
system lacks` on the count line; and beside the verdict, `not applicable here: <suite>`.
`finish` holds those lines to the list as it holds the stack's parts, so on macOS (the
order on disk) and on Windows (that and the checkout script's guards) a suite that stopped
saying so is a refusal, and
`the_suites_a_system_stands_aside_are_the_ones_written_down_for_it` holds the list per
system, empty on Linux. `ANNOUNCES_WHAT_THE_SYSTEM_LACKS` in `xtask/src/check.rs` counts
the guards into the same line as the database suites, read by the same rule — a count of
the guards in the sources, the same on every system — with
`every_source_that_asks_what_the_system_lacks_is_one_the_census_names` naming every source
outside `nc-platform` whose code names a door. It looks for the door by its own name, as a
whole word, so a suite that reaches it through `use nc_platform::system;`, or under
another name, is found as one writing the whole path is, and
`a_door_is_asked_however_it_was_brought_into_scope` hands that reader the spellings the
tree does not hold.

### Two tallies are not a pairing

A `diff` is its operands: CI's determinism job is checked invocation by invocation — left
the second checkout, right this one — never by counting `diff` lines against
`$GITHUB_WORKSPACE` occurrences anywhere, which a comment can pay.

### A ratchet is baselined from a tree, and the tree may already be wrong

A pin's first numbers are *measured*, and what they measure may be the defect: a doc
ratchet baselined where one item's documentation sat on another asserts the misplacement,
and the repair then fails it. Read what a baseline pins before writing it.

### A count catches a merge; the second number catches an insertion

Two doc blocks merging lower a count; an item *inserted* between a block and its item
conserves it — the newcomer gains the block, the original loses it — so each file pins two
numbers, blocks and items with none, and the pair is what moves.

### A field added to a hashed struct is a field outside the hash

`Entry`'s `hashed_form` destructures `Self` with no `..`, so a new field is a compile
error at the one site that must know, and `every_field_of_an_entry_is_hashed_or_named`
says which fields it may discard. A field outside the hash is editable with no digest
moving — Stage 10's criterion 3 false for that field.

### A whitelist of files cannot hold a rule; the behaviour can

`the_two_xx_rule_is_written_twice_and_no_more` limits the *spelling* of a 2xx test to two
files, and a classifier written another way matches no spelling. So
`the_health_check_answers_the_shared_table` stands a listener up for every row of
`xtask/canaries/status-lines.tsv` and requires the health check to agree: a third copy may
then exist and cannot disagree.

### A diagnostic's operands are absolute

Each side of CI's determinism `diff` names `$GITHUB_WORKSPACE` or an absolute scratch
path, never a relative one resting on a `cd -` no test reads, and the gate counts anchored
operands against `diff` invocations.

### The reader and the writer are one rule

`release`'s writer asks its reader where the version line is, so a spelling cargo accepts
and `cargo fmt` leaves alone — `version  = "1.2.3"` — cannot be readable and unwritable at
once.

### A measurement whose only value is the passing one is untested

`writers=0` alone reads `pg_class.relacl`, while a column grant lands in
`pg_attribute.attacl` and `pg_write_all_data` is a membership, so the row carries
`colwriters=`, `writeall=` and `superusers=` too, and
`a_grant_this_query_is_written_for_moves_its_own_number` grants each shape in a scratch
schema and requires its number to move.

### A repair is a claim, and a claim needs a case

A fix written to close a measured defect arrives, in the same edit, with the test that
would notice it undone, planted and measured like any rule; a one-line repair that reverts
with the gate green is held by nothing (`NOCTUA.md §19.5`).

### Two strings that differ in length are two coordinate systems

`without_string_contents` preserves the count of characters, not bytes. Blanking,
stripping and normalising each make their own coordinates, and a scan picks one and stays
in it: an offset from the source used on the blanked copy reads a different place.

### A pin over a set enumerates the set

A constant standing for "the rules we have" is checked against what the tool accepts, not
the subset somebody remembered: `[workspace.lints]` has the tables cargo defines, and a
pin reading only those the manifest uses lets another turn off the gate that reads
documentation.

### A test cleans up on the way out

A test removes its scratch on the way out, on the failing path too, never only before
writing it — which leaves one behind on every run with nothing counting them (invariant
16).

### A floor is calibrated against what the run in front of it can see

A constant measured in one environment and asserted in another passes only on the machine
that wrote it — a paragraph floor written against a workspace fails a clone of this
repository alone, which is what CI has. Where a number depends on the tree, derive it from
the tree: the documents opened, the members cargo lists, the files the walk read.

### A manifest is TOML and a source file is Rust

Where a parser exists in this tree — `toml` is a dependency, `gates::walk` the one walk —
the gate uses it: a line scanner over a manifest accepts `[lints]` and `workspace = true`
inside a description string, and a scanner over source that knows a handful of keywords
misses `pub type`, `pub async fn`, `unsafe fn`, `mod` and `macro_rules!`.

### `#[doc(hidden)]` is outside the documentation gate

rustdoc documents a hidden item only when asked, through a nightly flag this repository
does not take, so a broken intra-doc link on one is silence — and every test door is
hidden. Their prose names things in backticks, never brackets;
`grep -rn '_for_tests' crates` names the doors, `cargo doc --no-deps` writes a page for
none, and a `pub use` of a hidden item stays hidden. `xtask` is a binary, where private
items are documented anyway, so this is about the library crates. **Nothing rendered links
to a hidden item** — the link resolves, so rustdoc is silent and renders text with no page
behind it — and `no_visible_prose_links_to_a_hidden_item` in `xtask/src/check.rs` refuses
such a link; a hidden thing is named in backticks, with where to find it said in words.

### An `# Errors` section names the refusals the function actually returns

The variants that go missing from one are those that invert `retryable`, and a caller
written against it retries a defect it caused. A variant built in the function's own body
is held to its section by a test; one reached through a call is written by hand and
re-read whenever the callee changes.

### Two halves of one rule read the same text

When a rule asks two questions about one piece of source, both ask them of the same
string, extracted once into a named function — otherwise an attribute in front of a
declaration can make it public to one half and invisible to the other.

### A test that reads a process-global counter takes the hold

`ui`'s warning and skip counters are statics and `cargo test` runs in parallel, so "the
number this test added" holds only while nothing else prints. The printers take the hold
themselves, and a source assertion requires each to: a lock a test must remember is one
the next test breaks.

### A doc comment attaches to the next item

An item inserted under a doc block takes it, and `missing_docs` cannot see that, since
almost nothing in `xtask` is public: insert a new item above its neighbour's doc block,
never between the block and its item. `UNDOCUMENTED` in `check.rs` is the set of items
with no doc today, an equality: it cannot grow, and documenting one deletes its line in
the same edit.

### A shared reader is what makes two counters worth having

Two counts of the same ground catch a filter in one and miss a blind spot both share — two
parsers each needing a space between `CREATE` and `FUNCTION` miss the same statement and
agree. So both go through one reader, and the reader gets the test.

### A claim about what CI runs is checked against the workflow

`WHAT_CI_RUNS` in `xtask/src/check.rs` names the cargo commands CI runs — the gate is not
the only one, and the one most often left out brings up the stack the gate needs — and a
test holds the workflow and every sentence listing them, here, in `docs/README.md`, in the
`justfile` and in the tool's source, to it both ways.

### A rule written twice is written once, or held together by a file

A sentence claiming two pieces of code agree is not a mechanism: either one function, or a
table both are driven over — `xtask/canaries/status-lines.tsv`, since `xtask` does not
depend on `nc-log`. A list is the same: a constant and a literal of one set, or one
argument list in two functions, are two lists.

### Coverage is not proof

A gate asking whether a test exists is not one asking whether it proves something: a
denial case whose `because` names another policy satisfies coverage and proves nothing of
its own. When a check declines to report because "another gate covers that", ask whether
both ask the same question; if not, the decline is a hole.

### A test never releases a port and then expects it to be dead

Between dropping a listener bound to `:0` and connecting to its port, any parallel test
may take it. Connect to port `0` instead: the kernel refuses it every time.

### The assertion a suite's verdict comes out of is itself tested

`xtask/src/corpus.rs` decides every mutation-coverage tally through one `assert!`, so any
function that turns findings into a pass or a fail — there or in any gate — is handed, by
a test, a case that decided wrongly: a verdict nobody has watched refuse is not known to
be wired up.

### `let _ =` on a call is a dead function the lint cannot see

`-D dead-code` counts a call, not a use, so a helper whose answer is discarded can have
its predicate inverted with nothing going red. Either the answer decides something — an
assertion, a branch, a finding — or the helper goes.

## Known gaps in the tree's own instruments

These are the gates' and `xtask/mutants.py`'s own limits, not the platform's, so no **Not
answered** holds them. Each says what would close it.

- **`xtask/mutants.py` restores a source by writing it in place**, truncating first, so a
  disk that fills during `restore()` leaves that source empty and those after it
  unrestored; the in-flight marker (`SENTINEL`) and the pristine copies under `BACKUPS`
  make that recoverable, and a run is long on a shared disk, so check free space first.
  *Closes with* each source written beside its backup and renamed over the original, with
  a test of its own.
- **Its verdicts reach a file only when it exits**: the per-operator line is not flushed,
  so with output in a file a killed run loses every verdict reached — the in-flight marker
  recovers sources, not the tally. Run it as `python3 -u xtask/mutants.py`. *Closes with*
  `flush=True` on that line.
- **It needs `DATABASE_URL` exported**: `cargo xtask` derives the address when the
  variable is unset (`default_url` in `xtask/src/database.rs`) and the runner does not,
  since its port is the checkout's and a second derivation would be a second naming
  function — export what `cargo xtask dev` prints. *Closes with* the runner asking the
  gate for the url rather than deriving it.
- **A kill is a suite that went red, not a test that did**: no test is recorded per
  operator, so a suite that starts failing mid-run is a kill for every operator after it,
  and only the closing control pass sees that, refusing the whole run. *Closes with* each
  kill's failing tests recorded, and a kill made only by tests the control finds failing
  refused.
- **A mutation run leaves scratch directories behind** — invariant 16's gap.
- **Two guards are held by no operator.** The `AlreadyExists` arm of `private_directory`
  in `crates/nc-local/src/open.rs` is reached only by
  `a_fresh_profile_opened_from_several_threads_at_once_opens_every_time`, which meets that
  race by chance, so an operator on it would be judged by a coin; and the call of
  `unset_what_proptest_reads` in `cargo_with` (`xtask/src/check.rs`) is not, because a
  test cannot set its own process's environment under `forbid(unsafe_code)` in edition
  2024 (the helper itself has an operator). *Closes with* a way for a test to make the
  directory between the look and the create, and a test running the gate's cargo from a
  child given a `PROPTEST_` variable.
- **No test tells apart the order in which an undo counts its own notes and runs its
  compensations** (`crates/nc-kernel/src/history.rs`): the count comes first, so a
  compensation that would note past what the undo's unit may keep is refused as it runs.
  Telling the orders apart needs a step at `store_notes`' bound, whose undo outlasts a
  debug build's tree deadline; the notes kept free and the count are held by unit tests
  and operators. *Closes with* that bound lowered for one test.
- **The plugins step says more than it counts**: its line reads *built for wasm32,
  componentized, linted and documented* over the components built, and that `clippy` and
  `doc` ran over each plugin is held by the arguments `plugin_commands` in
  `xtask/src/plugins.rs` builds, not by anything cargo reported. *Closes with* the lint
  and doc units cargo's JSON names for each plugin, counted and floored by equality.
- **The doc-block rearrangement census is paid, not closed**: the constants beside
  `THE_REARRANGEMENTS_THE_FIRST_LINE_RULE_ACCEPTED` in `xtask/src/check.rs` accept
  rearrangements the doc-block rules cannot tell from a block as written. *Closes
  with* each such block rewritten as a summary, a blank line and the rest.
- **A fixture's plantings are floored, not complete**: the reader inside
  `reached_every_one` refuses a fixture that planted almost nothing this tree wears, not
  one that planted a handful; the `.env` walk and the determinism walk assert in shapes of
  their own and never reach it; and `the_names_this_tree_wears` reads the tree through
  `sections::READ`, so a kind named only by the canary walk's own lists would be planted
  under no name — the two agree today, and the coupling is written on the function.
- **`just checkout-shape` runs with this machine's cargo home**: a crate that cannot be
  fetched is invisible to it, and a missing `cargo-deny` aborts it under `set -e` before
  its equality. The toolchain is not part of this — rustup keeps it under `RUSTUP_HOME`,
  which the recipe does not touch. *Closes with* the recipe run once with an empty
  `CARGO_HOME` and no `cargo-deny` on the path.

## Dependencies

`NOCTUA.md §2.3` keeps this list here rather than only in a manifest, because the rule is
about *asking* before adding, and the conversation needs somewhere to have happened. Every
entry is already a dependency of `noctua-colors` or `noctua-design`, or named by a closed
decision — `NOCTUA.md §4` is where `wit-component`, `wit-parser` and `ureq` were decided —
and none was chosen by preference. `ureq-proto` and `url` are in the graph under `ureq`
and `sqlx`, and are named because the kernel matches on a type of each.

| Crate | Why it is not written here |
|---|---|
| `cedar-policy` | The authorization engine, `NOCTUA.md §4` [CLOSED]. `nc-policy` validates the pack with it and `nc-authz` decides with it, and a pack validated by one Cedar and decided by another is validated against a second implementation of its semantics. Cargo already resolves compatible requirements to one version (`cargo tree -i cedar-policy`), so the one `[workspace.dependencies]` line is not what unifies them: it is the one place the requirement is written, so raising it moves both, and it keeps either member from moving to another major version — a second Cedar beside the first — or to an exact `=` pin that silently decides the other's version |
| `sqlx` | Driver, pool and migration runner in one. `macros` is **on**, because `sqlx::migrate!` needs it and needs no database; what is refused is the `query!` family, which checks SQL against a live database *while compiling* — and `gates::no_compile_time_query` fails the build on one, in every Rust file in the tree. A feature list cannot say "this macro and not that one", so the rule is a source gate. From 0.9, text built at run time reaches `query`, `query_as`, `query_scalar`, `raw_sql` or an `Executor` only inside `sqlx::AssertSqlSafe(..)`, a claim the compiler takes on trust, so `gates::ASSERTED_STATEMENTS` names each file that makes one, how many, and every value their text is built from — a new wrap, or an old one built from a new value, is a finding until somebody has read where the value comes from. What stays outside what the compiler refuses is listed on `gates::Asserted` with how each is held — a reading of `sqlx`'s signatures, not a proof there is no other: `QueryBuilder`, whose `new` and `push` wrap whatever they are handed inside `sqlx` and which the same walk therefore refuses by name; `copy_in_raw` and `copy_out_raw`, which take a whole `COPY` statement as a plain `&str` and are read the way a wrap is, so a `COPY` built from values is counted in its file's row and one written as a literal needs none; `sqlx-sqlite`'s `SqliteConnectOptions::pragma`, whose key and value are any `Into<Cow<'static, str>>` written unquoted into a batch it runs on every connection, read the same way, so a pragma set from values is counted and one set from literals — every one this tree writes — needs no row; a migrator built when the store runs — `Migrator::new` over a path, `with_migrations`, a `MigrationSource`, `ResolveWith`, `resolve_blocking` — which reads its `.sql` then and hands the text to the database vouched for inside `sqlx`, refused by name in the sources `WRITERS` in `xtask/src/local.rs` names, where `sqlx::migrate!("../../local-migrations")`, embedded when the crate compiles, is the one migrator (`a_store_migrates_with_what_the_build_embedded_wherever_it_runs` opens a store from another working directory); and, held by nothing, a `String` leaked into a `&'static str` and the migrator's own table and schema names, which no migrator here sets. **SQLite, bundled, for the local database** (`NOCTUA.md §4`): `crates/nc-local`, `nc-prune`'s second target and `xtask`'s schema gate each take the `sqlite-bundled` feature — not `sqlite`, which in 0.9 also compiles extension loading, `deserialize` and unlock notification — and the workspace line stays without it. Which SQLite is linked is `Cargo.lock`'s choice, not a manifest's: `sqlx-sqlite` accepts `libsqlite3-sys` `>=0.30.1, <0.38`, `cargo update -p sqlx` alone keeps an old one, and `cargo tree -i libsqlite3-sys` says which; `nc-local`'s self-check at every open and the local schema gate in `check` refuse one below 3.50.2 |
| `tokio` | Arrives with `sqlx`, and drives the kernel and its tests — and `nc-local`, for its blocking pool alone, where an open waits for its profile's lock; `nc-platform`, for the daemon's socket and named pipe (`net`), because `create_with_security_attributes_raw`, the one call that makes a pipe with a descriptor of its own, is `unsafe` and that crate is where `unsafe` may be written; and `nc-ipc`, for the frames' reads and writes, the handshake's deadline, the heartbeat and the two tasks a connection runs on — a direct dependency there, already in the graph so taking it adds nothing to the lockfile, and still the developer's to confirm (`§2.3`): `wasmtime` instantiates and calls a plugin asynchronously, so that a host function can await the journal, the directory and the store, which are asynchronous themselves. The kernel takes `sync`, for the journal and the bus a call tree holds across an await, and `time`, for the deadlines the directory and every host call a plugin makes are held to — and runs a nested dispatch as a task of its own, off the plugin's stack |
| `wasmtime` | The plugin runtime, `NOCTUA.md §4` [CLOSED]: the Component Model, fuel and epoch interruption. Every default feature is off, and the feature list beside the `wasmtime` line in `Cargo.toml` names the ones the kernel runs, with what was left off and why — `call-hook` among them, which is how the epoch counts a plugin's own running and not the host's — and `cache` above all of what is off, whose reading half is `Component::deserialize`, which is `unsafe`, so the kernel compiles a component on every load |
| `wit-bindgen` | The guest half of the same world (`§4`: the Component Model "with `wit-bindgen`"), in `crates/nc-guest` only, `macros` only. Its `build.rs` links two prebuilt archives into every `wasm32` build — `deny.toml` says which and why its build-script rule does not see them |
| `wit-component` | What turns a plugin's core module into a component, inside `cargo xtask` rather than as a tool somebody installs, and what reads the component back. The encoder refuses only an import no embedded world describes, and a dependency with bindings of its own embeds one — `wasip2`'s `wasi:random` encoded without a word — so `xtask/src/plugins.rs` decodes each component and refuses any import the world `plugin` does not declare |
| `wit-parser` | The world as a parser reads it, for what a module and the kernel's generated bindings cannot say about the world they came from: its declared version, what the world `plugin` imports for `cargo xtask build --plugins` to hold a component to, and every function it holds for the capability census |
| `ureq` | The HTTPS client behind `net.fetch`, decided on 2026-09-28 — real HTTPS, with every rule about what may be asked the kernel's and the client only carrying a request already judged. Blocking, so the kernel runs it on the runtime's blocking pool under the host call's deadline; `rustls` alone, with `ring` and `webpki-roots` — Mozilla's root bundle compiled in, because a root store read from the platform reads `SSL_CERT_FILE` and `SSL_CERT_DIR` and the environment would choose whom the kernel trusts. `crates/nc-kernel/src/net/ureq.rs` configures it and gives it a resolver that dials only public addresses; its `Resolver` is outside ureq's semver promise, so the lockfile decides the version (`cargo tree -i ureq`) |
| `ureq-proto` | `ureq`'s own HTTP/1.1 protocol, already in the graph under it at the version `ureq` requires, so it adds no package (`cargo tree -i ureq-proto`). Taken directly only because the kernel matches on its `Error`, which `ureq::Error::Protocol` carries and `ureq` does not re-export: an answer with more headers than its parser holds is told as an answer too large (`[NC-PLUG-0014]`), not as a network that did not answer. A `ureq` moving to another release line of it carries an `Error` that match does not name, and the kernel fails to compile |
| `url` | The WHATWG parser a plugin's URL is held to before anything is sent, as a browser reads one, so every spelling of an address parses as an address the kernel refuses rather than as a name. Already in the graph under `sqlx`; taken directly because the kernel matches on its `Host` |
| `blake3` | The journal's chain and the machine flake both hash (`§14.9`, `§16.2`), a confirmation is bound to its dispatch's arguments by their digest, a blob is named by the hash of its bytes (`§13`), computed in `nc-kernel` as they are written, and each end of an IPC connection proves it holds the channel's token by a hash keyed by it, compared in constant time (`crates/nc-ipc`). Already `noctua-colors`' |
| `serde`, `serde_json`, `toml` | Reading the registries and writing canonical output, and reading a command's argument schema for `crates/nc-schema` to validate |
| `proptest` | `§16.3` and `§16.4` have algebraic laws, and `§19.2` puts those under property test |
| `clap`, `anstream`, `anstyle` | The command surface, in `xtask` only |
| `ed25519-dalek` | A device's key (`NOCTUA.md §16.2`): the Ed25519 public key a profile's flake and its journal's keeper derive from, and the key iroh takes as the device's identity in Stage 25 (`§13.2`), at the major iroh pins — a cryptographic primitive, which `§2.3` names as not viable in-house. `default-features = false` and `zeroize` alone: no `rand_core`, because the seed is drawn by `nc_platform::random`; no `fast`, because the one public key a start derives needs no precomputed tables. Its family arrives with it and adds nothing to write: `curve25519-dalek` (and `curve25519-dalek-derive`, its SIMD backend's macro), `ed25519` (the signature's type), `signature`, `fiat-crypto` (a backend compiled for some targets only), and `rustc_version`, which `curve25519-dalek`'s build script reads; `sha2` 0.11 and `subtle` are in the graph already. `cargo tree -i ed25519-dalek` says who takes it |
| `windows-sys` | The Win32 security calls of `crates/nc-platform`'s Windows half, and only there, compiled for a Windows target alone — decided on 2026-11-02: who the process token names (`OpenProcessToken`, `GetTokenInformation`), an access list built (`InitializeAcl`, `AddAccessAllowedAceEx`) and read (`GetSecurityInfo`, `GetAce`), a file and a directory created with a descriptor of their own (`CreateFileW`, `CreateDirectoryW`), the system's random bytes, which a device's seed and the daemon's token are drawn from (`BCryptGenRandom`), and the other end of a named pipe — its process from either end (`GetNamedPipeClientProcessId`, `GetNamedPipeServerProcessId`, feature `Win32_System_Pipes`), the user that process's token names (`OpenProcess`, `OpenProcessToken`), and the pipe's owner (`GetSecurityInfo` over a kernel object); and, behind the test doors alone, a descriptor written onto one path through its handle (`SetKernelObjectSecurity`), from its text when it holds a conditional entry (`ConvertStringSecurityDescriptorToSecurityDescriptorW`). Not written here because the declarations — every structure's layout and every function's signature — would be ours to get right, with no compiler to check them against the system's. Version 0.61, which `tokio`'s `mio` already brings, so it adds no package; its features are the modules those calls are in, pinned in that crate's manifest. It is the one dependency whose calls are `unsafe`, and invariant 17 says how that is held |
| `noctua-design` | The only cross-repository dependency this repository has: a `[workspace.dependencies]` path entry with the git-tag form in a comment above it (`NOCTUA.md §10.11`), reached by path while the two repositories are edited together. `noctua-colors` is reached **through** it and never directly (`§8.6`), which a gate holds |

**Three things are written here rather than taken**, all three a maintainer's call: the
HTTP client that ships log lines to a store on the same machine, the harness that proves a
dynamic string does not compile into a log event, and the argument-schema validator in
`crates/nc-schema` — a closed subset, so a keyword it does not implement is refused rather
than ignored (decided 2026-09-28).

**Adding to this list is a conversation, not a commit** (`NOCTUA.md §2.3`). If an in-house
solution is viable, stop and ask.

## Local notes

An
`AGENTS.local.md` <!-- allow-reference: names `AGENTS.local.md` — it may exist and usually does not; that is the convention -->
may exist beside this file: the developer's private, machine-specific notes. It is
gitignored. Read it if it is there.

## Maintaining this file

Update it **in the same change as the code**, unprompted (`NOCTUA.md §2.5`), in the shape
`NOCTUA.md §0.5` gives every document an agent is told to read.

**Record the command, not the value.** "Run `cargo xtask check`", never a count of tests:
a written count rots silently, and an agent trusting a stale one errs with confidence. The
written-count gate refuses a number directly before a noun this tree can count — crates,
invariants, migrations, registries, packs, compile-fail fixtures, integration test files —
unless its line carries the `allow-count:` marker with a reason.

**A finished piece of work leaves a rule, never an account of itself** (`NOCTUA.md §0.5`):

| What you have | Where it goes |
|---|---|
| A rule that still binds | this file, or the module header it governs |
| Evidence for an assertion | the comment beside that assertion |
| The story of the change | its commit message, and nowhere else |

A merge commit carries its pull request's title and body (`NOCTUA.md §22.1`), so the
story of a change goes there and in its commits, and never here. A rule keeps, in a
sentence or two, the reason a reader needs in order not to undo it; the measurement that
made it credible goes beside the code it justifies.

**Area-specific rules live in the module header**, not here, so they load when the file is
opened; only what binds everywhere belongs in this file. A heading, an invariant's number
or a sentence that code or another document cites keeps its words and its number — `grep`
for it before changing it.
