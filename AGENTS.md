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
  proves it holds the channel's token without sending it, one request at a time, a
  heartbeat, and a watch a client waits on, so the daemon never sends a frame nobody asked
  for; and the client every program that asks a daemon connects through
  (`nc_ipc::client`), the CLI's and the desktop interface's (`NOCTUA.md §11.1`).
- **The daemon a channel runs** (`apps/daemon`): the install read beside its executable,
  the profile it keeps, the device it mints identifiers on, the mark that keeps those
  identifiers unique across a crash and a clock moved back (`NOCTUA.md §16.2`), and the
  kernel it serves at its endpoint — one daemon to a profile, the plugins installed in it,
  and the pump and the blob sweep on the catalog's schedule (`NOCTUA.md §11.1`). Its
  binary, `noctua-daemon`, is the daemon, and answers `--initialize`, the installer's
  entry, and `--restored`, every restorer's, which honours an explicit stop.
- **The command line a channel is reached through** (`apps/cli`): a command line read
  into a command's arguments against the schema the daemon's registry lists, the
  daemon reached and proven before anything is sent, help from the registry, and one
  table of exit codes (`NOCTUA.md §20.2`); the channel's daemon started, stopped and
  asked how it is, a signal reaching only the process its PID record is held to; and the
  installer `cargo xtask dev --install` runs (`NOCTUA.md §17.1`). Its binary is `noctua`.
- **The desktop interface** (`apps/desktop`), being built in Stage 16: a window and the
  system's own webview in it, kept hidden until the state the daemon holds shows it and
  its document has loaded, that document painted in its theme from its first frame and
  every file it loads served from the binary through the `noctua` protocol, that
  state followed by a watch and read strictly, its footer — the modeline, the info line
  and the prompt line — drawn from it, and what the window asks sent as a command or a
  note: a control's command, and the prompt's text (`NOCTUA.md §12.1`, §12.3). Its binary
  is `noctua-desktop`, which answers `--probe`, asking with no daemon whether a system can
  host the interface.

It is one repository on purpose (`NOCTUA.md §10.1`): those parts share the command
catalog, the policy pack, the generated types and the journal format. `NOCTUA.md §10.2`
says which directories are absent until a stage fills them — every app but the daemon, the
CLI and the desktop interface, and the services, among them.

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
│   ├── commands.toml               every command the catalog declares — the kernel's own — with its action, its selector (§11) and its entry (§20.4)
│   ├── notifications.toml          typed notification handles
│   ├── emails.toml                 typed email handles and their parameters
│   ├── interface.toml              every text the desktop interface says that is no command's and no error's (§12)
│   ├── log-events.toml             every event the central log can receive (§15.4)
│   ├── capabilities.toml           every capability a plugin can be granted (§11.3)
│   ├── categories.toml             every category an entry is listed under, in sets: two standards' and the platform's own (§20.4)
│   ├── external-identifiers.toml   things outside the platform, and what identifies each on Linux, macOS and Windows (§20.4)
│   ├── i18n/                       every user-facing string and tag list, both locales
│   └── wiki/                       an entry's wiki text, by locale, registry and name (§20.4)
├── policy/                         HAND-WRITTEN — the Cedar packs (§14.4)
│   ├── schema.cedarschema          reference/identity-and-authorization.md §4
│   ├── world.json                  the entities every policy case is about
│   ├── core/                       the invariants, and tests/ beside them
│   └── packs/{family,org,collective}/
├── migrations/                     HAND-WRITTEN — forward-only, numbered
├── local-migrations/               HAND-WRITTEN — the local SQLite database's, never mixed
├── docs/                           GENERATED AND COMMITTED — the surfaces of §15.3 and §20.5
│   └── README.md                   HAND-WRITTEN — how to work on this repository; the root README.md presents the project
├── crates/
│   ├── nc-catalog/                 GENERATED AND COMMITTED — the shared core
│   ├── nc-catalog-compiler/        HAND-WRITTEN — what reads catalog/ and writes it, and every page beside it
│   ├── nc-canon/                   canonical JSON and hashing (§16.4)
│   │   └── canaries/               NOT COMPILED — a code the §15.1 walk must report
│   ├── nc-ids/                     Snowflake, the machine flake, Denote naming (§16)
│   ├── nc-fuzzy/                   the one fuzzy matcher: every word a subsequence of a name or a tag, positions per word, the ranking (§12.4)
│   ├── nc-markdown/                the one reader of Markdown: a closed subset read into a typed tree, anything outside it refused by line and column (§12.16)
│   ├── nc-pages/                   the pages an install carries: every generated page of `docs/` and the changelog, embedded as written (§12.16)
│   ├── nc-journal/                 the hash-chained journal (§14.9)
│   │   └── postgres.rs            in src/ — the store that runs
│   ├── nc-log/                     the central log's type wall (§15.4)
│   │   ├── events.rs              in src/ — GENERATED, one struct per event
│   │   └── fixtures/               the files that MUST NOT COMPILE
│   ├── nc-platform/                a path one user's alone, which process this is, a program the service manager restores, the user's bin directory and the clipboard's tool, per system (§2.7), and how the Leader reaches a process (§12.8) — THE TWO FILES OF `unsafe`
│   ├── nc-channel/                 every name an install gives the system, from one function (§17.1), the development checkouts installed beside one, the `stopped` mark's one reader (§17.5), and a person's keybindings, read beside the other channels' (§12.8)
│   ├── nc-ipc/                     the protocol a daemon is reached through: frames, the handshake, the heartbeat, an agent's session, the watch, a note and a display, the client every program asks through (§11.1), and the alert line an interface draws once it lost its daemon (§12.17)
│   ├── nc-policy/                  the pack's compiler, validator and case runner
│   ├── nc-authz/                   the decision point: the embedded pack, asked (§14.2), and the directory it asks, which holds the agents a person makes (§14.6)
│   ├── nc-kernel/                  the local kernel: the host of the plugin world (§11), and the interface's own state (§12.1)
│   │   └── wit/                    the plugin world: a byte copy of the workspace's reference
│   ├── nc-local/                   the local app database: one SQLite file, private (§4), the journal's store on the person's machine and the agents a person made in it
│   ├── nc-guest/                   the world's guest half, generated once for every plugin
│   ├── nc-prune/                   one pruner, for every table (§15.8)
│   └── nc-schema/                  a command's schema subset, selector and judge (§11.4), the tool an MCP session is handed of it (§20.3), and the Leader's grammar (§12.8)
├── plugins/                        WASM components, one directory each (§11.3)
│   └── reference/                  the plugin the kernel is proven with, its entries beside its manifest
│       └── i18n/                   what its commands say, one file per locale (§20.4)
├── apps/                           the binaries a person's machine runs, each a library and a thin binary (§10.2)
│   ├── daemon/                     `noctua-daemon`: the install, the profile, the device, the identifiers' mark, and the kernel it serves over the world and the agents its person made, to the person and to their agents' sessions (§11.1, §16.2, §20.3)
│   ├── cli/                        `noctua`: a command line read against a command's schema, the daemon asked and its process controlled, the exit codes, the layout its help and `docs/cli.md` share, the installer, `noctua-dev`, the `checkouts` picker, and `noctua mcp`, which serves an MCP client on its standard streams as one agent's session (§20.2, §17.1, §20.3)
│   └── desktop/                    `noctua-desktop`: a window and its webview, kept hidden until the daemon's state shows it, that state mirrored by a watch, its footer drawn from it and changed only by commands and notes, its own words said from the interface's registry, and `--probe` (§12)
│       └── sheet.rs               in src/ — GENERATED by `cargo xtask build --sheet`, the layout utilities its markup writes
├── stand-ins/                      the crates `[patch.crates-io]` puts in place of a dependency's dependency, members each named as the crate it stands in for (the root manifest says why)
│   ├── dirs/                       `dirs` for `wry`: the one function it calls, answering nothing
│   └── kuchikiki/                  `kuchikiki` for `wry`, which names it for Android alone: empty
└── xtask/                          HAND-WRITTEN — the five verbs
    ├── tailwind/                   Tailwind's `package.json` and `pnpm-lock.yaml`, which the sheet is built with
    └── canaries/                   NOT COMPILED — what the gates must find, and the review loop
```

The map draws three files relative to their crate — `crates/nc-journal/src/postgres.rs`, the
store that runs, `crates/nc-log/src/events.rs` and `apps/desktop/src/sheet.rs` — and they are
written out here so the reference gate resolves them. The kernel's `wit/` holds one file,
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
`docs/`, and two files inside hand-written crates, `crates/nc-log/src/events.rs` and
`apps/desktop/src/sheet.rs` — the second written by `cargo xtask build --sheet`, and held by
`check`, which builds it again and refuses one that differs (`xtask/src/tailwind.rs`).
`docs/README.md` is the one hand-written file in them, named in `HAND_WRITTEN_FILES` in
`crates/nc-catalog-compiler/src/lib.rs`.
`cargo xtask check` refuses a hand edit to any of them, and a leftover in the first two
that nothing generates any more.

## Commands

```bash
cargo xtask build     # compile catalog/ into the crate that carries it
cargo xtask build --plugins   # every plugin, built for wasm32 as a component
cargo xtask build --sheet     # the desktop's sheet, built by Tailwind from its markup's classes
cargo xtask check     # THE gate. CI runs this, `cargo xtask dev` for the stack
                      #   it needs, the `cargo install` for cargo-deny, and
                      #   `cargo xtask build` twice in the determinism job
cargo xtask dev       # bring up PostgreSQL and the central log store
cargo xtask dev --install     # install this checkout as its development channel
cargo xtask dev --uninstall   # remove it, its profile kept
cargo xtask docs      # regenerate the developer surfaces under docs/
cargo xtask release --channel staging --dry-run   # what a channel would release
cargo xtask release --channel staging   # gate, then write its version and changelog
```

`cargo xtask dev` brings the stack up on this checkout's two ports and prints them, with
the `DATABASE_URL` they make: a tool outside `cargo xtask` — a hand-run `docker compose`,
the PostgreSQL suites run alone, `xtask/mutants.py` — is told them by exporting what it
prints. `POSTGRES_PORT` and `LOGS_PORT` move a port.

**On Linux `cargo xtask check` needs `Xvfb` on `PATH`**: `nc-platform`'s suite starts one,
with no socket in `/tmp`, and presses the Leader on it, and a machine without it fails that
test rather than passing it unrun (*Not applicable is a property of the system, never of
the machine*). It is Debian's and Ubuntu's `xvfb` package, which the Linux job's runner
has to carry, and `nix shell nixpkgs#xvfb --command cargo xtask check` where Nix provides
it. `xtask/mutants.py` needs it as well, since its commands for `nc-platform`'s other
sources run that suite whole.

**And the desktop interface needs a webview and pnpm.** On Linux the workspace links
WebKitGTK, so building it needs `pkg-config` and the `-dev` packages of WebKitGTK 4.1, GTK 3
and libsoup 3 (Debian's and Ubuntu's `libwebkit2gtk-4.1-dev` brings all three), and the gate
builds the desktop's sheet with Tailwind, through `pnpm` (`xtask/src/tailwind.rs`), at the
version `xtask/tailwind/package.json` was locked with. Its suite opens a window on a display
the test is handed (`nc_platform::testing::Display`): on Linux an `Xvfb` of its own, the same
`Xvfb` the Leader's suite needs, so no window reaches the person's session and no wrapper is
needed; on macOS and Windows the session's own. On NixOS, which installs none of these
system-wide, this is the gate, with every library a build or a run needs on one line:

```bash
nix-shell -p pkg-config gtk3 webkitgtk_4_1 libsoup_3 xvfb nodejs pnpm \
  --run 'cargo xtask check'
```

`cargo xtask dev --install` builds the desktop interface with the CLI and the daemon, so on
Linux it runs where the same libraries are.

`cargo xtask dev --install` installs this checkout as its development channel, on every
system and with or without the stack: it builds the CLI, the daemon, the desktop
interface and the plugins and copies them into the install the base directories name — the
three binaries out of the target directory cargo built them in, wherever `CARGO_TARGET_DIR` or
`build.target-dir` puts it, and the plugins out of the checkout's own `target/`, from its
`plugins` folder, where the plugin build writes them whatever the environment says — with
the manifest, the device and the test world (`nc_cli::install`), stopping the
daemon an earlier install runs first — once it has found the service manager and asked
whether it holds the earlier restorer: from an environment that does not reach it, it stops
nothing and installs nothing. It registers the daemon with the system's service
manager as `noctua-daemon --restored` — a systemd user unit, a launch agent, a logon task
— restored when it dies and started at each login, and starts it through that manager
unless the profile carried the `stopped` mark, a person's stop or one an install or an
uninstall that did not finish left; on Linux it enables lingering where logind
lets it, so the daemon starts at boot, and otherwise prints the command and the NixOS
option that do. Last, it points `noctua-dev`, in the user's bin directory, at the CLI
`noctua-dev-<checkout>` it installed, records the checkout beside the manifest, and prints
where `noctua-dev` is; it leaves `PATH` and every file a shell reads at its start as they
are, so where the bin directory is not on `PATH` it prints the one line that puts it there
for the person's shell. `--uninstall` finds the service manager first — from an environment that does
not reach it, it removes nothing — then stops the daemon, unregisters its restorer, with
the one `systemctl --user daemon-reload` a unit the user's manager has read at a login or
a boot needs before it goes, and removes the binaries, the manifest and the record, and the mark its
own stop wrote, and keeps the profile — a person's stop with it; when `noctua-dev` pointed
at this checkout, it points it at the one installed most recently of those that remain, or
removes it. The environment is read then and never by a binary — the base directories, the
home the bin directory is under, and
`NC_INSTALL_NAMESPACE`, which relocates the install as a test's, which never asks for
lingering and whose restorer is kept where no login reads it on systemd and launchd — the
runtime directory the next boot forgets, the profile an agent is bootstrapped from for
one login — while on Windows its task starts at each logon too, until it is
unregistered.

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
| a member under `apps/` | `apps/<dir>/`: `Cargo.toml` and `src/`, a library and its binary | every list a crate joins (the row above), by its path under `apps/` — the trees are `MEMBER_TREES` in `xtask/src/members.rs`, so the walks read it already, and the hand-kept lists in `xtask/src/gates.rs` are the ones to join. The package is `nc-<dir>` — the mutation runner reads it from the path — and the binary is named in a `[[bin]]` table: what the app does is the library's, and the binary's `main.rs` reads its arguments and prints through the one printing door, the invocation it answers and its exit codes decided by functions its own unit tests call. **A source's mutation command runs its own module's unit tests, and never a suite that spawns a process**: `cargo test -p nc-<dir> -- <module>::` for a module and `--bin <binary>` for `main.rs` (`--lib` is refused outright), so a suite in `tests/` that starts the binary, a daemon or a child, or registers a unit, runs under no operator. `a_suite_that_spawns_a_process_is_run_by_no_mutation_command` in `xtask/src/check.rs` holds every suite of every member to that — one that starts a process through its crate's test doors, a `.spawn()` with nothing in its parentheses, included — but the ones `SPAWNED_BY_THE_RUNNER` writes down, and such a suite's `NOT_MUTATED` row says `spawns a process, so no mutation command runs it`. It makes its scratch, its children and its files under a scratch root, each with a guard that removes it |
| a plugin | `plugins/<dir>/`: `Cargo.toml`, `plugin.toml`, its entries — `commands.toml` and `i18n/<locale>.toml` for every locale — and `src/` | the lists a crate joins in `xtask/src/gates.rs` — `JUDGED_BY_THE_ERROR_CODE_RULE`, which the census of every member cargo resolves holds it to, and `JUDGED_BY_THE_RAISING_RULE` and `READ_BY_THE_HATCH_WALK` by hand — because the §15.1, raising and hatch walks read `plugins/` as they read `crates/`. The package is `nc-plugin-<dir>` — the plugin build refuses any other name, and the mutation runner reads the package from the path — with `crate-type = ["cdylib", "rlib"]`: what the plugin does goes in a module the host target tests, and `impl Guest` with `nc_guest::generated::export!` in an `adapter` module whose file opens with `#![cfg(target_arch = "wasm32")]`, because a host `cdylib` holding the exports does not link — the condition inside the file, since one on the `mod` line reads to the ignored-test gate as hiding every test in it — and `nc-guest` under `[target.'cfg(target_arch = "wasm32")'.dependencies]` when only the adapter names it, since a host build carrying a dependency nothing it compiles uses is a cargo warning the check would print. `cargo xtask build --plugins`, and `check` before the tests, builds it for `wasm32` and encodes it, refusing a component that imports anything the world `plugin` does not declare (which is what a dependency with bindings of its own brings in), then lints and documents it for that target — the only run that reads the adapter; a kernel suite loads it through `nc_kernel::component_for_tests`, which refuses a component its sources no longer build. Every `.rs` joins the mutation census, and the adapter's command is the plugin build |
| the plugin world | `reference/noctua-kernel.wit` in the workspace, then `crates/nc-kernel/wit/noctua-kernel.wit` | the workspace's file first — it is normative — then the copy, byte for byte, then `WIT_BLAKE3` in `crates/nc-kernel/src/world.rs`. `the_kernel_world_is_the_reference_world` holds the copy to the reference wherever the workspace is there, and the digest holds it in a checkout; a new `package` version is `WORLD_VERSION` as well |
| a command | `catalog/commands.toml`, both locales | its `args_schema` is held to the closed subset `crates/nc-schema` documents — every object `"additionalProperties": false`, no keyword outside `nc_schema::KEYWORDS`, and no character `nc_schema::unseen` answers for — `INVISIBLE_RANGES` and the default-ignorable `BLANK_RANGES` — but JSON's own whitespace, which the title is held to as well — and an `argument:<name>` selector needs that argument required, a string with `x-entity` naming its Cedar type, and `maxLength` at most 128, and never selects `Realm` or `Feature`, which `realm` and `feature:<id>` alone do; a `feature:<id>` selector names a feature of the platform the realm's directory holds, for a command of the kernel's own that reads none of the realm's data — the kernel refuses the form in a plugin's declaration — and needs the feature and a permit naming it (*a feature of the platform*, below); the kernel judges every command a plugin declares at `activate` by the same function (`NOCTUA.md §11.7`), so a rule written there reaches both. Its entry is `title` (the display name), `description` (the line description) and `paragraph` — keys into both locale files — and `tags`, the key of a list under `[tags]` in each, with `categories` — names `catalog/categories.toml` holds, `unknown-category` otherwise — and `glyph` optional: `crates/nc-catalog-compiler/src/metadata.rs` hands every command's texts to `nc_schema::entry::judge`, which refuses a line past one terminal line, a paragraph that repeats it, a tag twice or holding a comma or a brace, a text a reader cannot see all of, and a glyph outside `nc_schema::entry::SYMBOLS_ONLY` or written any way but `U+` and upper-case hex, four digits at least and no leading zero past them (`U+F0E2`, never `U+0F0E2`); a text missing from a locale is `missing-key` naming the command and the text, and the four texts are rows of `docs/commands.md`, what help says of the command a block of `docs/cli.md` in each locale, and its tool, by `nc_schema::entry::as_tool`, a block of `docs/mcp-tools.md` |
| a keybinding | `keybindings = [...]` on the command in `catalog/commands.toml` | written in the Leader's grammar alone — `Leader <key>`, `Leader+<key>` or `Leader+Chord+<key>`, the key named as `KeyboardEvent.code` names it, `Leader+Chord+KeyQ` — which `nc_schema::keys::Binding::parse` reads, the one grammar every reader of a binding is held to, every other spelling refused as `malformed-keybinding`; one key of one table runs one command (`duplicate-keybinding`), and the same key in another table is another binding. **A binding carries no argument**, so it names a command that takes none it requires — the leader menu's `s`, `h`, `d`, `/` and `w` are `page.settings`, `page.help`, `page.dev`, `page.shortcuts` and `page.actions` for that reason — and the desktop runs it as the person by the binding in force alone (`apps/desktop/src/keys.rs`): a key of the menu's table is an entry of the leader menu, which lists every one in force, and a held key runs at once. The catalog binds the leader menu's keys and, with the Leader held, `q` and the arrows; the workspaces' and the layout's keys arrive with them (Stage 17) (`NOCTUA.md §15.2`); `docs/commands.md` publishes a command's list as a `Keybindings` row, which `xtask/tests/the_pages_reach_the_catalog.rs` holds, and `docs/keybindings.md` lists every binding by its table and key with the display names of the command it runs. A key joins the vocabulary as a variant of `nc_schema::keys::Key` and a row of `VOCABULARY` in the same place, and as an arm of `nc_platform::leader::physical` — which does not compile until it has one — with the kernel's code for it, macOS's virtual keycode and, for a key no layout moves, Windows' virtual key, each copied from that system's own header and written out again, in the header's order, in `every_key_has_the_code_each_systems_list_gives_it`, and as an arm of `nc_platform::leader::xkb_name` with the keysym name a US board's key types, as xorgproto's keysymdef.h writes it, which a Wayland compositor's binding names the key by, written out again in `every_key_has_the_xkb_name_keysymdef_gives_it`; and raises `nc_channel::keys::FORMAT`, because an older reader of a person's configuration refuses a file naming it and another channel's reader must know it cannot judge one; a person's own choice overrides the catalog's in their channel's configuration directory, `<config>/keybindings.json`, which `nc_channel::keys` reads beside the other channels' and `noctua help keybindings` shows, each refusal with its `NC-CHAN` code; and a system's shortcut as a row of `SYSTEM_SHORTCUTS` copied from the source it names, never from memory, written out in `the_system_shortcuts_are_the_rows_their_sources_write` |
| a keyword the argument schema accepts | `OWNED_BY` in `crates/nc-schema/src/lib.rs`, and the subset table in that file's header | the one type it belongs to — a keyword written on another type is refused rather than read as a no-op — its reading in that type's compile and its refusal in the validator, and a row of `WHAT_EACH_KEYWORD_DOES` beside the tests: a schema using it, a value it refuses, and that refusal gone once the keyword is taken out. `every_keyword_in_the_subset_decides_something_or_is_an_annotation` refuses a keyword with no row, because a keyword the subset admits and nothing reads is one an author believes refuses what it does not. The catalog compiler and the kernel's `activate` both judge through this crate, so the keyword reaches every built-in and every plugin in the same build — and a plugin that uses it is refused by a kernel built before it, which is the closed subset working |
| a command of the kernel's own | `catalog/commands.toml`, in a namespace the kernel keeps, and `crates/nc-kernel/src/builtin.rs` | a namespace the kernel keeps is a row of `RESERVED` in `crates/nc-kernel/src/registry.rs`, refused to every plugin from then on, and a row of the loop in `a_namespace_the_kernel_keeps_is_refused_before_anything_is_compiled` in `crates/nc-kernel/tests/activation.rs`; a `Builtin` variant, in its `ALL`, named as the catalog names the command, with its arm in `builtin` in `crates/nc-kernel/src/dispatch.rs` — `every_command_the_catalog_gives_the_kernel_is_one_it_runs_and_it_runs_no_other` holds the catalog, the registry and the handlers to each other both ways — and a test holding the arguments the handler reads to `Command::arguments`, both ways; one that changes state outside every unit is noted as having left the machine before it runs, so an undo stops at the step that ran it, and changes nothing until its dispatch's entry has landed — `limits.prune` is rehearsed as a dry run, then journaled as allowed by `pruned_once_journaled` in `crates/nc-kernel/src/dispatch.rs`, and only then deletes, and an agent's change is rehearsed by the directory, then journaled as allowed with what it makes by `changed_once_journaled`, and only then made — since nothing a journal refuses afterwards could put it back. One that changes state only through the command it dispatches — `surfaces.activate` — is named in `dispatches_what_it_shows` in `crates/nc-kernel/src/ui.rs`, so it opens no unit and leaves no note of having left the machine, and in `AS_WHAT_IT_RUNS` in `crates/nc-catalog-compiler/src/docs.rs` and its mirror in `xtask/tests/the_pages_reach_the_catalog.rs`, so `docs/commands.md` says undo takes back what it runs |
| a feature of the platform | `policy/world.json`, `policy/packs/<profile>/features.cedar` with its cases in `policy/packs/<profile>/tests/features.toml`, and the command's `resource_selector = "feature:<id>"` | the `Feature` sits in the world under its realm — two rows of `THE_WORLD` in `xtask/src/policy.rs` — and its permit names it by its id and its verb (`resource == Feature::"<id>"`), with a case for each constraint: another verb (`the_action_is_the_boundary`), another kind of principal (`the_principal_is_the_boundary`) and another feature, which moves the lists the *policy* row names. No static gate holds a selector's feature to the world, so the kernel's suite dispatches the command and reads its entity back from the journal; and a development install keeps the world it was installed with until `cargo xtask dev --install` copies it in again |
| an agent permit | `policy/packs/<profile>/features.cedar` — `principal is Agent`, one verb, `resource == Feature::"<id>"` — beside the person's permit on the same feature, with its cases in `policy/packs/<profile>/tests/features.toml` | it is half of what an agent is answered: the decision point asks the same request again as the agent's operator, so the person's permit on the feature is the other half, and an agent reaches the feature only once its person grants it that feature with `agents.grant` — a grant may name any feature of the realm, and one no agent permit names allows nothing. Its cases ask as an agent marked `via_agent`, one link from its person: its kind of principal is refused with a `Device` so marked (`the_principal_is_the_boundary`), since a person is let in by the person's permit and an unmarked agent is refused by the core; the other half is a case asked as the person so marked; and an outsider's agent pins Cedar's answer over the whole world, which moves the lists the *policy* row names. A feature whose commands change who may act for a person takes none: the person's permits on `agents` and `mcp` ask `principal is User`, and the `agents.*` that change and a setting's set run at the top of a call tree only (`only_at_the_top` in `crates/nc-kernel/src/registry.rs`), refused nested with `[NC-KERN-0025]` |
| a setting | `catalog/commands.toml` — `settings.<setting>.read` and `settings.<setting>.set`, commands of the kernel's own in category `noctua.settings`, the read answering the one object the set takes whole — both locales, `crates/nc-kernel/src/settings.rs`, and the setting's feature (*a feature of the platform*, above) | the set's one effect is the kernel's own entry in the channel's local store, staged in the set's unit, so it is named in `writes_only_its_entry` in `crates/nc-kernel/src/settings.rs` — by the command, never by its namespace — or `leaves_the_machine` in `crates/nc-kernel/src/history.rs` notes it as having left the machine and every undo stops at it — and in `TAKEN_BACK_WHOLE` in `crates/nc-catalog-compiler/src/docs.rs` and its mirror in `xtask/tests/the_pages_reach_the_catalog.rs`, so `docs/commands.md` says undo takes its step back, as `of_the_kernels_own_changes_undo_takes_back_undo_redo_and_the_switch_alone` in `crates/nc-kernel/src/history.rs` holds the kernel to; **the set runs at the top of a call tree only** — `only_at_the_top` in `crates/nc-kernel/src/registry.rs` reads it from the namespace and from its being a change or decided as one, so a set declared no change but decided under `Write` is kept there too, and the decision point refuses it nested with `[NC-KERN-0025]` — so its row in `apps/daemon/tests/every_command_from_the_cli.rs` goes after `kernel.undo` and `kernel.redo`, which still step through the write before them; an entry holding anything but the texts the module writes is refused as a row the kernel did not write, never read as the default; and what the setting holds is a field of `nc_kernel::Settings`, read by `Kernel::settings`. A value that lets something act for the person — MCP's on — is named in `switches_mcp_on` and `puts_mcp_on` in `crates/nc-kernel/src/settings.rs`, so the decision point holds its set, and an undo or a redo that would put it back, for the person's confirmation in the Noctua window under codes of its own (`NOCTUA.md §12.15`); a value that does not, never waits. A setting is never a permission (`NOCTUA.md §14.3`): what a principal may do stays a grant's. **A setting whose value is the channel's `<config>/keybindings.json`** — the four shortcut settings — is a variant of `Which` in the same module, in its `ALL`, with its two names and its one field, a variant of `Shortcut` it is asked as and of `nc_channel::keys::Change` the daemon's keeper hands the judge (`Keybindings` in `apps/daemon/src/environment.rs`): its read answers the keys in force and its set is `written_once_journaled` in `crates/nc-kernel/src/dispatch.rs` — judged by the keeper, journaled as allowed, and only then written — never named in `writes_only_its_entry`, so it is a step that left the machine and every undo stops at it; it is decided on `feature:interface`, its schema's bounds are held to the vocabulary by `every_bound_the_shortcut_schemas_declare_is_held_to_its_source`, a refusal of a choice is one of the codes under *A shortcut setting refused* in `catalog/errors.toml`, and its rows join `FITTING`, `ROWS` and `SHORTCUTS` in `crates/nc-kernel/tests/the_decision_point.rs`. **A setting of how the interface is shown** — the footer's visibility, the language, the appearance — is neither: its value is a part of the interface's state (`crates/nc-kernel/src/shell.rs`), a variant of `Setting` there, in its `ALL`, with its one field, its closed list of words in `words` — the first its default — a field of `Chosen`, and its two commands variants of `Verb`, so everything *a part of the interface's state* joins applies; the commands are declared `reversibility = "read"` on `feature:interface`, the set `Write` and the read `Read`, the set's schema listing the words in `words`' order, which `each_setting_of_the_interface_is_one_word_of_its_own_list` holds both ways; the set is a row of the top-only list in `only_a_setting_or_an_agent_change_of_the_kernels_own_runs_at_the_top_alone`, and its nested refusal a case of `a_plugin_sets_no_setting_of_the_interface_from_inside_a_command`; and the desktop reads it as a field of `Settings` in `apps/desktop/src/mirror.rs`, each word a row of that type's `WORDS` |
| an interface text | `catalog/interface.toml` — a `[[text]]` with its `id`, its `text`, its `params` and where it is `shown` — and `"interface.<id>"` in both locale files | the id is two or more lower-case words joined by dots, which becomes the handle `nc_catalog::interface::Text::<Id>`, and the key is `interface.` and the id, nothing else (`interface-text-name`, `interface-text-key`); `shown` says in one English sentence where the interface shows it (`interface-text-unplaced`); a sentence is one key, so a count that reads differently for one and for many is two texts, and its placeholders are exactly `params`; the entry claims its key, so a sentence under `interface.` that no entry names is `unreferenced-key`. `docs/interface.md` publishes every text in both locales. **The desktop says a text only by its handle** (`apps/desktop/src/text.rs`: `said`, and `said_with` for one that takes parameters), never by a text written where it is drawn; a control is still named by its command's display name and a refusal by its code's message, which are their own registries'. Two ids of which one, with a backtick after it, sorts before the other with a hyphen after the same letters are listed on the page in an order its test refuses (`every_page_lists_its_entries_in_order`): name the second so that it does not open with the first |
| a construct of the Markdown this tree reads | `crates/nc-markdown/src/lib.rs` — the block or the inline, its reading and the `Rule` each way of writing it wrongly is refused by, in `Rule::ALL` with its word and its sentence — and `crates/nc-markdown/tests/markdown-corpus.tsv` | a row of the corpus in the same edit for what is read, and one for each refusal: `every_rule_has_a_row_that_refuses_by_it` fails a rule with no row, and a row is written as the suite prints a tree, compared as text. **The subset is closed on purpose**: whatever is outside it is refused where it is read, never passed through as text, so a construct is added only with what the interface draws it as, and a link's target stays one of `Target`'s three forms — `https` the only scheme. A link in a row whose target ends in `.md` and names no file of this repository is refused by the reference gate, which reads the corpus as it reads every text file: give the target an anchor, or name a file that exists. A clause is an operator in `xtask/mutants.py`, under `markdown`, which the crate's own command kills. The catalog compiler reads every wiki text written in Markdown and every page it emits through this crate (`markdown-outside-the-subset`, naming the file and the line), so a construct taken out fails `cargo xtask build` on the first page that used it |
| a page an install carries | a row of `PAGES` in `crates/nc-pages/src/lib.rs`, in the order of the names, with whether the help pages list it | nothing for a page the catalog compiler emits but the row: `the_pages_are_the_generated_pages_on_disk_and_the_changelog` fails until `PAGES` is every generated page under `docs/` and the changelog, each embedded as the file holds it, and `each_list_holds_the_pages_its_section_names` holds the help list written out. A page is never written here: it is generated (*a page about what the catalog cannot see*, below), and what a person reads in their own language is the registries' texts, not a page's prose, which is English with a column for each locale (`NOCTUA.md §12.16`) |
| a part of the interface's state, or a command that changes it | `crates/nc-kernel/src/shell.rs` — a field of `State`, its writing in `to_value`, its strict reading in `from_value` and its rules in `holds` — and, for a command, a `Verb` with its arm in `Change::made`, `catalog/commands.toml` on `feature:interface` as a read, both locales | a field written and not read back strictly, or read and not held by `holds`, is a saved state trusted or set aside wrongly: `varied` in the module's tests gives the field a value the first run's state does not hold, which `what_is_kept_is_read_back_as_itself` names without `..` and reads back, and a rule it is held to is a row of `State::rules` of its own, asked inside no other, with an edit in `rule_breakers` that breaks it and no other rule — the test refuses a rule no edit breaks alone — and a shape that changes what a saved state means is a new `SHAPE`, which sets every older one aside. A command is a variant of `Verb` in its `ALL`, named as the catalog names it, with what it reads in `reads`, which `the_declared_commands_of_the_interface_are_the_ones_this_module_implements` holds to `Command::arguments` both ways; its namespace, if new, a row of `RESERVED` in `crates/nc-kernel/src/registry.rs` and of the loop in `a_namespace_the_kernel_keeps_is_refused_before_anything_is_compiled`; a row of `FITTING` in `crates/nc-kernel/tests/the_decision_point.rs` and of `ROWS` in `apps/daemon/tests/every_command_from_the_cli.rs`; and a bound in its schema held equal to its `[kernel]` limit in `every_bound_the_schemas_declare_is_the_catalogs`. Never `mutating`: a change of what a window shows is no step (`NOCTUA.md §12.1`). A command whose change moves at the rate a window draws may also be a fact a note says (`nc_kernel::Kernel::note`): a row of `FACTS` in the same module, the fact's name beside the command, whose arguments the fact is — held to that command's schema and decided as it is — and a row of `a_note_says_the_four_facts_of_the_interfaces_rate_and_nothing_else`. **And the desktop's reader of it**: a field the state gains is read by `Shell::read` in `apps/desktop/src/mirror.rs`, which refuses a state holding a field it does not read — the window then keeps drawing the state it held — and `every_field_is_read_as_itself` there gives it a value; a part the daemon answers a watch with beside the state (`Watched::state` in `apps/daemon/src/server.rs`) is read by `watched_shell` there, which refuses one it does not read, and `a_watch_is_answered_with_the_state_beside_the_environment_and_the_leader` gives it a value — the count of the writes of the keybinding configuration, `keys`, is read by `watched_keys` beside it, and the window reads the keys in force again when it is another number (`reads_the_keys_again` in `apps/desktop/src/host.rs`); a fact a note says is a variant of `Fact` there too; and what the state holds of a seat other than the system's own is drawn by `apps/desktop/src/seat.rs`, written onto the page as the design system's seat attributes, never by a class or a rule written here |
| a read the dev pages show of this machine | `catalog/commands.toml` — a command of `dev.`, a read, both locales — and its handler in `crates/nc-kernel/src/dev.rs` | everything *a command of the kernel's own* joins, and: a row of `DECLARED` beside the module's tests, with the arguments it reads and what it is decided on — `feature:interface` for what counts, names and versions say, the realm for a record that says who did what — **never a feature of its own while one more would take `agents.list` past a frame** (`the_widest_list_of_agents_fits_one_frame` in `crates/nc-kernel/src/agents.rs`: an agent may be granted every feature the shipped world holds); a row of `FITTING` in `crates/nc-kernel/tests/the_decision_point.rs` and in `apps/daemon/tests/agent_sessions.rs`, of `ROWS` in `apps/daemon/tests/every_command_from_the_cli.rs`, and of `DEV_FOR_EVERYONE` or `DEV_RECORDS` beside the decision point's cases, which dispatch each as a guardian, an adult, a child, a frozen account, a device and an agent. An answer whose rows can grow is paged: `from` and `max` read by `page`, `max`'s maximum `[kernel] dev_page_max` — `every_bound_the_paged_schemas_declare_is_the_catalogs` holds the schema to it — its rows cut by `fitting` and `next` said. A count against a limit is a field of `Used` and a row of `counts`, named `<table>.<limit>` as `catalog/limits.toml` writes it, which a test holds to the registry; a table is answered from `nc_catalog::TABLES` alone, counted by `nc_prune::sqlite::Sqlite::rows`, never by the pruner's own count, which refuses a table a trigger protects. What the daemon holds and the kernel does not is a method of `nc_kernel::Hosting`, answered by `Host` in `apps/daemon/src/environment.rs` from the server. **A line of the log reaches an answer only through the sink** (`LogSink::kept`), which is the type wall's: nothing here formats one |
| a class of the desktop's markup | `class: "…"` in `rsx!` under `apps/desktop/src`, then `cargo xtask build --sheet` | written as a string literal right after `class:` — one written any other way is refused, since the sheet could not be built for it — as lower-case words joined by `-`, with variants before a `:` and a fraction of two whole numbers after a utility that takes one, a size or a place (`FRACTIONAL`), in no other shape (`judged` in `xtask/src/tailwind.rs`): no arbitrary value, colour written out, custom property, opacity modifier — a palette step's colour, whose name ends in a number, and a share after it among them — or important flag, and a sheet that mixes a colour (`color-mix(`) is refused whatever wrote it (`what_it_lacks`). A size a utility takes as a bare value — `border-8`, `w-px`, `h-screen` — is Tailwind's own length and held by nothing here. A utility Tailwind knows only from its own theme — a spacing step, its palette — emits nothing here, where only the design system's bridge is imported, and is refused as a class with no rule; a token the design system does not map for Tailwind is a gap of the design system's, never a mapping written here — so a margin of none on the document's body is `m-auto`, the auto margins of a block whose width is auto being used as none. **The utilities are in no cascade layer**, as the design system's sheets are not, and are linked after them, so a utility on a design system's root composes it — `shrink` on a footer line gives way where the design system's rule says `flex: none` — and only one of the same specificity or less; a utility on one of its roots is a decision about that component's layout, made only where the window needs it. `check` builds the sheet again and refuses `apps/desktop/src/sheet.rs` where it differs |
| a control of the desktop's footer or of its drawer's controls rail, or a word its document says | a control of the footer: a variant of `Action` in `apps/desktop/src/footer.rs`, in its `ALL`, with its command in `command`, and the control drawn in `TheFooter` with `action:` set to that command; a control of the drawer's rail: a variant of `Control` in `apps/desktop/src/drawer.rs`, in its `ALL`, its command's name in `word`, what it runs in `run`, and the control drawn in `ControlsRail`; a word: a variant of `Said` in `apps/desktop/src/host.rs`, read by `said`, said by `SCRIPT` in `apps/desktop/src/page.rs` | **a control runs a command, and only a command**: its `data-nd-action` is the command's name, which the page says back as `action <command>` and `said` reads only through `Action::of` and `Control::of`, so a word the page makes up is read as nothing — and a control of a plugin's surface writes there what it is found by instead, its surface and its node's pointer (`surface::hook`), which no command is spelled as, so `said` reads it as nothing and never as the command its node names (`every_control_is_found_by_its_node_and_never_by_its_command`); a footer's action takes no argument (`every_action_is_a_command_without_arguments`), and a rail's control takes only what `run` gives it from the state the window holds — a constant, or a place the state names — never anything the page says (`every_control_and_tab_runs_its_command`); one that needs a value a person chooses is a command list's, not a control's; it is named by its command's display name in the window's locale (`title_of`), never a text written here. A word is one of a closed set, each written one way, and `only_the_closed_set_is_read_from_the_webview` names it beside the shapes refused; a word whose text is free — a draft — is the whole of the message after its word and one space, held to its command's bound before it is said. What the page says is evidence only of what the person could do from the CLI, so a word never confirms and never carries a page's own markup back — what the system's own pointer is over is said as that seat's hover, a target of `shell.hover`'s shape, and what the page says it shows (`seen`) is of a closed shape, runs nothing and is printed for a test's install alone — and the window runs what it names only as one of five things it reads it through: a control's `Action` or `Control`, a key of the vocabulary by its `KeyboardEvent.code` run by the binding in force (`keys::key_of`), the leader menu's entry a binding in force names (`Keymap::is_entry`) or the `w` list's action (`drawer::ACTIONS`), an option of the prompt's list the window itself offers — a registered command, or a parameter or a value its schema gives (`host::picked`) — and a tab of the drawer or an entry of the sidebar naming a page the drawer or the window's workspace area holds (`drawer::tab`, `sidebar::chosen`) — each held by a test of `apps/desktop/src/host.rs`, `drawer.rs` or `sidebar.rs` beside the shapes refused |
| a bullet of `NOCTUA.md §12` the desktop answers to | a row of `BULLETS` in the tests of `apps/desktop/src/footer.rs`, by its section and the first 32 characters of its text with its emphasis taken out | **every bullet is a row and every row a bullet**: `every_bullet_of_the_interfaces_sections_is_held` reads every section Stage 16 builds (`SECTIONS` there: all of §12 but §12.12, §12.13, §12.20 and §12.21) from `NOCTUA.md` wherever the workspace is above this repository and refuses a bullet with no row, a row with no bullet and the two in another order, so a bullet written or rewritten there is a row moved here in the same change. A row is held by a test of this crate, which the table also runs — by one of them but for a part arriving later, which the row names — by a test elsewhere in this repository, or by a suite of this crate's that starts a process, named by its file and its name, which the table holds that file to declaring under `#[test]` or `#[tokio::test]` — by the design system's own suite, saying what of the primitive's — not yet, saying what builds it — or by nothing, saying why the bullet claims nothing a test could hold; a sentence written *as built* under those sections is prose, never a bullet |
| an asset the desktop's webview loads | its path's constant and its arm in `embedded`, in `apps/desktop/src/assets.rs`, and its link in `apps/desktop/src/page.rs`, by a path relative to the document | never a file read when the interface runs: an answer is bytes the binary embeds, found by the whole path the webview sent, nothing decoded or joined, so no path reaches a file. Its type is a row of `TYPES` by its extension. A file the design system ships needs no row — every one of `noctua_design_dist::FILES` is served under `DIST` by its own name — and `every_file_the_design_system_ships_is_served_as_itself` fails on a release that ships a kind `TYPES` does not hold, `every_address_is_one_the_protocol_answers` on a link the protocol does not answer. **And nothing the document loads is anywhere else**: its first word in its head is its content security policy (`POLICY` in `apps/desktop/src/page.rs`), which lets the page load a script, a sheet or a font from its own origin alone and an image from it or written inline, and connect, frame, submit or take a base nowhere — so a script is a file the protocol serves, the page's own two among them (`assets::SCRIPT`, `assets::ENHANCES`), never one written inline, which the policy refuses to run; a markup attribute `style` is refused as well, so a component whose markup writes one is drawn only once the policy says why it may. `noctua-desktop --probe` asks the page for an address off the protocol and holds the policy to refusing it, and its own files to none of them refused |
| a list filtered by typing | the list's items as `nc_fuzzy::Item`s, matched by one `nc_fuzzy::Matcher` | never a second matcher, and never the fields joined into one text — the desktop's command list builds each command's item of the fields `kernel.find` builds one of, in its order (`apps/desktop/src/commands.rs`), from the registry asked in each locale, and matches it in the window, since a dispatch per keystroke would journal what a person types: each field goes in with its `nc_fuzzy::Role` — a name, or a tag or a synonym, in every language at once — so a word reports the field it matched and a name outranks a tag; build the items once and match on every keystroke. A position is a `char` index into that field's own text, never a byte; the empty query lists every item in the order given; and a bound a command declares on its query is `nc_fuzzy::QUERY_CHARS_MAX`, held equal by a test beside the command |
| a rule of the fuzzy core | the module of `crates/nc-fuzzy/src/` the rule belongs to, and `crates/nc-fuzzy/tests/fuzzy-corpus.tsv` | a row of the corpus in the same edit: it is the case file `noctua-design`'s matcher replays, from a copy that repository vendors with a `SHA256SUMS` record of its own, so a rule changed without a row is one that implementation will not learn. Any byte of the corpus changed, a comment's too, fails `cargo xtask check` until `FUZZY_CORPUS_SHA256` in `xtask/src/check.rs` is its new SHA-256, written in the same edit, and the copy and its record are then refreshed in `noctua-design` from the file as committed: the line its `SHA256SUMS` holds is that value. A component of the score is a width derived in `const` from the bounds in `crates/nc-fuzzy/src/score.rs`, whose assertions keep the packed score below `2^53`; and a clause is an operator in `xtask/mutants.py`, which the crate's own tests kill |
| a wiki text | `catalog/wiki/<locale>/<registry>/<name>.<ext>`, one in every locale | it is found, never declared: its extension, lower case, is its format — `txt`, `md` or `typ` — and every locale writes one in the same format, or the build fails naming the file; it is compiled into `nc_catalog::WIKI`, its entry's generated `wiki` field names its format, and its registry's page — `docs/commands.md` or `docs/external-identifiers.md` — links it. A text names an entry of the registry it sits under, `commands` or `external-identifiers`, and of no other. Only `md` ships, and only for commands, so the first `txt` or `typ` text is also its kind added to the `catalog` row of `WHAT_MAY_SIT_UNDER` in `xtask/src/check.rs`, which refuses a kind allowed and worn by nothing; a registry of its own is a row of `REGISTRIES` and of `names_by_registry` in `crates/nc-catalog-compiler/src/metadata.rs`. The citation and path gates read every Markdown file, so a `§` in one must resolve and a backticked path must exist |
| a category or a set of categories | `catalog/categories.toml`, both locales | a category is named `<set>.<slug>` under the set it says it is in, and carries a display name and a line description in every locale — the two texts a list of them shows, held to `nc_schema::entry::judge` by `crates/nc-catalog-compiler/src/metadata.rs`, and no other text; the rest of its shape is `crates/nc-catalog-compiler/src/registries.rs`'s. A category of a standard set carries its `standard_name` as the standard writes it, and a `role` where its standard ranks its categories — every category of such a set does — while the platform's own set carries neither and names no `version`; a standard set names what defines it in `defined_by` and the version it was read from in `version`. A standard's values are copied from the standard, never from memory, and `xtask/tests/the_registry_reaches_the_code.rs` holds the two standards' rows written out, so a row added to or taken from a standard set is an edit there as well. A name that is the same in both languages — a project's own — is a row of `THE_SAME_IN_BOTH` in `crates/nc-catalog-compiler/src/messages.rs` with its reason, refused once the locales stop sharing it (`exemption-unused`). A command names categories from here; `docs/categories.md` and `noctua help categories` list them, the second with no daemon |
| an external identifier | `catalog/external-identifiers.toml`, both locales | it carries a command's four texts — `title`, `description`, `paragraph` and the key of a `[tags]` list — held to `nc_schema::entry::judge` by `crates/nc-catalog-compiler/src/metadata.rs`, with `categories`, `glyph` and a wiki text under `catalog/wiki/<locale>/external-identifiers/` optional as a command's are. What identifies it is a table for each system it is identified on — `[identifier.linux]`, `[identifier.macos]`, `[identifier.windows]` — at least one and none empty, each field a list of values of one closed shape, held by `identification` in `crates/nc-catalog-compiler/src/registries.rs`: no `..`, nothing a reader sees nothing of and at most 260 characters anywhere, a path absolute on Linux and macOS and under `%ProgramFiles%\`, `%ProgramFiles(x86)%\` or `%LOCALAPPDATA%\` on Windows. **A value is copied from a measurement on the system or from the product's own sources, never from memory**, the file's header says which, and one neither confirms is left out and written down for `NOCTUA-PLAN.md`'s Stage 27+; `the_external_identifiers_hold_chrome_and_chromium_on_every_system` in `xtask/tests/the_registry_reaches_the_code.rs` holds every shipped value written out, so a value added or taken away is an edit there as well. A field added to a system is a field of `OnLinux`, `OnMacOs` or `OnWindows` in `crates/nc-catalog-compiler/src/model.rs` and in the emitter's `the_systems_tables`, and a `Field` in `registries.rs` with its key, its shape, the line `held_in` reads it from, and its row of `IDENTIFIED_BY` — the one list the compiler judges by, the generated `ExternalIdentifier::identification` hands out, the page publishes and `noctua help external-identifiers` prints, so no reader names a field. `every_field_of_a_systems_table_is_one_the_list_names_reading_its_own_key` fails until a table's field is a row of the list, the generated crate does not compile until a row of the list is a field of its type, `every_external_identifier_reaches_the_code` holds the generated walk to the list, and `the_external_identifiers_hold_chrome_and_chromium_on_every_system` fails until the new field is written out. A product's name that is the same in both languages is a row of `THE_SAME_IN_BOTH` with its reason. `docs/external-identifiers.md` and `noctua help external-identifiers` list every entry with what identifies it, the second with no daemon |
| a capability | `catalog/capabilities.toml`, both locales | its functions are the world's, written as the world's parser names them — `[method]<resource>.<name>` for a resource's method — and every function of the world sits in exactly one capability or in the `functions` of an `[[ungated]]` interface, beside the reason that covers it: an interface given to every plugin gives only what it lists, so a function the world gains there is refused by the coverage test until someone writes it in, rather than reaching every plugin under a reason written before it. `crates/nc-kernel/tests/capabilities.rs` holds the registry to the world, to the reference plugin's probe (`PROBED` in `plugins/reference/src/behaviour.rs`) and to the capability each host function asks for, so a function the world gains is also a host function in `crates/nc-kernel/src/host/` whose first statement asks for it — or, in an `[[ungated]]` interface, records it with `HostState::ungated` — a row of `PROBED` and an arm of the probe in `plugins/reference/src/adapter.rs`, and, under a capability, a call in `every_gated_function` in `crates/nc-kernel/src/host.rs`, whose test grants one capability at a time and holds what each function asked for, and under which name, to the catalog — the probe cannot, because a method is out of its reach until its constructor is built |
| a rule a plugin's command is held to at `activate` | `crates/nc-kernel/src/registry.rs` | a refusal with the code of its kind — `[NC-PLUG-0008]` for a field left empty, `[NC-PLUG-0009]` for anything else wrong with the declaration, `[NC-KERN-0015]` for a name the plugin may not have, `[NC-PLUG-0029]` for what its entry says — and a unit test in that file; and, where the reference plugin can declare it, a `Variant` in `plugins/reference/src/behaviour.rs`, which the plugin reads from its own store's `activation` key, and a test in `crates/nc-kernel/tests/activation.rs` loading the real component that way. A rule the judge in `crates/nc-schema` can state belongs there instead, where the catalog compiler holds every built-in to it too |
| a plugin's command entry | its row in `plugins/<dir>/commands.toml`, by the name the command has inside the plugin's namespace, and its table in every `plugins/<dir>/i18n/<locale>.toml` | the texts are held at `activate` by `nc_schema::entry::judge`, the judge every command of the catalog is held to — so a rule they need goes there, never into `crates/nc-kernel/src/entries.rs`, which reads the shape alone — and a text identical in both locales is refused unless the row's `same_in_both` gives the reason; the English display name is the title the component declares, so editing one is editing both; no row for a command the catalog describes, and no display name a command of the catalog shows; every file is a build input, so `cargo xtask build --plugins` builds the component again, and the reference plugin's are held both ways to `behaviour::commands()` by `crates/nc-kernel/tests/the_reference_plugin_activates.rs` |
| a refusal the decision point makes | `crates/nc-kernel/src/dispatch.rs` | a code of its own in `catalog/errors.toml` with both locales, a `Reason` — whose sentinel begins `kernel.`, which no policy's `@id` may, and is what the journal records as having decided — with its variant for `kernel.dispatch_refused`, and a case in `crates/nc-kernel/tests/the_decision_point.rs` that dispatches into it through the real component and reads the refusal back from the journal and the log. A denial tells the caller its code and never the policy that decided: that is the journal's. Anything run from inside a host call that measures its own stack — Cedar does — runs on a task of its own (`Frame::dispatch` says why) |
| a rule a plugin's request is held to | `judged` in `crates/nc-kernel/src/net.rs` | a refusal in the order the module header lists the rules, with its `Reason` — logged by its own name in snake case, which the `net.refused` comment in `catalog/log-events.toml` must name and which `Reason::ALL` must hold, as `each_reason_is_logged_by_its_own_name_in_snake_case`, `every_reason_is_logged_by_a_word_the_event_names_and_no_other` and `every_reason_declared_is_one_all_holds` hold — and its code: `[NC-PLUG-0012]` for a domain, `[NC-PLUG-0017]` for a bound, `[NC-PLUG-0013]` for the rest; a unit test in that file, and a case in `crates/nc-kernel/tests/net.rs` sending the reference plugin's request through the decision point, which reads the transport's own record of what reached it. A rule about what is *dialled* belongs to the transport's resolver in `crates/nc-kernel/src/net/ureq.rs` instead, whose tests reach only sockets on this machine |
| a host function that waits | its interface's file under `crates/nc-kernel/src/host/` | its body, after the capability is asked, goes through `supervisor::in_time` under its `interface#function` name with `self.left()`, which holds it to `host_call_deadline_ms` or to what its call tree has left, whichever is nearer, and kills the plugin past it — every host function but `commands.dispatch`, whose command runs under its call tree's deadline and whose waits on the journal the decision point holds to the same bound (`Waits` in `crates/nc-kernel/src/dispatch.rs`) — and a case in `crates/nc-kernel/tests/isolation.rs` makes it never answer, on a paused clock, and reads the kill back as `[NC-PLUG-0006]` naming that function |
| a note a unit keeps | `Note` in `crates/nc-kernel/src/history.rs` | its kind is its `note` field, written as canonical JSON and read back by `Note::read`, the one reader: a note of a kind it does not know, or of a known kind in another shape, is refused as a row the kernel did not write (`NC-DB-0007`), so an undo never acts on a note it misread. A note of a blob made or a request sent is written where it happens (`crate::blobs`, `crate::net`) and read here too. A unit's notes of every kind share `store_notes`' bound, and `every_note_the_history_writes_is_read_back_as_itself` holds each kind's text both ways |
| a call the kernel asks of the store | `StoreBackend` in `crates/nc-kernel/src/backend.rs` | the same method on `nc_local::LocalStore` first — the trait answers as that store does, and is its specification — then on `MemoryBackend` beside the trait, with the same refusals in the same sentences; `crates/nc-kernel/tests/both_stores_answer_alike.rs` runs each script through both and holds the two transcripts equal, and its property test drives both through every operation, in two realms and every kind of unit, so a call the second store answers otherwise fails there — a script or a sequence that never varies what a filter selects on cannot see that filter go |
| a call the kernel asks of the journal store | `nc_journal::Store` in `crates/nc-journal/src/lib.rs` | a **required** method — one provided over `from(0)` would be the slow path, and the blind spot of a row that does not decode, inherited in silence — answered in the same change by all three stores: `InMemory` (`memory.rs`, the specification), `Postgres` (`postgres.rs`, one statement over the columns that bracket a unit, and nothing decoded but the unit each open one names) and `nc_local::LocalJournal` (`crates/nc-local/src/journal.rs`, every statement in that crate), and by every test store that implements the trait — `unit.rs`, `tampering.rs` and the kernel suites' wrappers. Its `# Errors` section names what each implementation builds (`every_errors_section_names_what_its_body_builds`, whose count of sections is written down), the shared scripts of `crates/nc-journal/tests/in_postgres.rs` ask it of the two stores in that crate, and `crates/nc-local/tests/the_local_journal.rs` holds the local journal's transcript to both, over written scripts and generated ones |
| a call the kernel asks of the blobs | `BlobStore` in `crates/nc-kernel/src/blobs.rs` | the same method on `DirectoryBlobs` first — the one a daemon keeps a person's files in — then on `MemoryBlobs` beside it, which `the_two_stores_answer_one_script_alike` holds to one script; the trait is asynchronous because the disk may not answer, so a host function that calls it goes through `supervisor::in_time` like any other that waits, and a writer is moved into a write and handed back, so one a deadline abandons takes its writing with it. What a blob is to the decision point — which unit made it, whether one still holds it — is the store's, in `nc_local`'s `blobs` table, never the blob store's |
| a kind of node a surface shows | `KINDS` in `crates/nc-kernel/src/ui.rs` | the table in that module's header is what a plugin author reads, and changes in the same edit; the kind's keys are a row of `KINDS`, a key holding a person's text is in `TEXTS` and a closed choice in `CHOICES`, so every text of every kind is held to the same bound and characters; and a test in that file keeps a tree of the new kind and refuses each rule it is held to, and a kind a person activates — whose `command` and `args` `surfaces.activate` reads from the kept tree — is a row of `ACTIVE` there too, with a case in `what_a_node_does_not_run_and_what_names_no_node_are_refused` for a node of another kind holding a `command`; **and the desktop's renderer draws it**: a variant of `Kind` in `apps/desktop/src/surface.rs`, in its `ALL`, with the keys it holds and may hold, its reading in `Reading::node` and its drawing in `Drawn`, by a component of the design system's — a word of a closed choice a variant of that choice's `Choice` there — since `every_kind_the_kernel_keeps_is_one_the_window_draws` reads `KINDS` and `CHOICES` from the kernel's file and fails until the window's tables are the kernel's, and a tree holding a kind the window does not read is refused whole; a control is found in the page by its surface and its node's pointer (`hook`), never by the command it runs, which the window neither keeps nor writes |
| a name the system sees | a field of `Layout` in `crates/nc-channel/src/lib.rs`, derived in `layout` and in the table of that crate's header | a line of `parts` in `crates/nc-channel/tests/the_names_are_disjoint.rs`, which destructures `Layout` without `..` and does not compile until the field is read — what it reads is held apart from every other install's name, case-folded; a name the whole machine shares carries the user and is a row of `MACHINE_WIDE` there too, unless it is a checkout's stack's, which is the checkout's; its spelling is written out in `each_name_is_spelled_as_the_crate_header_writes_it`; a relocated install's must carry its namespace and lie under its root, or be `None` as the update feed and the stack are; and nothing outside the crate spells one — `cargo xtask dev` names its stack's project and binds its ports through `Checkout::stack`, and `POSTGRES_PORT` or `LOGS_PORT` only moves a port |
| a restorer's shape | its generator in `crates/nc-platform/src/service.rs` — `systemd_unit`, `launchd_plist` or `task_xml` — and the table of restorers in that module's header | the generator's text is written out whole in the module's tests, and every character its format reads specially is escaped there or refused when the service is described; what each restorer does is measured, not assumed, so a change to one is measured again on its system's runner by the scratch services in `with_the_manager` in the same file — the test binary registered under this checkout's test prefix, killed, and read back through the manager's own counters — and `NOCTUA.md §17.3` says what each system's restorer is |
| a way a system delivers the Leader, or a change to one | `crates/nc-platform/src/leader.rs` for what is decided — which delivery a session has, which key and modifiers, what a refusal means — and the system's half for the call alone: `x11.rs`, `windows.rs` or `macos.rs` | the halves decide nothing, so the Linux job tests and mutates every decision: a flag, a code or an error a system names is a function or a constant of `leader.rs` with its unit test and its operator in `xtask/mutants.py`. A registration takes a `Leader`, reports presses alone, and gives the chord back when dropped; a chord another program holds is `Refused::Taken`, on every system; and a registration that stops delivering while it is held tells its caller so through `on_lost`, which X11 alone has cause to call — a path that ends the reading thread without telling is a defect, whatever ended it. `unsafe` is written in `windows.rs` and `macos.rs` and nowhere else (invariant 17), each block under its `// SAFETY:` comment; `x11.rs` is safe Rust, every request it sends held to its bytes by a test, every length it reads held to a bound before it is read, and its connection driven end to end by the server written beside its tests, on every system, and on Linux by a real `Xvfb`, which `this_system_grants_the_chord_hears_it_and_gives_it_back` starts and never stands aside for. **No test of a half is written under a `cfg`**, which the gate reads as a test hidden: the session is generic over its connection so every system compiles and runs its tests, the one test that asks the system itself branches on `clipboard::SYSTEM` at run time, and a test that needs a socket that is a file asks `served::sockets` — the one module of helpers written per system, holding no test — which answers nothing where there is none, the test then holding that system to its own refusal. Every wait of a session is a field of `session::Pace`, so a test shortens one rather than sleeping through it, and a test of a registration that might never return asks on a thread of its own and fails by its wait, since `xtask/mutants.py` plants exactly that. Wayland has no registration: its delivery is the compositor's binding running `noctua leader`, and the GlobalShortcuts portal is not built (**What is deliberately not claimed**). Which process of an install asks for the Leader on each system is `nc_ipc::leader::listener_on`'s — the daemon on Windows (`apps/daemon/src/leader.rs`), the desktop interface on Linux and macOS, on its event loop's thread (`listened` in `apps/desktop/src/host.rs`) — every press is `nc_ipc::leader::TAP` dispatched as the person, and every refusal is `nc_ipc::leader::refusal`'s code, told rather than dropped; a listener changed for a system is a row of that module's header table, of `the_daemon_listens_on_windows_and_the_desktop_elsewhere`, and of `NOCTUA.md §12.8`'s table. `NOCTUA.md §12.8` says what each system does, and changes in the same edit |
| a kernel limit | `[kernel]` in `catalog/limits.toml`, and a field of `KernelLimits` in `crates/nc-catalog-compiler/src/model.rs` with its row in `KernelLimits::each` | every limit is required and above zero, and one that bounds another is held to it where both are written, with a case in `crates/nc-catalog-compiler/tests/the_gates_fire.rs` at the boundary (`check_kernel` in the compiler's `load.rs`, `check_kernel_interface` for the interface's, and `check_daemon_ipc` for one a frame carries — `commands_per_plugin` × `entry_bytes` × 2 at most half of `ipc_frame_max_bytes`); `xtask/tests/the_registry_reaches_the_code.rs` compares every field by name, so a new one is a line there as well, and `docs/limits.md` publishes it with its unit. A plugin's manifest may lower only what `Lowering` in `crates/nc-kernel/src/limits.rs` names, and never raise it |
| an IPC message | `crates/nc-ipc/src/message.rs` — a variant of `Request` or `Answer`, or a message of the handshake — and the module header of `crates/nc-ipc/src/lib.rs` | its encoding and its reading in the same file, the reading **strict**: every field it holds taken by `Fields` and `done()` refusing the rest, so a field an older build does not read is a refusal rather than a value dropped in silence; a message the handshake reads names the protocol first, and a change of what an existing message means is a new `PROTOCOL`, never the same number, as is a request an older daemon would read as malformed — with one exception, written in the crate's header where it is used: **a protocol no release carries may gain a message, or change what one of its own means, under its number until the Stage that builds it closes**, as 4 does while Stage 16 is built, and what that costs is a development daemon installed from an earlier commit of it, which passes the handshake and then closes the connection at the first message it does not read — or reads one as that earlier commit meant it, a note held rather than applied, a watch answered without the interface's state — until `cargo xtask dev --install` replaces both ends; a request is an arm of both matches over `Request` in `apps/daemon/src/server.rs` — the person's `answer`, and `a_sessions`, which refuses a session everything but its tools and its dispatches — and of the test over every request beside them, and of `unread_as` in `crates/nc-ipc/src/client.rs` when it is an answer; a case in `crates/nc-ipc/tests/` that sends it through a real `serve` and `connect`, and one that sends it malformed by hand through `nc_ipc::frame`. Whatever the daemon answers is held to `ipc_frame_max_bytes`, which the compiler holds above the kernel's `payload_bytes` and the envelope of 64 KiB `crates/nc-catalog-compiler/src/load.rs` names, so an answer that may grow without bound — the registry, a status — is refused to the daemon before anything of it is sent (`NC-IPC-0016`), its request still waiting for an answer the daemon can send instead, and never a frame cut short |
| a daemon limit | `[daemon]` in `catalog/limits.toml`, and a field of `DaemonLimits` in `crates/nc-catalog-compiler/src/model.rs` with its row in `DaemonLimits::each` | every limit is required and above zero, and one that must keep a bound outside its table is held to it where it is written (`check_daemon`, `check_daemon_ipc` and `check_daemon_interface` in the compiler's `load.rs`, with a failing case in `crates/nc-catalog-compiler/tests/the_gates_fire.rs` at the boundary): `mark_lease_ms` below the day past which a minter reads a mark as corrupt, `ipc_frame_max_bytes` at least the kernel's `payload_bytes` and the frame's envelope, `ipc_silent_after_ms` above two `ipc_heartbeat_ms`, `ipc_connections_max` at most 252, what a Windows pipe's instances hold with one waiting and one past the bound, `sessions_max` below `ipc_connections_max`, so the person always finds a connection no MCP client holds, and what an interface asks beside the channel's own bounds (`check_daemon_interface`): `ipc_watch_wait_ms` below `ipc_silent_after_ms`, so a watcher that went away keeps its connection no longer than a silent client keeps one, `ipc_note_max_bytes` with the frame's envelope at most `ipc_frame_max_bytes`, and as many values of `ipc_display_value_max_bytes` as an announced environment names variables (`DISPLAY_VALUES`), every byte written as two, with the envelope at most a frame as well, and the schedule beside the kernel's bounds (`check_daemon_schedule`): `pump_every_ms` at most `timer_min_interval_ms`, `sweep_every_ms` at most `blob_orphan_grace_ms`, and `stop_drain_ms` at least `dispatch_deadline_ms`; and the CLI's process control beside the daemon's own (`check_daemon_control`): `start_within_ms` above `ipc_handshake_ms`, and `stop_within_ms` at least twice `stop_drain_ms`. `xtask/tests/the_registry_reaches_the_code.rs` compares every field by name and counts them, `xtask/tests/every_generated_value_is_compared.rs` counts the generated surface, and `docs/limits.md` publishes it under **Daemon** with its unit, whose prose `xtask/tests/the_pages_reach_the_catalog.rs` pins. The daemon, `crates/nc-ipc` and the CLI's process control read `nc_catalog::DAEMON` and take no value of the type from a caller |
| a precompiled file let through the audit | `[bans.build] bypass` in `deny.toml`, and `THE_EXECUTABLES_LET_THROUGH` in `xtask/src/check.rs` | only one this tree cannot build without and has read: one version of one crate, the file's path in it and the SHA-256 of its bytes as the registry serves them, argued above `[bans.build]`; the gate holds the list to the constant, in its order, and refuses an entry holding anything else — a glob (`allow-globs`), a build script let through whole, a file with no checksum or with another key — so the change that moves the lock past the crate rewrites both, as cargo-deny's own `checksum-mismatch` makes it |
| an advisory left out of the audit | `[advisories] ignore` in `deny.toml`, and `THE_ADVISORIES_LEFT_OUT` in `xtask/src/check.rs` | only one the lock cannot get past and that is argued not to reach this tree: the entry's `reason` says in one sentence why and when it leaves, the paragraph above `ignore` gives the argument, and the bullet of **What is deliberately not claimed** it rests on says so. The gate holds the list to the constant, in its order, and refuses an entry with no reason however TOML spells it; cargo-deny warns `advisory-not-detected` for an entry no crate matches while `[advisories] unused-ignored-advisory` is `warn`, which `deny.toml` writes out and `THE_AUDIT_IS_NOT_NARROWED` holds, and the warning fails the dependency step, so the change that moves the lock past the advisory takes it out of both lists |
| a CLI verb | `VERBS` in `apps/cli/src/args.rs`, its line in `what_it_does` in `apps/cli/src/help.rs`, and its arm in `apps/cli/src/run.rs` | one undotted word, which no registered command's name can be — every one is dotted (`nc_schema::command_name_is_well_formed`) — and `the_verbs_and_the_registered_names_never_meet` holds the two apart, over the catalog's and the reference plugin's names; a verb is what the CLI does itself rather than a command it dispatches — help, process control, and choosing the development checkout `noctua-dev` reaches, each acting without dispatching, `mcp`, a verb that dispatches because it is a transport: it carries an MCP client's calls to the daemon as one agent's session, and a command is what a transport carries, so no registered command could be it; and `leader`, the other verb that dispatches, because what it carries is where it runs: it announces the graphical environment a compositor's binding runs it in, which no command's arguments could say for it, then dispatches `leader.tap` (`nc_ipc::leader::TAP`) — or, with `--announce`, nothing, as the unit the person's graphical session starts runs it on Linux (`nc_cli::install`) — the one command a press of the Leader is wherever it is heard — never a feature a registered command could carry, which reaches every surface (`NOCTUA.md §2.8`). A verb whose standard streams carry a protocol says every refusal of its line on standard error, `--json` or not (`renders_json` in `apps/cli/src/run.rs`). A run that ends in a new way is a row of `Exit` in `apps/cli/src/exit.rs`, and `help exit-codes` prints that one table, which `docs/cli.md` publishes beside the verbs. A help topic is the same kind of word — `exit-codes`, `categories`, `external-identifiers`, `keybindings` — read in `topic` before a command's name is, and `the_registry_topics_are_undotted_and_never_a_registered_name` holds a registry's topic apart from every name |
| an MCP method | `Engine::heard` in `apps/cli/src/mcp.rs`, and a unit test in that file answering it | a method is answered only when this server offers what it belongs to, and every other is `-32601`; a capability is never declared for a feature that sends the client a request — elicitation, sampling, roots, tasks — because what a client answers would be read as the person's; a call's fields are a closed set, every other refused with `-32602`, and no field is ever read as a confirmation; every answer is one line through the session's one writer; and a protocol revision joins `REVISIONS` only once a saved schema of it shows every answer to a request whose id was read valid in it, so a revision this server does not speak is answered in its newest and the client declines it — an error answering a line whose id could not be read carries no `id`, which `2025-11-25`'s schema reads and `2025-06-18`'s does not |
| an MCP tool | nothing: a tool is a command (*a command*, above) | `noctua mcp` lists what its session's agent may dispatch now, as the daemon answers it, each entry projected by `nc_schema::entry::as_tool`, and a call is sent as the command its name reads back to, so a tool arrives with its command and an agent reaches it with its feature's agent permit and its person's grant (*an agent permit*); no list of tools is written anywhere, and `docs/mcp-tools.md` is the catalog's projection, not a session's |
| a page about what the catalog cannot see | a field of `Surfaces` in `crates/nc-catalog-compiler/src/surfaces.rs`, built by `surfaces` in `xtask/src/build.rs` from the crate that holds it, and the page in `docs::all` in `crates/nc-catalog-compiler/src/docs.rs` | the compiler links neither `nc-cli` nor `nc-channel` — both read the crate it generates — so what it publishes of them crosses as a field typed through `nc_schema`, and never as a command: every command a page shows is the catalog being compiled, and a function a field hands it, as `Cli::described` is, reads only what it is handed (`the_pages_read_the_catalog_they_are_handed_and_not_the_one_xtask_links` holds the pages to the catalog on disk). A part left empty is refused (`surfaces-incomplete`) and named in `incomplete` beside the type. Every file a field is built from is a row of `EMITS`, `EMITS_UNDER` and `BY_HAND` in `xtask/src/determinism.rs`, and its needles are that gate's; the page's `Source:` line names them, so `each_page_names_the_registry_it_came_from` checks each exists; and `PAGES`, `PROSE` and `HEADINGS` in `xtask/tests/the_pages_reach_the_catalog.rs` hold its prose and tables, a test there holding each value to the crate it came from. `a_hand_edit_to_any_generated_page_is_drift` reaches the page with no edit, because it reads every page the compiler emits |
| a restorer — anything that starts a channel's daemon on its own | its registration, or its code, starting the daemon as `noctua-daemon --restored` | that one argument, and never the bare daemon: started so, the daemon ends at once, and well, while the `stopped` mark is there or another daemon of the channel runs (`nc_daemon::run::restore`), so no restorer reads the mark itself and no stop has to know of one (`NOCTUA.md §17.5`). The service manager's is the installer's — `nc_cli::install` registers it under the name `restorer_name` gives it and keeps its definition where `kept_in` says, a test's where no login reads it on systemd and launchd, while a test's task on Windows starts at each logon until it is unregistered — from the plan `development_plan` makes, which `cargo xtask dev --install` and the suite below both install, and a start through it is `nc_cli::control`'s; `apps/daemon/tests/the_stop_mark_silences_every_restorer.rs` holds it to restoring a daemon killed, to registering it again and starting it through it when installed over a daemon that runs, to keeping a person's stop when installed over one, and to leaving it down after a stop and after a kill once the mark is written, through the manager's own counters, so a new kind of restorer is a leg of that suite before it ships. The interface's countdown is the second kind, decided in `nc_ipc::alert` — `Run::restorer`, the install's own daemon with `nc_ipc::alert::RESTORED`, which `the_daemon_and_initialize_are_answered_and_nothing_else_is` holds to the daemon's reader — and run by the desktop interface, which is not built yet: its leg in that suite, `the_interfaces_countdown_restores_a_daemon_lost_and_leaves_a_stopped_one_down`, asks the module as the desktop will, by the clock until its countdown ends, and runs what the line answers, over the suites' own install. A program that reads the mark to say something — the interface's alert line, an install — reads it through `nc_channel::stopped::on_purpose`, the one reader, and still restores only through `--restored`, so a mark written after it read one holds the daemon down all the same |

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
effect; 22 is gated over the statements written today and by nothing over the next; 25 is
held by its tests, and by no gate over a signal sent another way; 26 is held by its tests,
and by no gate over a rule written again beside the judge; 27 is held by its tests, and by
no gate over a table a function the compiler is handed reads of its own; 28 is held by its
tests, and by no gate over a path to Cedar written beside the decision point's.

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

17. **No `unsafe` is written in this repository but in two modules of one crate — and
    that is what `forbid(unsafe_code)` means here, exactly.** Every member takes
    `unsafe_code = "forbid"`, the plugins included, but `crates/nc-platform`, whose
    Windows half makes the Win32 calls `windows-sys` declares `unsafe` — the security
    calls, the system's random bytes, the processes at either end of a named pipe, when
    this process started, whether its standard handles reach a child, and the Leader's
    hotkey — and
    creates the daemon's pipe through `tokio`'s `create_with_security_attributes_raw`,
    which is `unsafe` for the descriptor it is handed (decided on 2026-11-02,
    `NOCTUA.md §2.7`), and whose macOS half makes Carbon's hotkey calls, six C functions
    it declares itself, for the Leader alone (decided for Stage 16, `NOCTUA.md §12.8`): a
    `forbid` cannot be lifted by anything
    under it, so that crate writes the workspace's lints out with `unsafe_code = "deny"`
    and allows it on `crates/nc-platform/src/windows.rs` and
    `crates/nc-platform/src/macos.rs` alone, each by one attribute on its
    `mod` line. **A third file is a decision, never an edit**: the Unix half and the X
    server's client beside them write none. Every `unsafe` there sits under the
    `// SAFETY:` comment — or, on an
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
    and strings blanked, which names `windows.rs` and `macos.rs` as its two exceptions
    (`WRITES_UNSAFE`), fails once either writes none, and holds each `unsafe` in them to
    its comment; and
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
    and the SQLite floor. The file's directory is made private to this user by the call
    that makes it, and a name already there is refused rather than reused or cleared, so
    the gate's SQLite reads no database or `-wal` another account wrote. A source gate,
    over every member whose manifest takes SQLite — `sqlx`'s `any` feature included, a test
    holding the gate's list to them — refuses any statement that builds or changes the
    schema, attaches a database or touches `writable_schema`; undoes the connection's
    hardening, through SQL, the pragma door or `sqlx`'s own option, every value judged as
    SQLite reads it and the list held both ways to the pragmas `crates/nc-local` sets; sets
    a pragma under a key no literal reads as one; copies the file out with `VACUUM` or
    points SQLite's files at a directory; loads an extension; gathers the planner's
    statistics (`ANALYZE`, `PRAGMA optimize`, `optimize_on_close`); sets `application_id`
    or `schema_version`; or writes `_sqlx_migrations`. A `#[cfg(test)]` arm, field or item
    hides only itself: the region closes at the comma, semicolon or brace that ends what
    the attribute governs.

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
    entry landed; the pruner's deletions and a change to a person's agents, which no unit
    can stage, are journaled as allowed — the prune once a dry run was refused nothing, the
    change once the directory rehearsed it (`changed_once_journaled`) — before anything is
    deleted or changed; a close
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
    left open, an agent's change whose entry the journal refused, a journal whose store cannot name a unit it holds open, and a kernel stopped
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
    `crates/nc-local/src/store.rs`, `crates/nc-local/src/journal.rs` and
    `crates/nc-local/src/agents.rs` write with goes through `write_exactly`, which refuses any count but the one the statement had to
    change (`NC-DB-0007`, and the journal's `NC-JRNL-0009`), every transaction first checks
    that no temporary object sits on its connection, and the pruner's SQLite target refuses
    a `DELETE` that removed more than its batch (`crates/nc-prune/src/sqlite.rs`). **Gated
    over the statements written today**:
    `a_write_a_trigger_silences_is_refused_and_leaves_nothing` in
    `crates/nc-local/tests/a_unit_lands_whole_or_not_at_all.rs` silences each of the store's
    in turn and requires the write refused with nothing left, and
    `an_append_a_trigger_silences_is_refused_and_lands_nothing` in
    `crates/nc-local/tests/the_local_journal.rs` the journal's, and
    `a_write_to_the_agents_a_trigger_silences_is_refused_and_leaves_nothing` in
    `crates/nc-local/src/agents.rs` each of the four the agents are written with. Each list is its own, so a new write that
    skips `write_exactly` and is not added there is refused by nothing — no gate reads the
    store's source for an `execute` outside it.

23. **The daemon has no network listener.** What a daemon serves, it serves at its
    endpoint — a socket or a pipe its user alone reaches (`nc_platform::ipc`) — and
    nowhere a network can: no `TcpListener`, `TcpSocket` or `UdpSocket` is written under
    `apps/*/src`, `crates/nc-ipc/src` or `crates/nc-platform/src` — every app's sources,
    their tests among them, and those of `nc-ipc` and `nc-platform`, which its endpoint and
    protocol are built from. **Gated** by `the_daemon_has_no_network_listener` in `xtask/src/gates.rs`,
    over those sources with comments and strings blanked, and by
    `the_listener_walk_goes_everywhere_inside_its_scope`, which plants a listener in every
    directory the tree has and holds the walk to finding each one in its scope and none
    outside it. **Not held**: a listener written in any other crate of this tree the daemon
    is built from — `nc-kernel`, `nc-log`, `nc-local` and the rest its graph reaches, whose
    sources this gate does not read, and whose own tests open listeners on this machine —
    which only review keeps from the daemon's run; and a listener a dependency opens on the
    daemon's behalf, which no source here names. The kernel's transport dials out and
    listens nowhere.

24. **No code outside `nc-platform` decides by the system it is built for**
    (`NOCTUA.md §2.7`): a `cfg`, `cfg!`, `cfg_attr` or `cfg_select!` arm naming `unix`, `windows`,
    `target_os`, `target_family`, `target_vendor`, `target_env` or `target_abi` — every
    part of a target but its architecture and its pointer width — is written in
    `crates/nc-platform` alone, which states each system's answer as a fact every system's
    code reads, so the decision is code the Linux job compiles, tests and mutates. The gate's own lines are the exception, each
    written down by its file and its text (`THE_GATES_OWN` in `xtask/src/gates.rs`):
    `check::RUNS_THE_STACK`, and two tests holding that answer to the system they run on.
    **Gated** by `no_system_is_asked_outside_the_platform`, over every Rust source of every
    member tree with comments and strings blanked, the predicate read inside the
    parentheses an attribute takes or whichever delimiters a macro is called with —
    `cfg!(…)`, `cfg![…]` and `cfg!{…}` alike, a path before it or none — and each arm of a
    `cfg_select!` read before its `=>`, and by
    `the_system_walk_goes_everywhere_outside_the_platform`, which plants the attribute,
    the arm and `cfg!` in brackets. **Not
    held**: a manifest's `[target.'cfg(…)']` table, which only `nc-platform`'s holds today;
    a `cfg` by architecture or pointer width, which names no system — the reference
    plugin's `target_arch = "wasm32"` among them; `cfg!` or `cfg_select!` renamed by a
    `use … as`, or handed its predicate by a macro of this tree's, where no predicate is
    written beside the name; and the system asked another way — `std::env::consts::OS`
    compared at run time — which is a decision this gate cannot read.

25. **Nothing unverified is signalled, and no process numbered zero or below.** A
    process the CLI's process control signals is one `nc_platform::process::Verified`
    holds to the daemon's PID record — its boot, its start, its executable and its user,
    as the system shows them — and the signal goes through what held it: a pidfd on
    Linux, one process handle on Windows, the process read again just before it on macOS.
    A stop asks the daemon through its endpoint first and signals only one that does not
    answer: one that answers it is stopping is waited for and never signalled
    (`NC-CLI-0008` when it outlasts the wait), and so is one whose record, read again once
    nothing answered, no longer names it — a daemon removes its record at the end of its
    own stop, before its endpoint goes. A record naming a process that runs no
    more — judged before anything else, so whoever holds its id now — or a process this
    user may not read at all, which is not its daemon (on Windows, another account's, for
    an account that is not an elevated administrator), is a daemon already stopped while
    nothing holds its endpoint; one naming another process, one that does not read, and an
    endpoint something holds with no record to hold it to a process are never signalled
    (`NC-CLI-0009`).
    `nc_channel::record` refuses a record naming process zero or below, and
    `Verified::open` refuses process zero. **Enforced by**
    `apps/daemon/tests/a_recycled_pid_is_never_signalled.rs` — four stale records, a
    sleeper still running after each, and the sleeper's own record the control a stop
    does end — by the tests of `crates/nc-platform/src/process.rs`, of
    `apps/cli/src/control.rs` and of `crates/nc-channel/src/record.rs`, by
    `a_stopped_runs_record_is_removed_before_its_endpoint_is_let_go` in
    `apps/daemon/src/run.rs`, which holds the order a stop asked again reads, by
    `a_daemon_whose_record_went_while_a_stop_asked_it_is_waited_for_and_never_signalled`
    in `apps/cli/src/control.rs`, which holds the stop's reading of it, and by the
    operators over those sources. **Not held**: macOS's moment between the last reading
    of a process and its signal; and a signal sent another way, which no gate reads the
    CLI's source for — every one this tree sends is a method of `Verified`, and
    `crates/nc-platform`'s halves are where the system's calls for them are made.

26. **A command's texts are judged by one judge, wherever they are written.** The
    catalog's commands at build time and a plugin's at `activate` are held to
    `nc_schema::entry::judge`, and to `[kernel] entry_bytes`, the one bound in bytes the
    compiler and the kernel both read from the catalog. The compiler drops from the
    judge's answer only what its message rules already report, and the kernel restates
    none of its rules: `crates/nc-kernel/src/entries.rs` reads a plugin's files for their
    shape alone. **Enforced by** the unit tests of `crates/nc-schema/src/entry.rs`, by
    `a_commands_texts_past_the_entry_bound_fail_the_build` in
    `crates/nc-catalog-compiler/tests/the_gates_fire.rs`, by the unit tests of
    `crates/nc-kernel/src/registry.rs`, a plugin's entry past the same bound among them,
    and by `crates/nc-kernel/tests/activation.rs`, where the real component is refused for
    a text its entry misses. **Not held**: a rule written again beside the judge, in the
    kernel or the compiler, which a review sees and no gate does.

27. **Every page comes out of the one compilation, and help and its page say one thing.**
    Every file under `docs/` but `docs/README.md` is emitted by
    `nc_catalog_compiler::compile`, the pages about what the catalog cannot see among them:
    the CLI's grammar, verbs and exit codes and each channel's default keys cross as a
    `Surfaces`, which `cargo xtask` builds from `nc_cli` and `nc_channel` and the compiler
    links neither of, and every command a page shows is the catalog being compiled. Help on
    a command and `docs/cli.md` are laid out by one function, `nc_cli::help::described`, which
    reads only what it is handed, and a command's MCP tool is projected by one,
    `nc_schema::entry::as_tool` — its name `nc_schema::entry::tool_name`'s, each dot an
    underscore, read back to the command by `nc_schema::entry::command_of_tool` alone, and
    its one annotation, `readOnlyHint`, from whether the command is `mutating` — the one
    function `noctua mcp` lists a session's tools with too.
    **Enforced by** `a_hand_edit_to_any_generated_page_is_drift`
    in `xtask/src/check.rs`, over every page the compiler emits and every page committed;
    `the_pages_read_the_catalog_they_are_handed_and_not_the_one_xtask_links` in
    `xtask/src/build.rs`; and `the_cli_page_says_what_help_says` and
    `the_mcp_page_projects_every_command_as_a_tool_and_says_what_a_session_sees` in
    `xtask/tests/the_pages_reach_the_catalog.rs`, which reads every tool's name back to its
    command. **Not held**: a table a function in a
    `Surfaces` reads of its own beyond the catalog that test changes, which a review sees
    and no gate does.

28. **An agent is decided as itself and as each one it acts for, at the moment of each
    decision, and is never confirmed.** Every dispatch of a call tree that acts for an
    agent, at its top and nested — and every authority its tree put off, a timer or an
    event, decided again when it comes due — is asked of the decision point once for each
    link of its chain — the agent first, then its person — with one moment, one epoch,
    what its session has spent and the kernel's own `via_agent` and depth, and runs only if
    every link allows. The chain is the
    directory's answer when the kernel opens the session (`Kernel::session`), for the
    person who made the agent alone, and never a caller's. Every entry of the tree names
    the agent and, in its actor chain, whom it acts for, with one segment of what decided
    for each link reached, joined by `;`; a person's entry is what a one-principal decision
    writes. No confirmation or approval is redeemed in an agent's tree, whatever it holds,
    and no compensation is registered there.
    **Enforced by** the agent tests of `crates/nc-kernel/tests/the_decision_point.rs` and
    `crates/nc-kernel/tests/the_rest_of_the_world.rs`, the unit tests of
    `crates/nc-kernel/src/session.rs`, `crates/nc-kernel/src/dispatch.rs`,
    `crates/nc-kernel/src/host/context.rs` and `crates/nc-kernel/src/host/commands.rs`,
    `an_operator_frozen_mid_session_freezes_their_agent_at_its_next_call` in
    `crates/nc-kernel/tests/units_and_history.rs`, and
    `no_member_turns_a_test_door_on_outside_its_tests` in `xtask/src/gates.rs`. **Not
    held**: a same-user process dispatching as the person without naming an agent, which
    the intersection never sees.

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
| A person's permit for a feature of the platform borrowed by a plugin, a file's name that is a path, or a caller's text turned into a logged identifier | `feature:` refused in a plugin's declaration at `activate`, and an argument naming `Feature` refused in every declaration; one builder and one parser, every name one path component; and the kernel handed text only | "A file's name is proposed and read by one function, decided and journaled like any command" |
| A keybinding that reaches what the command list could not, or reads two ways | a binding is a key of a closed set in one of three tables, written one way and resolved to a command's name, which dispatch holds as it holds any | "A keybinding is one of three forms, written one way, and names one key and one command" |
| Another account choosing a person's keys, or one channel's choice taking another's Leader | the file and its directory judged private before they are read, a bounded closed shape read by the one grammar, each part refused apart to its defaults, and the other channels' files read in one order and never written | "A person's keybindings are read only when private, refused part by part, and bind nothing but command names" |
| A query that costs the daemon unbounded work | a query bounded by its schema and by the core alike, every field read to its bound, the registry's lock let go before anything is matched, and an answer cut to its limit, whose widest fits one frame | "A command is found by its names and tags in bounded work, decided and journaled like any command", and "A query is matched in bounded work, and an item's text is read up to its bound" |
| A plugin that will not stop | the epoch, fuel, memory, a host call's deadline, the tree's deadline, depth and count, the refusal of re-entry, bounded queues, timers, surfaces and log, the restart budget and quarantine | "A plugin that crashes, hangs or exhausts its budget is stopped, and harms nothing else" |
| Half-applied state | writes staged and committed whole, recovery each time a kernel opens, and an undo that is a unit of its own | "A command runs only when it is decided, and the journal hears of it first", and "A person undoes their own steps, with the authority they hold now" |
| A request to somewhere it should not go | names the manifest declares, public addresses only, no redirect and no proxy, bounds, and only `GET` and `HEAD` where nothing changes | "A plugin reaches the network its manifest names, and nothing else" |
| Malformed input | canonical JSON and the closed schema, before the plugin runs | "A command runs only when it is decided, and the journal hears of it first" |
| A decision point that cannot decide | a denial, journaled | the same entry |
| An action that escapes the audit | journaled before its answer is returned, and its effects gone when the journal did not take it | invariant 20 |
| User data in the central log | typed events only, and a plugin's text kept in its own ring | "A plugin's own log stays in its own ring" |
| Undo used to escalate | the asker's own steps, authorized again now, refused on a conflict, compensations dispatched through the decision point | "A person undoes their own steps, with the authority they hold now" |
| A rule swapped under the kernel | packs compiled into the binary, and a realm's pin held to them | "The pack a kernel decides with is the one it was built with" |
| An agent named by text it was not made under, given or ended by anyone but its person, or granted past the feature it names | an agent routed by membership of the identifier the machine minted, never parsed; only a person of the realm makes one and only its person grants or ends; a grant's scope compared whole; a change kept before it is held, an ending held first; both tables bounded | "An agent is held under the identifier the machine minted, for its person alone, and a grant answers for one feature" |
| Authorization gone stale | decided again on every dispatch, and when a timer or an event comes due | "What a plugin puts off runs only under an authority decided again when it comes due" |
| A history rewritten on the person's machine | append-only triggers, a row only at the next sequence, the link inside the insert, every row read back only as this code writes it | "The journal on the person's machine refuses to be edited, and says which units are open" |
| Another account reading the local database | `0700` and `0600` on Unix, an access list naming the user alone on Windows, owner and link checks, `trusted_schema = OFF`, `main.`-qualified statements and append-only images | "The local database is private to its user, and refuses what it did not make" |
| Another account reaching the daemon's endpoint, or standing in for it | a socket `0600` in a runtime directory judged `0700`, or a pipe made first with an access list naming the user alone and remote clients rejected; the user and the process at the other end read off the system both ways, before a frame crosses | "A daemon's endpoint lets in its user alone, and each end reads the other off the system" |
| Something that never read the token speaking to the daemon, or answering for it | a proof keyed by the token at each end, over both nonces and a domain of its own, compared in constant time, with the token never sent | "Each end of a connection proves it holds the token, and neither sends it" |
| An identifier handed out twice, across a crash or a clock moved back | a ceiling made durable before a millisecond past it is handed out, and a start that resumes above it and the newest unit its local database keeps, waiting for the clock | "A daemon starts only on a profile that is its user's alone, and mints nothing it cannot resume above" |
| Two daemons over one profile, a network reaching a daemon, or a request reaching the kernel unproven | an instance lock taken at once or refused, no listener a network can reach, and every connection proven before a request is read, each refusal logged | "A daemon serves its kernel to its user alone, one to a profile, and to nothing a network reaches" |
| A sandbox escape through this repository's own code | no `unsafe` written anywhere but the Win32 calls of `nc-platform`'s Windows half and the Carbon hotkey calls of its macOS half, each beside its reason, plugins included, and what `forbid` does not reach written down | invariant 17 |
| Something that is not this install's daemon answering the CLI, or the CLI printing what a terminal acts on | the endpoint's other end read off the system, the token read only when private, the daemon's proof, the welcome held to the process and the channel, and every text from the daemon escaped | "The CLI sends nothing until both ends are proven, and prints nothing a terminal acts on" |
| A stop reaching a process that is not the daemon, or a restorer starting a daemon a person stopped | the daemon asked first, a signal only through the process its PID record is held to, and the `stopped` mark written durably before anything is asked and read by the daemon's own restored start | "The CLI starts and stops its own channel's daemon, and signals nothing it did not hold to its record" |
| An agent doing what its person may not or what it may not itself, or acting for someone the directory did not name | the session opened by the kernel from the directory's answer, for the asking person alone; every decision of its tree asked once for each link at one moment and one epoch, the first denial the answer; each entry naming the agent and whom it acts for | "An agent acts as itself and as the person it acts for, and is never confirmed" |
| An agent obtaining or presenting a confirmation or an approval | a session with no slot for one, a tree acting for an agent binding none, none ever bound to an agent, and an agent named without a session deciding as itself, which the core refuses | the same entry |
| A chain of agents deeper than the policy's two | the chain written by the kernel alone, bounded by its type above the policy and refused by `core.chain_depth_is_bounded`; its one longer constructor a test door no member turns on outside its tests | the same entry |
| An agent past its budget, or acting on through what its tree put off — a timer, an event, a compensation — once its person's or its own authority is gone, or as its person alone | a session's count, spent under the kernel's bus for every call, firing and delivery and read by the core's budget rule; a timer and an event carrying the session, decided again for every link when due; a compensation refused in an agent's tree; its steps kept in its person's history | the same entry |
| An agent made, granted or ended in a person's name by anything but their own call, or a change to it the audit never saw | four commands of the kernel's own decided by a person's own permit (`principal is User`), the three that change run only at the top of a call tree, and each change rehearsed, journaled as allowed with what it makes and only then made | "A person makes, grants and ends their agents by their own calls, each journaled before it is made" |
| A plugin, a timer or an event switching a setting in a person's name, or an agent or a device letting agents in | the setting's commands decided by a person's own permit (`principal is User`), and the switch run only at the top of a call tree, refused nested before it is decided | "A setting is switched by a person's own call, and MCP is off until one switches it on" |
| A client acting as an agent while MCP is off, past the sessions the daemon holds, for an agent its person did not make, or asking for what only the person asks | a session opened in order — the kernel running, a place among `sessions_max`, MCP on, the kernel's own opening for the person's agent — each refusal closing its connection; the connection bound once; only its tools and its dispatches answered; MCP read again before each request, whatever it asks, and off ending the session | "An agent's session opens only while MCP is on, for an agent of the profile's person, and asks for its tools and its dispatches alone" |
| An MCP client confirming for the person through a field of a message, or having the server ask them | a call's fields a closed set and `_meta` never read, no capability declared that lets a server ask, and no request ever written to the client | "`noctua mcp` writes nothing but the protocol, sends every call that names a command, and confirms nothing" |
| A call to a tool the session was not shown | sent and decided as its agent's like any other, spent and journaled, and its refusal written as an absent tool's | the same entry |
| A result or a tool's description hiding text from the person | every answer, refusal detail, line and paragraph written with each character a reader sees nothing of escaped | the same entry |
| Standard output carrying anything but the protocol, or a silent client losing its connection | one writer through one door, each answer one line; every refusal before a session on standard error; every read and write off the runtime's thread, so the heartbeat runs | the same entry |
| A tool reached under a second spelling of its name, or marked as changing nothing when it can | a tool's name is its command's with each dot an underscore, read back to that one command by the command-name parser and never from a dotted name; its one hint from `mutating`, which the kernel holds a command to | "A tool is named as its command and read back to it alone, and its one hint is one the kernel holds" |
| A name on a person's `PATH` that runs what another account chose, or a file of the person's replaced by an install | one name written outside Noctua's directories, in a bin directory judged this user's and written by this user alone, replacing only a link or a launcher to a checkout's CLI of its own bases, and a launcher's pointer read only when private | "`noctua-dev` is the one name an install writes outside its own directories, and it replaces nothing it did not make" |

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
schema declares, a resource type that action applies to, the whole realm selected only
by a command that reads it, since a permit for a realm-wide change would authorize every
plugin's, and never a feature of the platform — not by `feature:`, and not through an
argument naming `Feature`, which the judge refuses in every declaration (`[NC-PLUG-0009]`)
— whose permit is a person's for the kernel's own commands. Its name sits in the plugin's own namespace, never
one the kernel keeps — a row of `RESERVED` in `crates/nc-kernel/src/registry.rs`, which holds the
kernel's own and the interface's, `window.` and `page.` among them — declared once, and matches the
catalog where the
catalog describes it. Each of its own is shown by its entry, a row of the plugin's
`commands.toml` under the name it has inside the namespace, judged as the next entry says
(`[NC-PLUG-0029]`); a command the catalog describes is shown by the catalog's entry
alone. A namespace and an identity belong to one loaded plugin at a time,
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
command each way the reference plugin can, and described by entries that miss a command
or a locale.

### What a plugin says of its commands is judged as the catalog's is, and shown as text

**Guaranteed.** A plugin's entries — `commands.toml` beside its manifest, one row per
command by its name inside the plugin's namespace, and `i18n/<locale>.toml` for every
locale the build ships — are input from outside the platform, and nothing of them is
shown before it is judged:

- **Read privately, bounded and in order.** The daemon reads the plugin's manifest first,
  then the command table, then `i18n/` — listed, a file in it named for no locale this
  build ships refused rather than left unread, whatever the case of its name — then each
  locale's texts, and the component last; each path is judged this user's alone with no
  link followed, and each entries file is read no further than
  `PluginEntries::most_bytes`, its length asked first and the read itself held a byte past
  it, and only as UTF-8. A file missing is `[NC-PLUG-0029]`; a path another account can
  reach, or one the filesystem cannot answer for, keeps the profile's code.
- **Read for its shape, then judged whole.** `PluginEntries::parse` refuses TOML of
  another shape, a field it does not know, a name no namespace can precede, a row written
  twice, a locale's texts for a command the table does not list and a locale handed twice
  or not at all. At `activate`, every command the plugin may name has a row, and each row
  is held to `nc_schema::entry::judge` — the judge every command of the catalog is held
  to: every text in every locale, nothing a reader cannot see, no placeholder, each text
  within its bound in characters and the locale's texts within `[kernel] entry_bytes`
  together, each tag with the quotation marks and the comma it is listed in, a translation
  that is not the English again unless a reason says why, the categories the catalog holds
  and a glyph the font draws. A text a locale misses is
  refused naming the command and the locale.
- **A plugin cannot describe what is not its own, nor pose as the kernel.** A row for a
  command the catalog describes is refused, and so is a display name a command of the
  catalog is shown by, in any locale the catalog shows it in, compared as the untranslated
  rule compares sentences. Two commands of one plugin shown by one display name in a
  locale are refused too, naming both.
- **The component and its directory say one thing.** The title a command declares is its
  English display name, or the plugin is refused.
- **One plugin's texts take at most half a frame.** The compiler holds
  `commands_per_plugin` × `entry_bytes` × 2 — every command at its bound, each byte one
  JSON writes as two — to half of `ipc_frame_max_bytes`, and the judge counts each tag in
  those bytes with the quotation marks and the comma the answer lists it in, so no number
  of short tags takes more of the frame than its bytes say. The catalog's commands are held
  to `entry_bytes` too. A command's categories and glyph are not its texts — names the
  catalog holds and one code point — and travel in the other half, with the names and the
  schemas.
- **Shown as text, whoever wrote it.** The registry is asked in one locale
  (`Request::Registry { locale }`), and the daemon resolves every command's texts in it,
  the catalog's as much as a plugin's; the CLI escapes every one it prints, as **The CLI
  sends nothing until both ends are proven, and prints nothing a terminal acts on** says,
  and prints no glyph. A refusal cuts and escapes whatever it repeats
  of a plugin's text.

**Enforced by** `crates/nc-kernel/src/entries.rs` and `crates/nc-kernel/src/registry.rs`,
with a unit test per rule; `apps/daemon/src/plugins.rs`, whose tests remove each file,
plant a stranger in `i18n/`, open each file to another account, write one past its bound
and write one that is not UTF-8; `crates/nc-kernel/tests/activation.rs` and
`crates/nc-kernel/tests/the_reference_plugin_activates.rs`, which hold the reference
plugin's entries to the commands it declares, both ways;
`apps/daemon/tests/serves_its_kernel.rs`, where a plugin whose Portuguese texts miss a
command is shown refused and the daemon runs without it;
`a_plugins_entries_at_their_bound_fit_half_a_frame` in `apps/daemon/src/server.rs`, which
lists the heaviest entry the judge admits over every count of tags, and the compiler's
`an_entry_bound_that_lets_one_plugin_outgrow_half_a_frame_fails_the_build`; and the
operators over those sources.

**Not answered:** whether a plugin's texts are true or its translations faithful beyond not
being the English again; display names that differ only by look-alike characters, which
needs the confusable folding `BLANK_RANGES` says `nc-schema` does not carry; commands of two
plugins shown by one display name, which one plugin installed under two namespaces is, its
one set of entries serving both — only a catalog command's name, and one its own plugin
shows, are refused; that the
whole registry answer fits a frame, its commands' categories and glyphs included, which no
share of it counts (under **What is deliberately not claimed**); and that a glyph renders on
a surface.

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

- an approval issued for real — only a test door issues one, so a shipped kernel denies
  every command a quorum must approve; a confirmation is issued for real only as the
  person's answer to a call the kernel held (*A call that cannot be undone runs once the
  person confirms it in the desktop the daemon started*);
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

### An agent is held under the identifier the machine minted, for its person alone, and a grant answers for one feature

**Guaranteed.** `nc_authz::LocalDirectory` holds the agents a person makes over the world a
profile was started with (`NOCTUA.md §14.6`). An agent is held under the Crockford text of
the `Snowflake` the kernel minted for it, turned into an identifier in `nc-authz` alone, and
a question reaches it only by that text, compared whole: a label, a lower-case spelling, the
text of a value no change made, a principal of another kind and a question about another
realm are the world's to answer and never reach the agent held, and nothing is parsed out
of an identifier (`NOCTUA.md §5.1`). Only a person the world holds in the realm is made an
operator, and only an agent's operator grants it a feature or ends its grants
(`[NC-AI-0002]`); a name no agent is held under is `[NC-AI-0001]`. An agent is delegated
only while the world holds its operator in the realm, whatever was kept. A grant runs for a
window it must have — it ends after it begins — on a feature the world holds in the realm,
with a reason of one to 256 characters a reader can see all of, and a label is held to one
to 64; anything else is refused as the data the directory must never hold (`[NC-VAL-0002]`).
A grant answers only a question about the feature it names, compared whole, never through an
entity the resource is in, so an agent granted every feature holds no grant for the realm's
content; a grant the world holds, a person's or one of the world's own agents', stays
realm-wide. A new grant on a feature ends the live one there. Neither an agent nor a grant
is ever deleted, and each table is refused past its bound (`[NC-AI-0003]`), an ended grant
still counted. A new agent or grant is held only once its keeper kept it; an ending is
held first, so the next question is answered without it even when the keeper then refuses
it (the keeper's code). An agent's slice holds its operator beside it — so a rule reading
`principal.operator` decides in the kernel as over the whole world — its budget as
`budget_remaining` and `autonomy` as `supervised`, which no embedded policy reads; and
`Facts::budget_spent` is taken off that budget before Cedar reads it, without wrapping, so
`core.budget_is_a_permission` refuses an agent that has spent it all. A directory opened
from what was kept refuses what it would never have written, and leaves another realm's
rows out of what it holds while they still count against its bounds: each bound is held to
the keeper's count of its whole table, and a count below the rows the keeper handed over is
refused.

**Enforced by** `crates/nc-authz/src/agents.rs` and its unit tests — the identifier held
and nothing else, the routing by text, the operator's alone, the feature compared whole,
each malformed grant, label and budget, the bounds, the order of keeping, the reopening and
each refusal's kind, code and word; `crates/nc-authz/src/directory.rs`, whose world answers
an agent's operator only for an agent of the realm and a person of it; and
`crates/nc-authz/tests/the_embedded_pack_decides.rs`, where a budget spent is taken off what
an agent holds, a person holds none to spend from, a spend past what a `Long` holds leaves
nothing, and one whose entity does not read back is refused.

**Not answered:** whether the person may make a change, which the decision point decides
before the change is asked for; whether a grant is current, which Cedar decides against the
kernel's clock; whether an agent's rights are its operator's as well as its own, which the
enforcement point asks of each apart; and an identifier as a secret — it is neither one nor
authentication, and whoever can reach the person's processes can name any of their agents.

### An agent acts as itself and as the person it acts for, and is never confirmed

**Guaranteed.** An agent reaches the kernel through a `nc_kernel::Session`, which
`Kernel::session` opens for an agent and a person: the directory names the agent and its
operator (`nc_authz::Directory::delegation`), and the session is opened only when that
operator is the asking person, the answer an agent for the identifier asked; an agent held
for another person, one held for no person and one never made are refused alike, one
sentence and `[NC-AI-0001]`, and a directory that does not answer is `[NC-POL-0006]`. The
session's chain is written by the kernel from that answer — the person, then each agent
between, outermost first — and nothing a caller sends adds to it. `Kernel::dispatch` takes
a `Caller`, acting as itself, or a session (`nc_kernel::Asker`). Every decision of a tree
acting for an agent — the dispatch at its top and every one nested — is asked once for each
link, the agent first, each with its own grant and slice, all with one moment, one epoch,
`via_agent` true and the chain's length as the depth, and the first denial is the answer;
so an agent is allowed only what its own permit, its person's grant to it and its person's
own rights all allow, decided again at every call, and its person frozen or their grant
ended refuses its next call. The family pack gives an agent two permits of its own,
`family.an_agent_names_files` and `family.an_agent_finds_commands`, and a grant on a
feature no agent permit names allows nothing. What decided is one segment per link reached,
joined by `;`, and the caller is told one sentence and the code, never which link refused;
every entry of the tree, its refusals before any decision included, names the agent as its
principal and the chain as its actor chain; the grant it records, when a decision reached
one, is the agent's and never its person's. An agent named as a caller with no session
decides as itself, unmarked, which `core.an_agent_never_impersonates` refuses. **No agent
is confirmed**: a session holds no token and has nowhere to hold one, a tree acting for an
agent or as an agent alone redeems and spends none it is handed, and the kernel never binds
a token to an agent principal, whoever issued it; so an agent's command that cannot be
undone is refused with `[NC-KERN-0006]` — unless, held while the desktop the daemon started
is attached, its person confirms it there, which binds the person's input to the call the
kernel holds and hands the agent nothing — and its delete by the core's two floors. A chain
past the policy's two links is refused by `core.chain_depth_is_bounded` and recorded whole;
the type holds at most four, and nothing but a test door builds one past a single link.

**A session spends its agent's budget, and the core refuses what is past it**: one unit for
each call it sends — refused or not, counted under the kernel's bus before anything is asked
of the call — one for each timer its tree set that fires, and one for each event its tree
published that is delivered, whatever its subscribers; what a call dispatches nested spends
nothing, and each session counts its own, a clone of it spending from the same count. Every
decision of a tree is told what its session had spent when the tree began, which `nc-authz`
takes off the agent's `budget_remaining`, so `core.budget_is_a_permission` refuses a spent
session's call, firing or delivery, and the entry names the rule. **What its tree puts off
acts as its tree did**: a timer it sets and an event it publishes carry the session, are
decided again for every link when they come due — its person's authority withdrawn or ended
cancels the timer or drops the event — and call back as the agent for its person, never
confirmed. **Its steps are its person's**: a step its tree keeps, its timer's callback's
included, is in its person's history, whose undo takes it back, and an agent's own undo
finds no step. **Nothing it arranges runs later as its person alone**: a compensation
registered in its tree is refused (`[NC-KERN-0026]`), so its step is undone by its images or
is a barrier. **It may list what it may dispatch**: `Kernel::dispatchable` is the decision
point's probe — each command whose resource is known without arguments and that is not
one that cannot be undone, decided for every link on what the session has spent, never
journaled, spending nothing and redeeming nothing — and its search finds that set.

**Enforced by** `crates/nc-kernel/src/session.rs` and its unit tests — what a chain holds,
how deep it is, that only a principal acting as itself holds what the kernel issues, that a
session and its clone spend from one count, and that the probe lists no command that cannot
be undone, though the decision point allows it;
`crates/nc-kernel/src/dispatch.rs`, whose unit tests hold that no token issued to an agent
is bound, that an agent acting alone redeems none even of its own, and that an agent's tree
hands on nothing redeemed even when the binding alone would redeem it;
`crates/nc-kernel/tests/the_decision_point.rs`, over runtime agents a
`nc_authz::LocalDirectory` holds and the world's own, through the real pack: both halves
allowing and each refusing, a grant ended, withheld or run out on the kernel's clock between
two calls, a nested dispatch, the session refused for every agent that is not the asker's
and for a directory's answer about another agent or a person, a chain of two and one of
three, and every token path — the operator's confirmation and approval, the agent's own
presented without a session, which the core refuses whatever it holds, an argument named as
one, a serial carried in an argument the schema takes, a nested irreversible command, a
delete;
`crates/nc-kernel/tests/the_decision_point.rs` too for the budget — spent to its end,
refused calls spending, nested ones not, each session its own and nothing but the core's
rule refusing — and for the probe, held both ways to every command whose resource is known
without arguments, to each grant and permit, to the agent's search and to what each
dispatch decided, and journaling nothing;
`crates/nc-kernel/tests/units_and_history.rs`, where the person is frozen mid-session, an
agent's step is undone by its person, its own undo finds nothing, and a compensation its
tree names is refused; `crates/nc-kernel/tests/the_rest_of_the_world.rs`, where an agent's
timers and events are decided for it and for its person when they come due, spend from its
session's budget and are never confirmed, a timer's firing and an event's delivery past
that budget are each refused by the core's budget rule, its timer's step joins its person's
history, and the chain a plugin reads is the one the journal records; the unit tests of
`crates/nc-kernel/src/host/context.rs` and `crates/nc-kernel/src/host/commands.rs`, where an
agent's tree reads no confirmation and registers no compensation;
`crates/nc-authz/tests/the_embedded_pack_decides.rs`, which holds the commands
an agent reaches in the pack to naming and finding and refuses an agent of another realm by
the slice; the cases of `policy/packs/family/tests/features.toml`, `accounts.toml` and
`policy/core/tests/`; and `no_member_turns_a_test_door_on_outside_its_tests` in
`xtask/src/gates.rs`, which refuses a member turning the test doors on in a build that
ships.

**Not answered:** a same-user process, which dispatches as the person without naming an
agent; a session opened again, which counts its budget from nothing; a command that takes
its resource from an argument, which the probe and an agent's search leave out though its
dispatch is decided as any; and the operator's grant in the entry, which names the agent's
alone. See "What is deliberately not claimed".

### A person makes, grants and ends their agents by their own calls, each journaled before it is made

**Guaranteed.** `agents.create`, `agents.list`, `agents.grant` and `agents.revoke` are
commands of the kernel's own (`crates/nc-kernel/src/agents.rs`, whose header has the
rules), decided on `Feature::"agents"` by `family.a_member_keeps_their_agents`, which lets
every person of the realm keep their own and no agent or device: an agent's half of every
question about the feature is refused, its own grants included. The operator of an agent
and the giver of a grant are the asking person, written by the kernel and never an
argument; an agent is named by the identifier `agents.create` answered, matched whole by
the directory, and an identifier no agent is held under is `[NC-AI-0001]`; only the agent's
person grants it or ends its grants, so another person's agent is `[NC-AI-0002]`; a grant
runs for one to `[kernel] agent_grant_minutes_max` minutes from the kernel's clock, a budget
is one to `[kernel] agent_budget_max`, and a label or a reason holding a character a reader
cannot see is refused before anything is minted (`[NC-VAL-0002]`). **The three that change
run only at the top of a call tree** — a plugin run as the person, a timer's or an event's
callback, is refused with `[NC-KERN-0025]` before it is decided — and **each change is
journaled before it is made**: rehearsed by the directory within what the call tree has
left, a refusal journaled as failed with its word; journaled as allowed with what it makes —
the agent, the grant, the feature as an entity, the window as integers, the grant it ends,
the grants ended, and never the label or the reason; and made only once that entry has
landed, so a journal that does not take it leaves the agents as they were. A step that
changed them left the machine, so undo stops at it; the reverse of a grant is a revocation.
A daemon opens its kernel over `nc_authz::LocalDirectory`, the world and what the profile's
database keeps of the agents of the world's realm, with the count of each whole table,
bounded by the catalog's `max_rows`, so another realm's agents count against the bound when
a change is rehearsed, before it is journaled; it keeps each change there through its
keeper — a table at its bound `[NC-AI-0003]` — and refuses to start over rows the directory
would never have written.

**Enforced by** the unit tests in `crates/nc-kernel/src/agents.rs` — the declared commands
both ways, the schemas' bounds held to the kernel's limits and to the profile's table, each
refusal's kind, code and word, nothing minted for a text a reader cannot see, each change
handed to the keeper at the kernel's clock and by its asker, and the widest list of agents
within a frame; `only_a_setting_or_an_agent_change_of_the_kernels_own_runs_at_the_top_alone`
in `crates/nc-kernel/src/registry.rs`; `crates/nc-kernel/tests/the_decision_point.rs`, where a
person makes, grants and ends an agent, each entry read back, another person grants and ends
nothing of it (`a_person_grants_and_ends_nothing_of_another_persons_agent`), an agent and a
plugin make, grant and end none, and a journal and a directory that do not take a change
leave it unmade; the unit tests in `apps/daemon/src/environment.rs`, where what the daemon's
keeper keeps is what a directory opened again holds, each ended grant read back as ended where
it was asked and by its asker, and another realm's agents at the bound refuse one more as it
is rehearsed;
`apps/daemon/tests/every_command_from_the_cli.rs` and `serves_its_kernel.rs`, where the CLI
makes, grants, lists and ends an agent, names it by its identifier alone, and a daemon started
again holds what was kept; the cases of `policy/packs/family/tests/features.toml` and
`accounts.toml`; and the operators over `agents.rs` and the daemon's `environment.rs` in
`xtask/mutants.py`.

**Not answered:** a process running as the same user, which runs the CLI as the person or
writes the profile's database, as "The agents a person made are kept as written: each row
written once, an agent never changed and a grant ended once" says; a change made once its
entry landed
that the database then refused to keep, which the unit's close says did not finish — a new
agent or grant is held nowhere, and an ending holds until the kernel stops; and an agent of
another realm the profile's file holds, which the directory leaves out of what it answers
and counts against its bounds. See "What is deliberately not claimed".

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
  their newest undo among their newest `undo_history` units. A step an agent's call tree
  kept is in the history of the person it acts for, who undoes it; an agent's own undo
  looks for steps of its own and finds none.
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
  `store_notes`' notes free for its undo. None is registered in a call tree acting for an
  agent (`[NC-KERN-0026]`): the undo would dispatch it as the person alone.
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
again, and where an agent's step is undone by its person, its own undo finds nothing and a
compensation its tree names is refused; `crates/nc-kernel/tests/the_decision_point.rs`, where the journal does not take an
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

### A file's name is proposed and read by one function, decided and journaled like any command

**Guaranteed.** `naming.propose` and `naming.read` are the one builder and the one parser
of `NOCTUA.md §16.3`, `nc_ids::naming`, as commands of the kernel's own (`crates/nc-kernel/src/builtin.rs`
says what each takes and answers):

- **Decided at the one door and journaled**, as every command is: `Read` on
  `Feature::"naming"`, which `family.a_member_names_files` gives every person of the realm.
  A frozen account is refused by the floor; a device by the permit's `principal is User`,
  pinned by cases; a person of another realm by the decision point's slice. An agent is
  allowed by a permit of its own, `family.an_agent_names_files`, only once its person
  granted it the feature, and only while its person may (*An agent acts as itself and as
  the person it acts for*). The handler — and so the minting — runs only after the
  decision allowed it, so a denied proposal spends no identifier.
- **The entity always means naming.** Only a command of the kernel's own selects a
  feature: a plugin declaring `feature:` is refused at `activate`, and an argument naming
  `Feature` is refused in every declaration by the judge the catalog's commands and a
  plugin's are held to (`[NC-PLUG-0009]` for a plugin), so no caller chooses which
  feature decides a command, a plugin cannot borrow the permit, and a journal entry naming
  `Feature::"naming"` is a naming command.
- **A person's text stays out of the chain and the log.** Titles and keywords are
  arguments, which no entry records: the entry holds the command and the entity. The kernel
  never turns caller text into an identifier: a kept `id` is read in `nc-ids`, which
  cannot log, and comes back as text, which the log's type wall refuses as an identifier.
  So no kernel file writes a needle of `AWAY_FROM_A_CALL`, and `nc_ids::naming::propose`
  and `read` return no identifier type, which the census of public routes to one in
  `xtask/src/wall.rs` would otherwise have to name.
- **A name is one path component holding nothing a filesystem refuses**, at most
  `nc_ids::naming::MAX_BYTES` bytes, which `crates/nc-ids/tests/properties.rs` holds for
  every generated input, so an extension written as a path is cleaned of its separators
  and names no directory.
- **Bounded**: the argument schema's bounds — each the byte limit implies, which a test in
  `crates/nc-kernel/src/builtin.rs` holds — then the kernel's payload bound, then a parse
  and a build linear in the name; a call tree's naming calls are bounded by its dispatch
  count.
- **An identifier minted for a name is the machine's one minter's**, the same the journal's
  units come from (`nc_journal::Journal::mint_identifier`), behind the mark persisted
  before any is handed out, so no name's identifier is a unit's.

**Enforced by** `crates/nc-ids/src/naming.rs` and the properties beside it — a proposed
name reads back as the fields that built it, a kept identifier in its canonical spelling —
`crates/nc-kernel/src/builtin.rs`'s tests of the arguments, the bounds and each failure's
kind, code and word, `crates/nc-kernel/tests/the_decision_point.rs` over the real pack and
world — every person answered, a frozen account, a device and an agent named without a
session refused, an agent's session allowed on both halves and refused on either, a rename
keeping its identifier, a plugin's nested proposal, a minter that mints nothing — the
registry's and `crates/nc-kernel/tests/activation.rs`' refusal of a plugin's `feature:`
and of an argument naming `Feature`, the selector's own refusal of that argument in
`crates/nc-schema/src/selector.rs`,
`policy/packs/family/tests/features.toml`, and `apps/daemon/tests/every_command_from_the_cli.rs`,
through the CLI and over the endpoint as one JSON dispatch.

**Not answered:** see "What is deliberately not claimed": a proposal reserves nothing, a
kept identifier is not vouched for, and whether a file of that name exists or collides is
the saving feature's.

### A command is found by its names and tags in bounded work, decided and journaled like any command

**Guaranteed.** `kernel.find` is the registry filtered by the one fuzzy core
(`NOCTUA.md §12.4`), a command of the kernel's own (`crates/nc-kernel/src/find.rs` says
what it reads and answers):

- **Decided at the one door and journaled**, as every command is: `Read` on
  `Feature::"registry"`, which `family.a_member_finds_commands` gives every person of the
  realm. A frozen account is refused by the floor; a device by the permit's
  `principal is User`, pinned by cases; a person of another realm by the decision point's
  slice; an agent is allowed by a permit of its own, `family.an_agent_finds_commands`, only
  once its person granted it the feature, and only while its person may. It opens no unit,
  and its entry names the command and the feature, never the query.
- **An agent finds what its session may dispatch now.** In a call tree acting for an agent
  the registry is cut, before anything is matched, to the decision point's probe
  (`Kernel::dispatchable`): each command whose resource is known without arguments, that
  is not one that cannot be undone, and that the agent and its person may be allowed, on
  what its session has spent — so an agent's search and its tool list are one set. A person's finds every
  registered command.
- **Bounded work, whatever is typed.** The schema refuses a query past
  `nc_fuzzy::QUERY_CHARS_MAX` characters before the handler runs — its `maxLength`, which a
  test beside the handler holds equal to the core's bound — and the handler refuses one
  again rather than cut it. Every field is read to `nc_fuzzy::FIELD_CHARS_MAX` characters,
  which every text a registry may hold fits within, and one matcher's scratch serves every
  command. The registry's lock is let go before anything is matched, so no load and no
  dispatch waits on a search.
- **A bounded answer.** At most `find::LISTED_MAX` commands, the schema's `maximum` for
  `limit` and the most listed whatever `limit` says; the widest answer those can make —
  every word echoing a whole field of the character canonical JSON writes longest — fits the
  kernel's `payload_bytes`, which the compiler holds within one frame with its envelope.
- **Nothing new is shown.** The texts it echoes are the registry's, judged at the build or
  at `activate`, and listed already to the same person by `Request::Registry`; the CLI
  escapes them as it escapes every text from the daemon. Each command found says whether it
  is a plugin's (`owner`), so a surface can tell a plugin's tags from the platform's.

**Enforced by** the tests beside the handler in `crates/nc-kernel/src/find.rs` — the query
bound held to the schema's, every registry bound within the core's, the widest answer, the
limit and the count, the registry's order among equal scores, and both languages' texts
over the catalog's own registry — `crates/nc-kernel/tests/the_decision_point.rs` over the
real pack and world — every person answered, a frozen account, a device and an agent named
without a session refused, an agent's session refused a feature it was not granted, its
search finding exactly what its session may dispatch, and a plugin's command found by its
own entry — `policy/packs/family/tests/features.toml`,
and `apps/daemon/tests/every_command_from_the_cli.rs`, through `nc_cli::run` against a real
daemon.

**Not answered:** see "What is deliberately not claimed": how many commands the registry
holds; which commands a person may dispatch, a person's search listing every registered
command; and an agent's command that takes its resource from an argument, which its search
does not find though its dispatch is decided as any.

### A setting is switched by a person's own call, and MCP is off until one switches it on, confirmed in the Noctua window

**Guaranteed.** A setting of the channel (`NOCTUA.md §12.15`) is two commands of the
kernel's own, and the first is MCP's, `settings.mcp.read` and `settings.mcp.set`
(`crates/nc-kernel/src/settings.rs`):

- **Decided at the one door and journaled**, as every command is: `Read` and `Write` on
  `Feature::"mcp"`, which `family.a_member_switches_mcp` gives every person of the realm, a
  child included. A frozen account is refused by the floor; an agent and a device by the
  permit's `principal is User`, pinned by cases — the agent's marked as acting for its
  person, so only the type test refuses it; a person of another realm by the decision
  point's slice. The set runs in a unit of its own, the read in none.
- **Only a person's own call switches it.** The set runs at the top of a call tree alone:
  dispatched from inside a command — a plugin run as the person, a timer's or an event's
  callback — it is refused with `[NC-KERN-0025]`, journaled under `kernel.top_level_only`
  before anything is decided, and that is asked before whether a read's frame may change
  anything. The read runs anywhere.
- **Off until switched on, per channel.** The value is the kernel's entry in the channel's
  local store, under a holder no plugin's identity can be, so no plugin reads or writes it,
  and switching one channel's switches no other. Never written, it is off; an entry
  holding anything but the two texts the set writes is refused as a row the kernel did not
  write, never read as on or as off (`Kernel::settings`). The daemon reads it before it
  opens an agent's session and before each request of an open one, and refuses both while
  it is off (`NC-DMN-0014`), so what it lets in is the entry below.
- **Switched on only once the person confirms it in the desktop the daemon started**
  (Stage 16). The set asking for on — whatever MCP is now — and an undo or a redo that would
  put it back on — the undo of a switch off, the redo of a switch on, read from the step the
  history finds before anything runs — are held for the person's confirmation as a command
  that cannot be undone is (*A call that cannot be undone runs once the person confirms it*,
  below): refused at once with `[NC-KERN-0032]`, exit 4, while no desktop is attached;
  declined, `[NC-KERN-0034]`, exit 15; unanswered in time, `[NC-KERN-0033]`, exit 14; each
  journaled once under `kernel.mcp_on_needs_confirmation`, and listed while held as the
  `write` its command is declared as. Switching it off never waits. A history the
  decision point cannot read is the undo's or the redo's own to meet: refused with the
  store's code, or — read then — put back so as to switch MCP on only holding a
  confirmation (`[NC-KERN-0032]` otherwise).
- **A step like any other.** The set's one effect is that entry, staged in its unit, so it
  is no step that left the machine: the person's undo puts the value back and reaches the
  steps before it, and their redo puts it again — confirmed in the window when that switches
  MCP on, so a switch off is never a barrier to the steps before it.
- **Three settings are how the interface is shown, and are the interface's state**
  (Stage 16): `settings.footer`, `settings.language` and `settings.appearance`, each a
  `.read` and a `.set` answering and taking one object — `visibility`, `language`, `mode`.
  Their value is a part of the state the kernel holds for every window
  (`crates/nc-kernel/src/shell.rs`), so each is decided on `Feature::"interface"`, which
  every person of the realm is given and no agent or device is, is a read that opens no
  unit — no undo takes one back, and the setting set again is the way back — reaches every
  window with the state it follows, and is kept with it between starts. A value is one
  word of a closed list — the footer `always` or `hidden`, the language `system` or the
  tag of a locale the catalog ships, the mode `system`, `light` or `dark` — the first its
  default, and any other word is refused with `[NC-VAL-0002]` from a command and sets the
  whole saved state aside when it is read back. The set runs at the top of a call tree
  alone, as every setting's does: it declares no change, since it is no step, and is kept
  there by the action it is decided under, `Write` (`only_at_the_top`).

**Enforced by** `each_setting_of_the_interface_is_one_word_of_its_own_list` and
`a_saved_states_settings_are_read_only_as_words_of_their_lists` in
`crates/nc-kernel/src/shell.rs`, for the three settings of the interface — each list held
to its command's schema and, the language's, to the catalog's locales — with the rows of
`apps/daemon/tests/every_command_from_the_cli.rs`, which set and read each through
`nc_cli::run` against a real daemon; and, for MCP, the tests beside the module in `crates/nc-kernel/src/settings.rs` — the
declarations held to what it reads, the holder no plugin's identity, the default, and
every entry holding anything but the two texts it writes refused — and `only_at_the_top`'s in
`crates/nc-kernel/src/registry.rs`; `crates/nc-kernel/tests/the_decision_point.rs` over
the real pack and world — every person answered, the switch on refused unconfirmed and run
confirmed, held, confirmed, declined and cut short through `Kernel::dispatch_held` while the
switch off is never held, the undo of a switch off and the redo of a switch on refused
unconfirmed and run confirmed, a frozen account and a device refused, a plugin's nested
switch refused from a command that writes and from one that reads, and each of the three
sets of the interface refused nested the same way with the setting left as it was
(`a_plugin_sets_no_setting_of_the_interface_from_inside_a_command`) —
`crates/nc-kernel/tests/units_and_history.rs`, which undoes and redoes the switch and an
earlier step past it, the unit test of `crates/nc-kernel/src/history.rs` holding the
kernel's own changes undo takes back to undo, redo and the switch, which `docs/commands.md`
answers by name, `policy/packs/family/tests/features.toml` and
`policy/packs/family/tests/accounts.toml`,
`apps/daemon/tests/every_command_from_the_cli.rs`, through `nc_cli::run` against a real
daemon with no desktop, where the switch on ends in exit 4,
`apps/daemon/tests/channels_side_by_side.rs`, where MCP switched on in one install is on in
that one alone, and the desktop's own test in `apps/daemon/src/desktop.rs`, where the CLI's
switch on waits for the desktop, runs once confirmed and ends in exit 15 once declined.

**Not answered:** see "What is deliberately not claimed": a process running as the person,
which writes the profile's database — every suite but the desktop's switches MCP on that way
(`nc_daemon::testing::kept_mcp_on`) — and what an agent's tree set going while it was on,
which switching it off does not end.

### A shortcut setting writes the channel's keys through the judge that reads them, and a choice refused writes nothing

**Guaranteed.** The channel's Leader, its chord modifier, the leader menu's keys and the
chords are four settings (`NOCTUA.md §12.8`, `§12.15`) — `settings.leader`,
`settings.chord`, `settings.menu-keys` and `settings.chords`, each a read and a set of the
kernel's own, the read answering the one object the set takes whole — whose value is the
channel's `<config>/keybindings.json` and never an entry of the store:

- **Decided at the one door and journaled**, on `Feature::"interface"`: `Read` and `Write`,
  which `family.a_member_drives_the_interface` gives every person of the realm, a child
  included, and no agent or device; a frozen account is refused by the floor. The set runs
  at the top of a call tree alone (`[NC-KERN-0025]`), in a unit of its own, the read in none.
- **Read as the file is read.** A read answers the keys in force
  (`nc_channel::keys::effective`): the Leader and the chord modifier as the grammar writes
  them, the menu's keys as every binding of the menu's table and the chords as every binding
  of the two tables a held Leader has, each saying which. What a set is asked is read by the
  same grammar (`nc_schema::keys`): a chord, a modifier, a key or a command's name it does
  not write — a modifier alone, a lock key, the numpad, a media key and Esc among them — is
  refused with `[NC-CHAN-0025]`, what it repeats of the arguments escaped and cut at 64
  characters.
- **Nothing is written that its reader would refuse any part of.** A choice is made over
  the file as it stands (`nc_channel::keys::proposed`): one setting's value changes — the
  Leader, the chord modifier, the menu's table or the two tables of the chords, whole — and
  the rest of the file stands; a choice that is the channel's default, and a command bound
  as the catalog binds it, are written as no choice, so setting what was read where nothing
  was configured configures nothing. It is read back by the file's own reader, so it is
  held to the file's bound, and judged as the file is; any refusal — of the part the choice
  changed or of one it left — refuses the set: a rule of the Leader's, or another channel's
  file it could not be judged against, with `[NC-CHAN-0023]`; a key of one table bound
  twice with `[NC-CHAN-0024]`; and a file as it stands that is not this user's alone, was
  not read, or is not one this build reads — a later format among them, never written
  over — or a choice past the file's bound, with `[NC-CHAN-0027]`. Each sentence says that
  nothing was written and the keys in force stand, never that a default does.
- **Journaled before it is written, and written whole or not at all.** The choice is
  judged, its entry journaled as allowed, and only then written
  (`written_once_journaled`): a journal that does not take the entry leaves the file as it
  was. The file is written through a private file beside it, synced and renamed over it,
  into a directory judged this user's alone and never made here; a file a cut write left
  beside it is removed only once judged this user's alone, and a write the disk refuses is
  `[NC-CHAN-0027]`. The file is outside every unit, so the set is a step that left the
  machine: the person's undo stops at it, and the way back is the setting set again.
- **A Leader the system refuses is never written**, where the daemon is the process that
  asks the system for it (`nc_ipc::leader`, Windows): a set of the Leader asks for the chord
  chosen first, on the thread that holds the registration and with the Leader in force
  still held (`nc_daemon::leader::Asking`) — the Leader held already asked of nobody — and
  what the system refuses (`nc_platform::leader::Refused`) refuses the set with
  `[NC-CHAN-0026]`, naming the system's own code, nothing written and the Leader standing
  as it was — registered where it was, and where the system refused it when the daemon
  started, still not; once the file is written the old chord is let go and the new one is
  the one held, and a write that fails gives the new one back. **Only a set of the Leader
  asks**: the chord modifier, the menu's keys and the chords leave the Leader as it is and
  ask the system for nothing, so a Leader refused at the start keeps none of them from
  being written.
- **Each write is told to whoever holds the keys in force.** The daemon counts the writes
  its settings made since it started and answers the count to every watch as `keys`, at a
  new revision; the desktop interface, which asks the system for the Leader on Linux and
  macOS, reads the file again when the count is another number, tells its page the keys,
  and where the Leader changed asks the system for the new chord before it lets the old one
  go, saying a refusal on the info line as it does when it opens.

**Enforced by** `crates/nc-channel/tests/a_configuration_that_collides_is_refused.rs` —
`a_setting_changes_its_own_value_and_a_default_is_written_as_no_choice`,
`the_menus_keys_and_the_chords_each_replace_their_own_tables`,
`a_choice_that_collides_is_refused_and_nothing_is_written`,
`a_choice_over_a_part_already_refused_is_refused_for_that_part`,
`a_file_its_reader_refuses_is_never_written_over` and
`a_choice_is_written_whole_into_a_private_directory_or_not_at_all`, each write read back
as the keys the choice said it puts in force and each refusal leaving the file as it was;
the tests beside `crates/nc-kernel/src/settings.rs` — the declarations held to what the
module reads, every bound of the schemas held to the vocabulary, each setting answered as
the object its set takes, every key kept out of a shortcut refused as reserved and
arguments of another shape as the arguments are — and the lists of
`crates/nc-kernel/src/registry.rs` and `crates/nc-kernel/src/history.rs`, where the four
sets run at the top alone and leave the machine;
`crates/nc-kernel/tests/the_decision_point.rs` over the real pack and world —
`a_shortcut_setting_is_read_and_set_by_a_person_alone_and_written_once_journaled`, with a
keeper the test holds: a kernel told of none refuses all eight, a choice the keeper
refuses is journaled as failed and never written, a journal that refuses the set's entry
leaves the keeper asked to write nothing, a write refused is the answer's refusal, and a
set made is journaled as allowed on the interface's feature and is a step undo stops at —
and `neither_a_frozen_account_nor_a_device_reads_or_sets_a_shortcut`; the unit tests of
`apps/daemon/src/server.rs`, over a kernel opened as the daemon opens one and
registrations the test grants and refuses —
`a_leader_a_setting_changes_is_asked_for_beside_the_one_held_then_kept_or_given_back`,
`a_leader_refused_at_the_start_is_asked_for_again_and_one_kept_is_said_held`,
`a_shortcut_setting_writes_the_keybinding_configuration_and_each_write_is_counted`,
`a_leader_a_setting_changes_is_granted_before_it_is_written_and_refused_where_the_system_refuses`,
where each registration says what the file held when it was asked,
`a_leader_refused_at_the_start_keeps_no_other_shortcut_from_being_written`, and
`the_count_of_the_keys_written_is_answered_once_there_is_one_and_turns_as_a_revision`;
`apps/daemon/tests/every_command_from_the_cli.rs`, where the eight are run through the
CLI against a real daemon and
`a_shortcut_set_through_the_cli_is_the_file_its_reader_reads` holds the file, the refusals
and the watch's count to each other; `apps/desktop/src/mirror.rs`
(`a_watchs_count_of_the_keys_written_is_read_as_the_daemon_writes_it`) and
`apps/desktop/src/host.rs`
(`the_keys_are_read_again_when_the_count_of_their_writes_is_another`); and the operators
over these sources in `xtask/mutants.py`.

**Not answered:** a process running as this user, which writes the file as a setting
would and is told to nobody — a file edited by hand is read at the next start of whoever
holds the keys; a binding to a name no command has, which is written as the file's reader
takes it and runs nothing; the window's own asking for a changed Leader on Linux and
macOS, which no test makes, since a test's install asks the system for nothing — the
count it reads and the rule it reads it by are what is held; the Windows row, read and
not run, as *What is deliberately not claimed* says of the daemon's listener; a Leader
the system grants and another program takes between the grant and the write, which is a
registration lost and told as one; and two chords held for as long as the write takes,
each press of either a tap.

### A call that cannot be undone runs once the person confirms it in the desktop the daemon started, and nothing else confirms one

**Guaranteed.** A command declared irreversible runs only with a confirmation the kernel
issued for that principal, that command and those arguments (*A command runs only when it
is decided*), and **the one a shipped kernel issues is the person's answer to a call it
held** (`crates/nc-kernel/src/confirmations.rs`). The daemon dispatches through
`Kernel::dispatch_held` while a connection is attached as the desktop interface it started
(`apps/daemon/src/desktop.rs`), and through `Kernel::dispatch` otherwise, so with none
attached such a call is refused at once with `[NC-KERN-0006]`, exit 4, as it always was.
Held, the call — allowed by the decision point at the top of its tree, from the CLI or from
an agent's session — is journaled nowhere and run nowhere yet; it waits, at most
`confirmations_pending_max` at once (one past them refused with `[NC-KERN-0006]`), for at
most `confirmation_ttl_ms`, holding no lock of the kernel's; it is listed by
`confirmations.list` and `confirmations.read` — reads on `Feature::"interface"`, which no
agent is granted — and its serial is in the daemon's watched state as `confirmations`. It
is answered by `Kernel::settle` alone, which the daemon calls for a `confirm` or a
`decline` on a connection attached as that desktop and on no other (`[NC-DMN-0018]`, logged
as `ipc.refused`); no command answers it, and `confirm` is no command — the named exception
to Stage 16's twelfth criterion. **A connection attaches by both halves, once**: a proof of
the 32 bytes the daemon handed the process it started down its standard input — never
written to disk, sent or logged — keyed over the connection's own handshake nonces, so a
proof is good on its connection alone; and the process the system names at the other end,
by its number as the connection was accepted, being that process, still running. One that
fails either, or that attached already, is refused `[NC-DMN-0017]`, logged as
`ipc.refused`, and closed. Confirmed, the call is asked of its asker first — the daemon
reads MCP again for an agent's session, so MCP switched off while the call waited refuses
it with `[NC-DMN-0014]`, ends the session, and journals nothing, as any request of the
session once MCP is off — then the kernel issues the person's confirmation of that very
call — bound to the person the tree acts for, the command by its name and its holder and
the arguments' canonical text, redeemed by that call alone and never presented by a
caller — and decides the call again from the start; declined, it is refused with
`[NC-KERN-0030]`, exit 15; with no answer in time, or once the daemon stops — its wait
ending as the stop begins — `[NC-KERN-0029]`, exit 14; each journaled once under
`kernel.irreversible_needs_confirmation`, and a session's budget spent once. **An answer
given as the wait ended is the one the call is decided by**: `settle` hands it over while
the call is still held, under the lock that lets the call go, so either the call takes it
or the answer is refused `[NC-KERN-0031]` — never a confirm answered while its call is
refused as not confirmed in time — and an answer to a call whose asker stopped waiting is
refused the same way. **MCP
switched on is held the same way** — the set asking for on, and an undo or a redo that would
put it back on — under codes and a sentinel of its own (*A setting is switched by a person's
own call*, above). **An agent's call is held the same way and the agent confirms nothing**: the confirmation is
bound to its person and never to an agent, and its plugin's `context` still answers that no
person confirmed a call in an agent's tree. Proven by the held-call tests of
`crates/nc-kernel/tests/the_decision_point.rs` — among them
`a_confirmed_call_its_asker_no_longer_admits_runs_nowhere_and_is_journaled_nowhere` and
`an_answer_given_as_the_wait_ends_decides_the_call_and_one_nobody_waits_for_answers_nothing`,
the second over two dozen rounds each a coin the wait's select tosses —
`a_held_calls_confirmation_is_bound_by_that_call_alone` in
`crates/nc-kernel/src/dispatch.rs`, the tests of `apps/daemon/src/desktop.rs` —
`a_desktop_the_daemon_started_attaches_and_answers_what_waits_and_nothing_else_does` over a
daemon a child of the test binary serves, whose desktop is another child handed its secret:
a second connection of it attaching and refused an attach asked again, the CLI's held
calls confirmed and declined there, a desktop started again by a person's `leader.tap` once
the first ended, a connection that did not attach and one that attached with a proof of no
secret refused, a held call ended in 14 by the daemon's stop well before
`confirmation_ttl_ms`, and each refused attach and answer logged as `ipc.refused` by its
code; and `a_desktop_attaches_by_its_own_process_proving_its_own_secret`, each half held
alone — `a_sessions_call_held_while_mcp_was_switched_off_is_refused_once_confirmed` and
`a_press_of_the_leader_starts_the_desktop_again_where_the_one_started_last_is_gone` in
`apps/daemon/src/server.rs`, `no_request_the_wire_reads_carries_a_confirmation` in
`crates/nc-ipc/src/message.rs`, and
`a_session_asks_for_its_tools_and_dispatches_and_is_refused_the_rest` in
`apps/daemon/src/server.rs`.

**Not answered:**

- **a process running as the same user that tampers with the desktop interface**: it can
  read the secret out of the desktop's memory or drive the desktop — attach a debugger to
  it, inject a library, send it synthetic input (XTEST on X11, a compositor's virtual
  keyboard on some Wayland sessions) — replace its executable or the units that start it
  before the daemon starts it, or announce a display of its own. On a desktop operating
  system that is not a boundary between two processes of one user (`NOCTUA.md §2.4`,
  §11.1); what is held is that a process that only speaks the protocol — the CLI, an MCP
  client, an agent, a plugin, a dispatched command — confirms nothing;
- whether the desktop asked the person: that a `confirm` comes from a person's own input
  in a confirmation the desktop drew is the desktop's to hold, and is not built yet;
- a desktop a daemon started before it last started: it holds the old secret, so it can
  watch and dispatch as the person and never attach, and retiring it is the desktop's;
- a process number handed out again: the process half compares the number the system
  named at the connection's other end as it was accepted, never when that process
  started, so a process that read the secret, connected, handed the connection to another
  and ended leaves a connection that passes the process half once the daemon starts a
  desktop under that number — reading the secret being already outside the promise, above;
- a held call whose desktop went away waits its whole time, unless another desktop the
  daemon starts attaches and answers it; and a desktop that ended is started again only
  when the daemon serves, a person's connection announces a display, or the Leader is
  pressed — never by a loop of starts.

### What the interface shows is the kernel's, changed by commands that are no step, and read back only as written

**Guaranteed.** The interface's own state (`NOCTUA.md §12.1`) is the kernel's
(`crates/nc-kernel/src/shell.rs`): the windows and each one's stack of pages, every page
open, the drawer, the sidebar, the footer, the info line, the prompt and each seat's focus,
hover, selection and drag.

- **Every change is a command, decided at the one door and journaled.** The commands of
  `shell.`, `window.`, `leader.`, `page.`, `drawer.`, `sidebar.`, `footer.`, `info.` and
  `prompt.` — namespaces the kernel keeps, refused to every plugin — are `Read`
  (`shell.read`) and `Write` on `Feature::"interface"`, which
  `family.a_member_drives_the_interface` gives every person of the realm, a child
  included. An agent and a device are refused by the permit's `principal is User`, pinned
  by cases; a frozen account by the floor; and a person's grant of the feature to an agent
  allows nothing, since no agent permit names it.
- **No step, and nothing undo takes back.** Each is declared a read: it opens no unit and
  makes no step, so an undo passes over every one to the person's last real change, and
  `docs/commands.md` says so of each. Each is still journaled once, its entry naming the
  command and the feature and never the arguments.
- **Held until its call tree ends.** What a command changes is its call tree's until the
  tree ends with every entry it gave the journal taken — or, nested in a command that
  writes, until that unit is kept — so a tree the journal refused releases nothing it
  holds itself: what it held is let go the moment the entry is refused, and what it holds
  after is never released. The next command of the same tree reads the change back, and a
  state held is released only over the one it was computed from, never over one released
  since. **A unit's state carries the tree's own**: in a timer's callback, the one tree
  that holds changes of its own beside a unit, a unit kept releases what it computed over
  the tree's hold, and the tree lets its own go then (`CallTree::kept_over_its_own`), so
  the commands after it read what the unit released; a callback's own is released after
  its unit, and only where no unit was kept holding one. **Except once the journal refused
  an entry of the tree**: a unit begun then computes over the kernel's state, never over
  the tree's own (`CallTree::shell_held`), so a unit kept carries no change the journal
  did not take, nor one the tree will not release. `shell.read` in a tree that holds a
  change answers that change with no revision, since a revision names a state the kernel
  held.
- **Bounded.** The windows, the pages, the drawer's pages and a page's sub-pages are each
  held to their `[kernel]` limit, a command that would pass one refused with
  `[NC-KERN-0028]`, as is one that would number a window or a page past the last number
  canonical JSON writes; a seat past `shell_seats_max`, and the prompt's draft or the info
  line's message past `shell_text_bytes` — counted in characters by its schema and in bytes
  by the module — are arguments, refused with `[NC-VAL-0002]` as the schema refuses them; a
  window is never smaller than `shell_window_min_width` by `shell_window_min_height`; a
  window, a page or a seat's window the state does not hold is refused with
  `[NC-KERN-0027]`; and the widest state there can be is an answer within `payload_bytes`.
- **Kept outside every unit, and never trusted when it is read back.** The pump saves the
  state when it changed, through the store's one write outside every unit
  (`LocalStore::keep`, under the kernel's own holder, at a revision no unit hands out, and
  refused over a key a unit wrote), a daemon's stop saves it last, once nothing it waited
  for runs (`Kernel::keep_interface`), and a kernel that opens reads it back. A save the
  store refuses is logged as `kernel.shell_unkept` and made at the next pump. A saved state
  of another shape, or one that breaks any rule a command keeps it to (`State::rules`), is
  set aside whole, logged as `kernel.shell_discarded`, and the kernel begins from the first
  run's state, which shows nothing but the footer.
- **A Leader tap follows `NOCTUA.md §12.8`'s three rules in order**: with the leader menu
  open in the drawer of the seat's window, shown, it sends the next page to the other
  place, with the seat's keyboard on the prompt of a window shown and the footer that
  holds it shown it opens the menu, and otherwise — the menu left open in a window hidden
  or closed, or the footer hidden with the keyboard on its prompt, included — it shows the
  window and puts the keyboard on the prompt. Every tap shows the footer, as
  `NOCTUA.md §12.3` says. **It never hides the interface**, and a menu or a prompt line no
  one sees never keeps it from showing it.

**Enforced by** the tests beside `crates/nc-kernel/src/shell.rs` — the declarations and
every schema's bound held to what the module reads and the catalog's limits, each
command's change and refusal, the three rules of a tap, the last identifier, every field
of a saved state read back at a value the first run's does not hold, every rule a saved
state is held to broken alone, the one rule broken read back, the widest state within a
payload, and a release over the state it was computed from alone; the read in a tree
that holds a change, and a tree's own state let go when the journal refuses its entry and
never read by a unit begun after, in `crates/nc-kernel/src/host/commands.rs`;
`crates/nc-kernel/tests/the_decision_point.rs` over the real
pack and world — a person driving the interface in no unit, a frozen account and a device
refused, a change whose entry the journal refused never made, an undo passing over the
interface to a note, the state kept and read back by a kernel opened again, a save the
store refused logged and made at the next pump, the stop's save, one of another shape set
aside, and a plugin driving it twice in one tree from a read and from a write;
`crates/nc-kernel/tests/the_rest_of_the_world.rs`, a timer's callback changing it around a
unit kept in it, and one whose first change the journal refused making the unit's alone; `apps/daemon/tests/serves_its_kernel.rs`, a note made just before a stop
read back by the next start; `crates/nc-kernel/tests/both_stores_answer_alike.rs` and
`crates/nc-local/tests/a_unit_lands_whole_or_not_at_all.rs` for the keep;
`policy/packs/family/tests/features.toml`; `apps/daemon/tests/every_command_from_the_cli.rs`,
through `nc_cli::run` against a real daemon; and the operators over the module in
`xtask/mutants.py`.

**Not answered:** see "What is deliberately not claimed": one journal row for each command,
the revision's scope, and a plugin driving the interface as its person.

### A Markdown text is read by one closed subset, and nothing outside it reaches a page

**Guaranteed.** Every Markdown text this tree compiles or draws is read by one reader,
`nc_markdown::parse` (`crates/nc-markdown/src/lib.rs`), into a typed tree, and the tree is
what a page is drawn from:

- **The subset is closed.** Headings, paragraphs, fenced code naming one of six kinds,
  lists of one marker each with lists under their items, block quotes, tables, the
  thematic break; code spans, emphasis, strong, links, a backslash before ASCII
  punctuation and three entities. Anything else is refused with a rule of its own, at the
  line and the column — in characters — it was read at: raw HTML and an autolink first, an
  image, a reference link, a setext heading, an indented block, a hard break, a tab or
  another control character.
- **No markup of a text's own reaches a reader of the tree.** There is no node for raw
  HTML, so none is passed through; what a node holds is text, its escapes and entities
  already read, for whoever draws it to escape as its own.
- **A link leads to one of three forms**: an address opening with `https://`, held to
  printable ASCII with no quotation mark or apostrophe, angle bracket, backtick,
  parenthesis or backslash; a relative path of plain
  segments ending as a text this tree links does, with an anchor or none; or an anchor of
  this page. No other scheme is read as a target — `javascript:`, `data:`, `file:`,
  `http:` and an absolute path among them.
- **The work is bounded by the text.** A list item or a block quote nests eight deep at
  most and is refused past it, so the reader's own depth is bounded; emphasis holds no
  emphasis and a link no link.
- **A text outside the subset fails the build.** The catalog compiler reads every wiki
  text written in Markdown and every page it emits through the same function, and writes
  nothing while one is refused (`markdown-outside-the-subset`, naming the file and the
  line); a person's words a page shows — a translator's sentence, a key's label — are
  escaped as text first, so no translation fails it; `cargo xtask release` writes each
  commit's words as text and refuses a changelog the reader refuses before writing it;
  and the pages `crates/nc-pages` embeds are read again by its own suite, the changelog
  among them.

**Enforced by** `crates/nc-markdown/tests/the_corpus_is_the_subset.rs`, which replays
every row of `crates/nc-markdown/tests/markdown-corpus.tsv` — a source and its tree, or a
source, where it is refused and by which rule — and holds every rule to a row that refuses
by it; the unit tests beside the reader, for the coordinates, the nesting bound, the six
kinds of fence and every form a target is refused in; the operators under `markdown` in
`xtask/mutants.py`; `a_markdown_wiki_text_outside_the_subset_fails_the_build_naming_its_line`
and `a_page_outside_the_subset_fails_the_build_naming_the_page` in
`crates/nc-catalog-compiler/tests/the_gates_fire.rs`, with
`a_label_that_is_markdown_is_escaped_in_a_cell` there for a person's words;
`every_subject_the_history_rule_accepts_is_its_own_words_in_the_changelog`,
`backticks_that_begin_an_entry_open_no_fence_though_they_close` and
`a_changelog_outside_the_subset_is_refused_before_anything_is_written` in
`xtask/src/release.rs`; and
`every_page_is_read_by_the_one_reader_and_opens_with_its_title` in
`crates/nc-pages/src/lib.rs`.

**Not answered:** see "What is deliberately not claimed": how long a text may be, which is
its caller's bound; what an `https` address leads to; and that another reader of Markdown
shows a text of the subset exactly as this one reads it.

### A tool is named as its command and read back to it alone, and its one hint is one the kernel holds

**Guaranteed.** A command's MCP tool is what `nc_schema::entry::as_tool` projects, and
nothing else maps a tool's name (invariant 27):

- **One spelling per tool, read back to one command.** `nc_schema::entry::tool_name`
  writes the command's name with each dot an underscore, the characters a model API takes
  in a tool's name where a dot is refused, at the command's own length. No command's name
  holds an underscore, so no two commands share a tool's name, and
  `nc_schema::entry::command_of_tool` reads a name back through the one command-name
  parser, `command_name_is_well_formed`: a dotted name — the command's own among them —
  two underscores in a row, one at either end, a capital letter, one word, and a name past
  the bound are no tool's, and whatever it reads back, `tool_name` writes as the text it
  read. So a call naming a tool reaches the command its name was made from, or nothing.
- **One hint, and one the kernel holds.** A tool's annotations are `readOnlyHint` alone,
  true exactly when the command is not `mutating`, and the kernel holds a command that
  does not mutate to writing nothing: no write to a store or to the blobs, no nested
  dispatch of a command that changes state, and no request but `GET` and `HEAD`. No
  `title`, which would be a display name, a person's words; and no other hint, which
  nothing the registry holds a command to could make true, so the protocol's defaults
  stand.

**Enforced by** the unit tests beside `as_tool` in `crates/nc-schema/src/entry.rs` — the
whole tool written out, every refusal of a name, and the hint both ways — with the
`schema_entry` operators of `xtask/mutants.py` over each clause; the two name laws in
`crates/nc-schema/tests/properties.rs`, over every name the grammar writes, at the bound
among them, and over texts made near a tool's name, held to be read and refused often;
and `the_mcp_page_projects_every_command_as_a_tool_and_says_what_a_session_sees` in
`xtask/tests/the_pages_reach_the_catalog.rs`, which reads every published name back to its
command and every hint against the catalog's `mutating`.

**Not answered:** a tool's name keeps its command's length, up to `nc_schema::NAME_MAX`
characters, which a host or a model API that bounds a name shorter cuts or refuses; and a
hint is a hint — `readOnlyHint` confirms nothing, and what a host does with it is the
host's.

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
  as the call tree that set or published it acted, for an agent's once for each link of
  its session's chain, the session spending one unit of its budget for a timer fired and
  one for an event delivered, whatever its subscribers — the authority it was set or
  published under; a denial is journaled and cancels the timer or drops the event.
- **A callback is a call tree of its own**, acting as that tree did, never confirmed, every
  command beneath it nested and never undo or redo; an event's callback, and a timer's set
  where only reading was allowed, change nothing and begin no unit, so putting a change
  off escapes no frame that would refuse it.
- **Supervised like a command**: a callback that traps is killed and restarted; a
  quarantined plugin's due timers wait and its events are dropped and counted; an unloaded
  plugin's timers are cancelled, its surfaces closed and its unheard events let go.
- **Who a call runs for is the kernel's to say**: `context` answers the principal's opaque
  identifier — in an agent's tree, the agent's — the actor chain every entry of the tree
  records, on whose behalf it acts, outermost first, and empty for a principal acting as
  itself, and whether a person confirmed the dispatch at the top of the tree — never
  nested, never in a callback, never in an agent's tree — and nothing a plugin writes
  changes them; in `activate`, the context, events, timers and surfaces are refused
  (`[NC-PLUG-0025]`).

**Enforced by** `crates/nc-kernel/src/events.rs`, `crates/nc-kernel/src/timers.rs`,
`crates/nc-kernel/src/pump.rs`, `crates/nc-kernel/src/units.rs` and the host interfaces
over them, with their unit tests, and `crates/nc-kernel/tests/the_rest_of_the_world.rs`,
which publishes, sets and pumps the reference plugin beside its peer — authority withdrawn
in between, a unit let go, a nested command that put something off and failed beneath a
caller that went on, a timer set by a read or an event's callback, a plugin quarantined,
killed or unloaded, and an agent's timers and events decided for it and its person, spent
from its budget, the chain a plugin reads held to the one the journal records.

**Not answered:** what a callback does with what it is told, which is its plugin's code; a
callback changing its own plugin's state, which it cannot — a command of that plugin
dispatched from it is a dispatch into itself — so a timer reaches its plugin's data only
through another plugin's command; timers and events across a restart of the kernel, which
keeps none on disk in this build, and when the pump runs, which is the daemon's schedule
(`pump_every_ms`); an event published or a timer set by a command that only reads and then trapped,
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

**Who rendered each tree is kept with it** (Stage 16): the plugin is the surface's owner,
and beside it the person the call tree acted for and, when an agent's session acted for
them — its command, or a timer's or an event's callback it put off — the agent, read from
the call tree and never from the plugin (`Renderer`). `surfaces.list` and `surfaces.read`
answer it, reads on `Feature::"interface"`. **A click is `surfaces.activate`**: the surface by
its identifier and the node by the JSON pointer a refusal of the tree names it by, a
`button` or a `toggle` not disabled, its command and its `args` read from the kept tree and
never from the caller, who gives only the text typed into each input the node binds, read
as the input's `kind` says. **A surface whose kept tree an agent's call tree rendered is
never activated** (`[NC-KERN-0035]`), whatever the node, since what it names would run
with the person's whole authority, chosen by the agent; a surface or a node not there is
`[NC-KERN-0036]`, and a node that runs nothing, one disabled or an input it does not bind
or cannot read is `[NC-KERN-0037]`. Otherwise the command is dispatched nested in the
activation's call tree, as the tree acts — decided, journaled, never confirmed, and
refused there when it runs only at the top of a call tree, a setting's switch and the
agents' changes among them — and the activation opens no unit of its own and leaves no
note of having left the machine (`dispatches_what_it_shows`), so the step is the command's
own, labelled by it, and one that only reads makes none.

**Enforced by** `crates/nc-kernel/src/ui.rs`, a test per rule; its interface in
`crates/nc-kernel/src/host/ui.rs`; `crates/nc-kernel/tests/the_rest_of_the_world.rs`, where
an agent's tree is kept as the agent's and a person's click on it runs nothing until the
person's own tree renders it again; `crates/nc-kernel/tests/units_and_history.rs`, where a
click's write is a step of its own that the history names and undo takes back, and a click
is neither a confirmation nor a call at the top; and
`crates/nc-kernel/tests/the_decision_point.rs`, where a read the journal did not take and
a unit it did not keep each show nothing they did.

**Not answered:** a renderer that dispatches a node's command by its name rather than
through `surfaces.activate`, which any client can do as the person — the desktop interface
is held to the activation by its own code, not by the kernel; a tree a person's call tree
rendered from what an agent wrote before — a plugin rendering from its own store, which an
agent's command may have written, renders as the person; whether this machine holds the
blob an image names, which is the renderer's to ask when it draws, since a blob can arrive
or be swept after the tree is kept; and what a tree says, which is its plugin's.

### What the dev pages read is decided like any command, bounded, and holds no line the log's types would not

**Guaranteed.** The part of the dev pages that is this machine's state (`NOCTUA.md §12.16`,
`§15.3`) is seven reads of the kernel's own (`crates/nc-kernel/src/dev.rs`), each decided,
journaled and answered as every command is, so none is reachable only from a window:
`dev.limits.read`, `dev.plugins.read`, `dev.sessions.read`, `dev.agents.read` and
`dev.policy.read`, a `Read` on `Feature::"interface"`, which the family pack gives every
person of the realm and no agent or device; and `dev.logs.read` and `dev.journal.read`, a
`Read` on the realm, which the pack gives a guardian alone, as it gives `journal.verify`. A
member who is not a guardian is refused the two records, and a frozen account, a device and
an agent — one its person granted the interface's feature included — are refused all seven,
each denial journaled.

**Every answer is bounded.** The plugins, the log and the journal are read a page at a
time — `from`, and `max` of `[kernel] dev_page_max` rows at most, the maximum each schema
declares — each answer saying in `next` where the next page starts, and a page ends before
its rows' text would pass half of `payload_bytes`, its first row apart. A page of the
journal is read through `nc_journal::Store::page`, a `LIMIT` in the statement of each store,
so it costs what it answers and not what the history holds; it holds this kernel's realm's
entries alone, and names an entry the store could not read instead of skipping it. A table
of the local database is counted row by row only where the catalog bounds it; the one it
bounds by no number, the local journal, is counted from the end of its key
(`nc_prune::sqlite::Sqlite::rows_to_the_end`) — its triggers land every entry at the next
sequence from zero and refuse every delete, so its last sequence plus one is its count —
because the limits read runs under the bus every dispatch waits on, and a count of every
row would cost the whole history each time.

**The log read is the daemon's own ring, and nothing reaches the ring that the log's types
keep out of the store.** `nc_log::Log::keeping` keeps the newest lines a log recorded, each
the one text `Log::line` makes of an event — the line the store is sent — and no line is
written into it any other way; the daemon's sink is made keeping `[daemon]
dev_logs_ring_lines` of them (`sink` in `apps/daemon/src/environment.rs`), and the kernel
copies what `LogSink::kept` answers and adds nothing. So a secret or a person's text is in
an answer only if an event's typed fields could carry it, which is what **The central log
cannot contain user data** refuses.

**What the daemon holds open is the daemon's to say**: the agents' sessions and the
connections are read from the server each time (`Host` in
`apps/daemon/src/environment.rs`, `nc_kernel::Hosting`), and a kernel told of no host says
neither count, as `null`, never as zero.

**Enforced by** the module's own tests — the seven held to the catalog both ways, each
page's bound to `dev_page_max`, the byte budget at its boundary, an entry and each count
written out — and, in `crates/nc-kernel/tests/the_decision_point.rs`,
`the_dev_pages_reads_are_decided_for_a_person_and_the_records_for_a_guardian_alone`,
`a_frozen_account_a_device_and_an_agent_read_no_dev_page`, the limits', the agents', the
sessions', the policy's, the plugins', the log's and the journal's own cases beside them,
and `a_journal_that_cannot_be_read_is_answered_unavailable_and_no_entry_of_it`; by
`a_page_is_the_first_entries_from_where_it_was_asked_and_no_more` and the three stores'
shared scripts for `Store::page`, PostgreSQL's order under a clustered heap among them; by
`the_rows_of_a_table_are_counted_whatever_a_delete_from_it_would_reach` and
`a_dense_table_is_counted_from_the_end_of_its_key_in_one_step` in
`crates/nc-prune/tests/in_sqlite.rs`; by the ring's two tests in `crates/nc-log/src/lib.rs`;
and, over the daemon, by `the_daemons_sink_keeps_its_newest_lines_within_the_catalogs_bound`,
`the_dev_pages_read_the_sessions_the_connections_and_the_log_this_daemon_holds` and, against
a daemon started as an install starts one,
`a_running_daemon_answers_its_dev_pages_the_sessions_it_holds_and_the_lines_it_logged` in
`apps/daemon/tests/agent_sessions.rs` — which fails once the start no longer tells the
kernel of its host. The seven run through the CLI in
`apps/daemon/tests/every_command_from_the_cli.rs`.

**Not answered:** the central log, which is read from its own store; the limits that are a
size or a time, which have no use to show and are `docs/limits.md`'s; and what **What is
deliberately not claimed** lists of these reads.

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
- **Which units are open is asked of four columns** — the realm, the unit, the action and
  the outcome, over the index `(realm, work_unit)` — so no row that fails to decode hides
  a unit opened after it, and a recovery visits the realm's entries inside units, decoding
  none of them but the unit each open one names, rather than every realm's history. An
  opening bracket whose unit is not spelled as this code writes one is refused by name,
  never skipped (`NC-JRNL-0001`).

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

### The agents a person made are kept as written: each row written once, an agent never changed and a grant ended once

`nc_local::LocalAgents` (`crates/nc-local/src/agents.rs`, whose header has the rules) keeps
the agents a profile's person made, and every grant they gave one, in the local file beside
the plugin store and the journal: `agents` and `agent_grants`, built by
`local-migrations/0005_agents.sql`, and never the world file a daemon starts from
(`NOCTUA.md §5.13`). It learns no type of the decision point's: an operator and a giver are
opaque identifiers, and a grant's scope is a type and an identifier as text.

**Guaranteed:**

- **An agent is written once and never edited or deleted, and a grant is written once,
  ended once and never deleted**, by triggers that refuse the statement whoever runs it,
  under `trusted_schema = OFF`: `BEFORE UPDATE` and `BEFORE DELETE` ones, and
  `BEFORE INSERT` ones that refuse an identifier the file holds before SQLite resolves the
  conflict — so an `INSERT OR REPLACE`, or a `REPLACE`, which would delete the row it meets
  and fire no delete trigger, is refused as a plain `INSERT` is. The end's trigger holds
  every other column of a grant as it was and an end to both when and by whom, so a
  connection that switches the `CHECK`s off with `PRAGMA ignore_check_constraints` still
  ends no grant without saying who ended it.
- **One live grant per agent and scope**, whatever statement writes it and whoever runs
  it: the grants' `BEFORE INSERT` trigger refuses a second live one before SQLite
  resolves a conflict, a plain `INSERT` and a replace alike (a replace would otherwise
  delete the live one to make room), and the file's partial unique index states the same
  rule beneath it. A grant that replaces the live one on its scope ends it in the
  transaction that writes it, and only that one: a replacement naming a grant of another
  scope, agent or realm, or one ended already, is refused with the new grant, and the old
  one stays live.
- **Every write is counted** through `write_exactly`, in a `BEGIN IMMEDIATE` that first
  checks no temporary object sits on its connection (invariant 22), and **each table
  refuses a new row at its bound**, `catalog/limits.toml`'s `max_rows`, counted over the
  whole table in the transaction that would add the row (`nc_local::KeepError::Full`): every
  realm's rows and every ended grant count, and a bound the catalog does not hold refuses
  every row rather than none.
- **A row read back is judged, or the read is refused** (`NC-DB-0007`): an identifier no
  minter gives, a realm or a person that is not an opaque identifier, a budget no agent
  holds, a time below zero, or a window that ends where it begins or before. A label, a
  reason and a scope are handed on as the file holds them, for `nc_authz::LocalDirectory`,
  which a daemon opens over the rows, to judge — and it refuses to start over one it would
  never have written.

**Enforced by** the unit tests in `crates/nc-local/src/agents.rs` — the triggers driven
around the store with the `CHECK`s on and off, a plain insert of an identifier held, an
`INSERT OR REPLACE` and a `REPLACE` of an agent, of an ended grant, of a live one and on a
live one's scope, a second live grant, a replacement refused half way and one naming a
grant of another scope, agent or realm, an ended one or none, a trigger silencing each of
the four writes, a temporary object, a full table, an ended grant read back with when and by
whom it ended, and rows planted with the checks off —
by the operators over `agents.rs` in `xtask/mutants.py`, each clause of the insert triggers
among them, and by `nc_local::schema::EXPECTED`, to which every open and `cargo xtask check`
hold the file's objects.

**Not answered:** a process running as the same user, which can drop the triggers and then
write, edit or delete any row — SQLite has no roles, and no row is signed: a trigger
dropped is a schema the next open refuses, and one put back before it is not seen — and
which can also rewrite in place, at the same length, a text column no index holds, such as
an agent's operator or a grant's realm (so its own realm no longer reads it), through
SQLite's incremental BLOB I/O, which runs no statement, fires no trigger and changes no
schema; who may make an agent, or give or end a grant, which this crate does not decide —
it keeps what it is handed; and the bounds being the profile's, under
**What is deliberately not claimed**.

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
in ordinary running, as `bounds`' own doc says. A log told of no store
(`Log::to_standard_error`, a daemon whose install names none) writes every line there
itself, in the shape and through the bounded writer a shipper's fallback uses, and its
`Drop` waits one `GRACE` for that writer as a shipper's does.

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
Leader, which is held apart across the three channels and only there, and `noctua-dev`,
the development channel's, which every checkout of one bases and namespace shares and
which meets no other name — a namespace is never eight hexadecimal digits, which is how a
checkout is written, so `noctua-dev-<namespace>` is never a checkout's CLI. Across two users
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
manifest carries, and `nc_channel::found` reads the manifest beside a canonicalized
executable once it and its directory are judged this user's alone, refusing no manifest
(`NC-CHAN-0006`), one another account can reach (`NC-CHAN-0007`) and a path the
filesystem does not answer for (`NC-CHAN-0008`). The checkout's digest and its rendering are the ones `cargo xtask dev`
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
`the_project_name_does_not_move_under_a_checkout` in `xtask/src/dev.rs`, the tests of
`crates/nc-channel/src/found.rs`, and the operators over those two sources and over
`nc-canon`'s refusal of a key written twice in `xtask/mutants.py`. `layout` takes an install and a user and nothing else.

**Not answered:** a process running as the same user, which can rewrite a manifest —
judging the file this user's alone is the reader's, and stops other accounts only; and
whether anything uses a name as derived, which is each consumer's — the daemon, the CLI,
the installer and the service manager's registration are what read a layout. What the
names leave narrowed is under **What is deliberately not claimed**.

### `noctua-dev` is the one name an install writes outside its own directories, and it replaces nothing it did not make

`apps/cli/src/chosen.rs` (whose header has the two ways it reaches a CLI) writes it;
`cargo xtask dev --install` and the `checkouts` verb ask it to.

**Guaranteed.** `noctua-dev` — `noctua-dev-<namespace>` for a test's install, which is
how no test's install names the developer's — is the one name a development install
writes outside the directories `nc_channel::layout` derives, in the user's bin directory,
.local/bin under the home on every system, read from the environment by the installer
alone (`nc_platform::bin_directory`) and recorded beside the manifest, so no binary reads
the environment for it. **The directory is this user's and written by this user alone**
(`nc_platform::judge_written_by_user_alone`): a directory another account owns or may
write in — a group or other write bit on Unix, an entry letting anybody but the user,
`SYSTEM` or the Administrators group write on Windows — a link and a file are refused,
named, and nothing is written in them; one that is missing is made, each level private.
**What stands at the name is replaced only when this install made it**: nothing, or a
symbolic link — on Windows a launcher whose pointer is this user's alone — to exactly the
CLI `layout` names for a development checkout of this install's bases and namespace; a
regular file, a link anywhere else, a launcher with no pointer and one pointing elsewhere
are refused, named and left as they were. The install judges it before anything is
stopped or written, so a refusal leaves an earlier install running and untouched, and
points it last. It is pointed through a new link or file made beside it, synced and
renamed over it, so it names the old CLI or the new one at every moment; what an
interrupted pointing left at that temporary name is removed only when it is what this
install writes there — a link or a pointer to such a CLI, or the very launcher being made
— and refused, named and left as it was otherwise. **A launcher runs
nothing it did not read privately**: its directory judged as above and its pointer this
user's alone, read no further than `RECORD_BYTES_MAX`, naming a plain absolute path,
before it runs that CLI with every argument and its standard streams, ending with its exit
code; one refused ends in `NC-CLI-0014` and exit 12, having run nothing. A copy of the CLI
with no manifest beside it is a launcher in a bin directory others may read, as a person's
commonly is, as in a private one: no install is read from a directory others may read, and a
copy beside a manifest is refused as an install, never run as a launcher. **A record or a
pointer is read only as written**: canonical JSON of one format with every key it holds
and nothing else, judged private first, refused otherwise. An uninstall leaves the name to
another checkout only when it points at the one removed, and leaves a name it would refuse
as it is. The `checkouts` verb lists every checkout with its texts escaped for the
terminal, refuses a digest no installed checkout has (`NC-CLI-0010`), and hands the path it
copies to the clipboard's tool on its standard input, never as an argument.

**Enforced by** the tests of `apps/cli/src/chosen.rs` — a name made, pointed again and
removed as a link and as a launcher, a regular file, a link elsewhere, a launcher with no
pointer and one pointing at another namespace's CLI each refused and left as it was, what
an interrupted pointing left beside the name cleared only when this install wrote it, a
bin directory another account may write in refused with nothing written, and a target
taken as a checkout's CLI only where the layout names one — of `apps/cli/src/install.rs`,
where an install makes the name and records its checkout, a second checkout's points it
there, uninstalls leave it to the checkout installed last or remove it, another
checkout's uninstall leaves it, and an install over a name it did not make refuses before
anything is written; of `apps/cli/src/checkouts.rs`, `apps/cli/src/launcher.rs`,
`crates/nc-channel/src/checkouts.rs` and `crates/nc-platform/src/lib.rs`, where a
directory another user may write in is refused on every system; by
`apps/cli/tests/the_launcher_passes_everything_through.rs`, which runs a launcher on every
system, in a private bin directory and in one others may read; by `noctua_dev_is_the_development_channels_and_meets_no_other_name` in
`crates/nc-channel/tests/the_names_are_disjoint.rs`; and by the operators over those
sources in `xtask/mutants.py`.

**Not answered:** a process running as the same user, which can replace the name, the
launcher or its pointer between a judgement and a run; what sits earlier on the person's
`PATH`, which is theirs to order; whoever no access list stops — root, `SYSTEM` and the
Administrators group; and the clipboard's tool, found on `PATH` (under **What is
deliberately not claimed**).

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
dead socket replaced and a file or a link at its name left as it was, a file put at a
listener's name while it lived left as it was when the listener is let go, a dead socket
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
asked — of another user, refused, or of this one; a Windows listener whose next
instance could not be made, which makes it again as a first instance and so is refused
while its own connections still hold the name; and on Windows another account making the
pipe's name first while the daemon is down — the name is the channel's and the user's,
which any account can spell — which keeps the daemon from starting (`NC-IPC-0011`, at
every start its restorer makes) though it is never joined or taken for the daemon, since
the client holds the pipe to its owner and the process at its other end.

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
  than handing the late answer to the next request, and one the connection could not send
  abandons nothing, so every call after the daemon is gone is refused as lost
  (`NC-IPC-0003`), which a caller may retry. Each side pings after `ipc_heartbeat_ms` of
  its own silence — while a dispatch runs, too — and drops a peer silent for
  `ipc_silent_after_ms` (`NC-IPC-0008`); and a write that waits that long ends the
  connection however the peer pings (`NC-IPC-0017`), the writer telling the reader, so
  both halves of the stream are let go. Every bound is the catalog's, read by the crate
  itself.
- **A session names its agent in the one spelling it has, and a request carries nothing
  else.** A session request holds one field, the agent as `to_crockford` writes the
  Snowflake the profile's minter gave it, read back only where it writes back the same —
  in lower case, with a digit Crockford reads as another or with a leading zero it is
  malformed (`NC-IPC-0002`); the tools are asked in a locale read as the registry's is;
  and a dispatch holds its command and its arguments and no field beside them, so no
  field the wire reads is a confirmation. A dispatch is decided as the connection's
  principal — the person, or the agent of the session it opened — which a build of the
  protocol before sessions would read otherwise, so that protocol was 3.
- **A watch, a note and a display are requests like the others, so the daemon sends no frame
  nobody asked for.** A watch names the revision its client has seen, or none, and how long
  it will wait, and its answer names the revision the watched state is at, with the state as
  one object or without it. A note holds one object, `ipc_note_max_bytes` of canonical text
  at most, made and read by one rule (`Note::new`). An announced environment
  (`Graphical::new`, the one rule it is made and read by) names some of `DISPLAY`,
  `WAYLAND_DISPLAY`, `XAUTHORITY` and `XDG_RUNTIME_DIR` and no other variable, one of the
  first two at least and each once, every value within `ipc_display_value_max_bytes`, not
  empty and free of control characters, and the two paths absolute — so nothing a client
  announces names another variable. A count below zero is malformed, and one past what the
  wire's integer holds is written as that integer: a wait is capped by the daemon whatever
  it says, and the daemon's revision turns to zero after that integer, so no two revisions
  are written as one number. A daemon of 3 reads each of the three as malformed and closes,
  so `PROTOCOL` is 4 and a peer of 3 is refused at the handshake (`NC-IPC-0004`), each end
  reading the other's version before a request is sent.
- **An attach, a confirm and a decline are requests like the others, and the desktop's
  alone.** An attach holds a proof of exactly 32 bytes, which the client makes of the secret
  its daemon handed it over this connection's own handshake nonces (`Secret::proof`), so a
  proof made over another connection's proves nothing; a confirm and a decline name one
  serial, a whole number not below zero, and nothing beside it. No field of any of the three
  is read as a confirmation of a dispatch. Joining protocol 4 as Stage 16 built it, a daemon
  of an earlier commit of it reads each as malformed and closes (*an IPC message*,
  **Making a change**). Who may ask them is the daemon's (*A call that cannot be undone
  runs once the person confirms it*).
- **One client for every program that asks a daemon.** `nc_ipc::client` (whose header has
  the checks in order) is what the CLI and the desktop interface connect through — the
  desktop's two connections, `client::follow` and `client::Asker`, each by its `connect`: the
  endpoint, the token judged before it is read, the handshake, the welcome held to the
  process the system shows and to the install's channel, and then each request with its
  answer held to the request's kind — a watch's answer that names another revision than the
  one seen and sends no state of it is refused as unread (`NC-CLI-0005`), since the client
  would hold a revision it was never shown. Its codes stay `NC-CLI-`, and what a refusal
  exits with is the CLI's own (`exit_of` in `apps/cli/src/client.rs`). `nc_ipc::testing`,
  the scripted daemon a client's suite connects to, is compiled only for tests and behind
  the crate's `test-support` feature, **off by default**, so nothing shipped can script a
  daemon through the crate.

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
the bound, from either end, after the pings, a write longer than the relay buffers left
waiting by a peer that pings ending the connection at the bound with the daemon's stream
let go, every request after the daemon is gone refused as lost, a client let go ending
the daemon's connection at a frame's boundary, a dispatch outlasting the bound waited
for, pipelining and an unasked answer refused, and a session and its tools crossing in
order, one written in another spelling refused by the end that reads it, a watch, a note
and a display crossing in order and one written wrong refused by the daemon's end
(`a_watch_a_note_and_a_display_cross_and_one_written_wrong_is_refused`), a watch's answer
whose state is not an object refused by the client's, and a note and an environment naming
every variable, each at its bound in the bytes JSON doubles, crossing in one frame
(`a_note_and_an_environment_at_every_bound_cross_in_one_frame`), and an attach, a confirm
and a decline crossing through a real `serve` and `connect`, the attach the proof over this
connection's own nonces, and each written wrong by hand refused by the daemon's reader
(`an_attach_a_confirm_and_a_decline_cross_and_one_written_wrong_is_refused`)); `handshake.rs` again
for the two versions (`a_client_and_a_daemon_of_protocol_3_are_refused_at_the_handshake`,
the numbers written out, the client having sent nothing but its goodbye) — every wait in
the three bounded, so an end that never comes fails the suite rather than spinning the
paused clock through its pings — by the unit tests of `crates/nc-ipc/src/message.rs` (a
session's agent read back only as written, the tools' locale read as the registry's,
no field a dispatch may hold read as a confirmation, a watch and its answer in every
spelling they have and each one they have not, a note at its bound and one byte past it,
made and read, and an environment refused for each rule it breaks), by the unit tests of
`crates/nc-ipc/src/client.rs` against the scripted daemon (each check before a request in
its order, every answer of another kind than its request named by a word of its own, and
`a_watch_is_answered_the_state_or_that_nothing_changed`), and the operators over the
crate's sources in `xtask/mutants.py`, the client's judged by its own unit tests.

**Not answered:** a process running as the same user, which can read the token from the
runtime directory and speak for anybody — authorization re-decides every dispatch (`§14`);
the confidentiality of a connection from such a process, which is the endpoint's: nothing
is encrypted; a peer slow to speak inside its bounds, which holds one of
`ipc_connections_max` for as long as it pings; and a peer that pings and reads nothing,
which holds one as well until a write waits on it — only once the daemon's writes fill the
system's buffer for the connection, which between requests, a ping each
`ipc_heartbeat_ms`, takes minutes on a Linux socket and hours on a Windows pipe. Only this
user reaches the endpoint, so both are a same-user residual. Who writes the token and the
process it is read by are the daemon's and the CLI's, under **What is deliberately not
claimed**.

### A daemon starts only on a profile that is its user's alone, and mints nothing it cannot resume above

`apps/daemon` (`profile`, `device` and `mark`, whose headers have the rules), with
`nc_channel::found` for the install, reads what a start stands on, in order, and refuses
with a code of the `CHAN` or `DMN` area at the first thing it cannot vouch for.

**Guaranteed:**

- **An install names itself, and nothing else does.** The manifest is read through
  `nc_channel::found`, the one reader the CLI beside the daemon reads it through too:
  beside the canonicalized executable, in a directory judged this user's alone by
  `nc_platform::user_in` — the one way to the user a start runs as — each time the install
  is read; it is judged so itself with `nc_platform::judge`, and read only as
  `nc_channel::Install::parse` reads it. A binary with none refuses (`NC-CHAN-0006`) rather
  than fall back to a channel built in, a manifest another account can reach, or one in a
  directory another account can reach, is not read (`NC-CHAN-0007`), and a path the
  filesystem does not answer for is its own refusal (`NC-CHAN-0008`) — so the daemon and
  the CLI refuse one manifest with one code.
- **The profile is the user's alone from its first moment.** Its data, configuration,
  cache and state directories, and the log and runtime ones under the state one, are each
  made private one level at a time — never a parent on the way — or judged so, and
  anything else refuses the start with it left as it was (`NC-DMN-0001`); so is every file
  of it a start reads — the seed, the device's record, the world and the mark — judged
  before it is read, and the data and configuration directories `--initialize` writes a
  seed and a world into, judged before anything is written there, with the same code. The
  plugins directory under the data one is made or judged with the rest, and each plugin in
  it is judged before it is read (the daemon's entry below).
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
  `NC-DMN-0006`); the realm's epoch, a whole number above zero; and the grant the person
  holds, when the file names one — one of the world's `Grant` entities inside that realm.
  An epoch missing or of zero, a grant the realm does not hold and a key the file does not
  define are each refused (`NC-DMN-0006`), and a world naming no grant is a person who holds
  none. The copy is written beside it and renamed over it; one an interrupted copy left
  there is removed only once it is judged this user's alone (`NC-DMN-0001`).
- **No identifier is handed out past a ceiling that is not yet durable.** The minter asks
  the mark to raise its ceiling `mark_lease_ms` past the clock before it hands out a
  millisecond past the one it holds, and refuses before any identifier exists when the
  write fails (`NC-IDS-0009`). The mark is two checksummed slots overwritten in place and
  synced, so a torn write spoils one and the other holds the ceiling nothing was minted
  past; with neither whole the start refuses (`NC-DMN-0007`). The lease is the catalog's,
  which the mark reads itself, so no caller chooses how far a raise reaches. A start
  resumes above the larger of the mark and the newest unit its local database keeps —
  the largest identifier any journal entry names, in every realm
  (`nc_local::LocalJournal::newest_unit`), or the largest unit the store committed
  (`nc_local::LocalStore::newest_unit`), a reversal's among them, which the kernel mints
  itself and no entry names — so a mark lost or rolled back is still above every unit
  kept; it waits for
  a clock behind it — logging `ids.clock_backwards` past the lease — and refuses a mark
  more than a day ahead; and a clean stop settles the ceiling (`mark::settled`, through
  `nc_ids::Minter::settle_ceiling`) to the exact high-water mark, so a clean restart waits
  for nothing.

**Enforced by** the tests of `crates/nc-channel/src/found.rs` — a missing, reachable and
malformed manifest, a manifest in a directory another account can reach refused and left as
it was, a manifest read through a link beside the file that runs — by
`no_install_is_exit_12_and_the_daemon_refuses_it_with_the_same_code` in
`apps/daemon/tests/exit_codes.rs`, where a daemon and the CLI refuse one missing and one
reachable manifest with one code, by the world file's tests in
`crates/nc-authz/src/directory.rs` — its grant held by its person alone, its epoch the
realm's, and each way an epoch or a grant is refused — by the unit tests of each module in
`apps/daemon/src/` — the profile's data, configuration, cache, state, log and runtime
directories each made private and judged again, a missing base not made on the way, a
world copied, replaced and
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
`crates/nc-ids/src/snowflake.rs`, by
`the_newest_unit_is_the_largest_any_entry_names_in_any_realm` in
`crates/nc-local/tests/the_local_journal.rs`,
`the_newest_unit_is_the_largest_the_store_holds_of_any_realm_and_kind` in
`crates/nc-local/tests/a_unit_lands_whole_or_not_at_all.rs` and
`a_start_reads_the_newest_unit_its_journal_holds` in `apps/daemon/src/environment.rs` — a
reversal's unit the store committed and no entry names among what it reads — by
`a_settled_mark_is_the_last_millisecond_handed_out` in `apps/daemon/src/mark.rs`, by
`a_run_resumes_above_the_newest_unit_kept_and_a_clean_stop_settles_its_mark` in
`apps/daemon/src/run.rs` — the daemon's own run, in a child of its test binary, over a
store holding a unit further ahead of the clock than the lease and no mark: the wait
logged once, the unit a dispatch opens above the one kept, and the mark a clean stop
leaves exactly that unit's millisecond, so the run's own wiring of the mark is held by the
unit tests the operators run — and by the operators over those sources in
`xtask/mutants.py`, each judged by its own module's unit tests.

**Not answered:** a process running as the same user, which can rewrite the manifest, the
world or the mark, or delete the mark so that a start resumes from the local database
alone — the newest unit its journal names, or a reversal's its store committed;
a slot spoiled after it answered, which reads as a torn one and falls back to the older
ceiling; a crash inside the mark's very first write, which leaves a file with neither
slot whole, so the profile does not start until it is removed by hand; a crash inside the
seed's first write, which leaves a seed of another length that no start and no
`--initialize` reads (`NC-DMN-0004`), so the profile does not start until it is removed by
hand; a disk that answers a sync before the bytes are on it; and an administrator. What
the daemon leaves undone is under **What is deliberately not claimed**.

### A daemon serves its kernel to its user alone, one to a profile, and to nothing a network reaches

`apps/daemon` (`run`, `server`, `environment`, `plugins`, `schedule` and `pidfile`, whose
headers have the rules) runs what the entry above reads: a start in the order `run`'s
header gives, the endpoint served, and a stop.

**Guaranteed:**

- **One daemon to a profile.** A start takes the instance lock in the profile's state
  directory at once or not at all (`nc_platform::try_hold`), before it reads the device or
  writes anything of its run, and a second refuses (`NC-DMN-0008`) with the first left as it
  was — so no two kernels take each other's open units for a stopped process's, and no two
  minters share a flake. The lock is never the data directory's, which each open of the
  local database holds while it opens.
- **The endpoint is the only door, and nothing reaches the kernel before it is proven.**
  A connection is accepted only once the system shows its other end to be this user's, and
  served only once the client proves it holds the token (the two entries above); each
  refusal of either is logged as `ipc.refused` with its code, and closes the connection
  before a request is read. A proven connection that then sends what is not a request,
  falls silent past `ipc_silent_after_ms`, takes nothing or is lost is closed and logged
  the same way; one its client closes between requests ends well. The daemon binds nothing
  a network can reach (invariant 23). A dispatch, the registry, the tools or a session
  asked before the kernel runs is refused as not running yet (`NC-DMN-0009`) and one asked
  after a stop began as stopping (`NC-DMN-0010`); the status and a stop are answered from
  the first moment, and every answer handed to a connection is written before the daemon
  lets it go.
- **Every dispatch is the kernel's to decide, as the connection's principal.** On a
  connection that opened no session that is the person the world names; on one that opened
  an agent's session, that agent acting for the person (the entry below). The daemon hands
  `Kernel::dispatch` that principal and the arguments as they arrived, decides nothing
  itself, and hands back the kernel's answer or its refusal by the kernel's own code and
  kind — the journal holds each, whoever asked. The directory the kernel decides with is
  `nc_authz::LocalDirectory`: the world's, holding the grant and the epoch the world file
  names, and what the profile's database keeps of the agents of the world's realm, each
  change to them kept there (`apps/daemon/src/environment.rs`).
- **The token is fresh at each start, and the record names a process, not a number.** The
  token is drawn from the system's generator and the PID record holds the process as the
  system knows it — its boot, its start, its executable — and the instance the start drew,
  each written into a private file beside it, synced and renamed over the last, the token
  first so a record naming the process says the token beside it is that start's. A record
  naming process zero or below is refused as a record (`NC-DMN-0013`), never read as one;
  a file a write left beside either is removed only once judged this user's alone; and a
  clean stop removes the record only where it names its own instance, and the token.
- **A plugin is installed by its user and refused alone.** The plugins directory, each
  plugin's directory, its manifest, its entries (`commands.toml`, `i18n/` and each file of
  it) and its component are judged this user's alone before they are read; one refused —
  by that judgement, its manifest, its entries or the kernel — is logged as
  `daemon.plugin_refused` with its code and shown in the status with its sentence, and the
  daemon runs without it. What the status keeps of that sentence is its opening, at most
  `nc_daemon::plugins::REFUSAL_BYTES_MAX` bytes cut on a character and ended with `…`,
  whoever refused it, so one plugin's files cannot take the status past a frame and
  `noctua status` away with it.
- **The schedule is the catalog's, and a stuck pruner is escalated.** The pump runs every
  `pump_every_ms` and the blob sweep every `sweep_every_ms`, between call trees, each to
  its end before the next is waited for; a run of pumps whose pruning was refused is logged
  as `daemon.prune_stuck` at two and each doubling, and a clean one ends it.
- **The central log is the install's, and goes nowhere the install did not name.** An
  install naming no store has the daemon's lines written to its standard error alone
  (`nc_log::Log::to_standard_error`), and a store that does not answer has them written
  there by `nc_log`; an install naming a store `nc_log` refuses — one off this machine —
  does not start (`NC-DMN-0011`). A start and a stop are logged (`daemon.started`,
  `daemon.stopped`, with what a stop gave up on).
- **A stop leaves nothing of the run but its effects.** New dispatches are refused — each
  counted as running before the state is read, so a stop either refuses one or waits for
  it — and no pump and no sweep begins. The stop then waits `stop_drain_ms` at most — at
  least one call tree's whole deadline, which the compiler holds — for the dispatches
  running, for a pump or a sweep it finds inside a run, and for every connection to write
  its last answer, the stop's own among them, and close (`nc_ipc::Served::close`): each
  connection is counted before it is served, answered the first request it makes, and
  ended between two requests. Then the mark is settled, the local database closed and the
  record removed; then the socket goes with the listener, the token is removed, and the
  lock let go. **Until the record is removed the endpoint answers**: a connection made
  while the stop drains is told the daemon is stopping — a stop asked again answered
  `stopping`, a status saying so, a dispatch refused (`NC-DMN-0010`) — so a stop asked
  again, a restart's or a reinstall's, waits for the daemon as the first did and signals
  nothing, and one asked once the record is gone finds a daemon stopped — on Windows, once
  no connection the daemon still serves holds its pipe (**What is deliberately not
  claimed**). A stop asked
  while the start is under way waits for the connections alike.
- **A restorer's start honours a stop.** `noctua-daemon --restored`, the entry every
  restorer starts it by, reads the `stopped` mark before anything else of the profile —
  the state directory and the mark judged this user's alone, one another account can
  reach refused (`NC-DMN-0001`) rather than read as a stop — and ends at once with exit 0
  while it is there, or while another daemon holds the profile; otherwise it is a start
  like any other (`nc_daemon::run::restore`).
- **A spawned daemon holds none of its spawner's handles**: a suite starts it with its
  standard input and output null and its standard error a file in its profile, after
  keeping this process's own standard handles from every child
  (`nc_platform::process::keep_standard_handles_from_children` — on Windows a child is
  otherwise handed every inheritable handle its parent holds); and a daemon that does not
  lead its process group starts a session of its own (`nc_platform::leave_the_session`),
  so no terminal it was started from signals it.
- **A person's connection watches what the daemon holds of the interface, and the daemon
  answers it within a bound.** The daemon holds one watched state at a revision (`Watched`
  in `server.rs`, whose header has the rules): the graphical environment a person's
  connection announced last, the interface's state as the kernel holds it, and what came of
  the daemon's asking for the Leader where it is the process that asks — the first and the
  last its own, this process's alone, journaled and stored nowhere and gone with the daemon;
  the interface's state the kernel's, kept between starts and saved last at a stop. The
  kernel's state is taken in when a watch is asked and each time the kernel tells it moved
  (`nc_kernel::Kernel::shell_moves`), the move marked seen before the state is read, and
  only when its revision is newer than the one taken in — two watches each read it outside
  the channel's lock, and the slower one's read is the older — so a state newer than the
  revision a watch names answers it, and none older ever replaces it. A watch that has seen another
  revision than the state's, a later one as much as an earlier, is answered the state at
  once; one that has seen it waits for the first change, or the shorter of the wait it names
  and `ipc_watch_wait_ms`, and is then answered the revision alone; any number of
  connections watch, and one change answers them all. A watch and a note are refused before
  the kernel runs (`NC-DMN-0009`) and once a stop began (`NC-DMN-0010`), a watch that was
  waiting when the stop began among them, at that moment, so a stop waits for no watch; a
  display is taken from the first moment. **A note is the kernel's to apply**
  (`nc_kernel::Kernel::note`): admitted as a dispatch is, each fact it says — `draft`,
  `focus`, `hover`, `scroll` — read as the arguments of its command equivalent (`prompt.set`,
  `shell.focus`, `shell.hover`, `page.scroll`), held to that command's schema, decided for
  the person as that command would be by the probe that journals and spends nothing, and
  made by that command's rule, whole or not at all, between two call trees, journaled
  nowhere; what the kernel refuses is the answer. A display that says what is held already
  moves no revision, and neither does a note that changes nothing. Nothing of a connection
  is read while its watch waits, so a watcher that went away is found only when its answer
  is written — which is why the compiler holds the wait below the silence a peer is dropped
  at.
- **Where the daemon is the process that asks for the Leader, it asks on a thread of its
  own and taps for each press** (`apps/daemon/src/leader.rs`, `nc_ipc::leader`): on
  Windows, for an install that is not a test's, the Leader in force is registered on a
  thread that holds the registration until the stop gives it back, each press is
  `leader.tap` dispatched as the person and admitted as every dispatch is, and what came of
  asking — held, refused or lost — is held in the watched state and the status, a refusal
  logged as `daemon.leader_refused` by its code. On Linux and macOS the desktop interface
  asks (Phase D of Stage 16), and the daemon asks for nothing.

**Enforced by** `apps/daemon/tests/ipc_refusals.rs` — a real daemon process handed a hello
with no proof, a proof keyed by another token and nothing at all, each turned away with its
code, each logged once, a dispatch sent after each never read, the journal empty afterwards
and the daemon still answering a client that proves itself — by
`apps/daemon/tests/serves_its_kernel.rs` — the registry held both ways to the catalog's
commands with the reference plugin's beside them, a plugin refused shown and logged, one
refused for an entries file of malformed rows at its bound shown in a status that still
answers within a frame, every
dispatch found in the journal with the outcome it was answered with, as the world's person
and with the grant and the epoch the world names, the pump on its schedule and the sweep
once, a second daemon refused with the first still answering, a stop that ends with exit 0
and leaves no record, token, socket or lock and
a profile that starts again with a new instance, a spawner whose output ends while the
daemon it started runs, and a daemon that leads a session of its own and not the suite's
(`the_daemon_leads_a_session_of_its_own`) — by
`apps/daemon/tests/every_command_from_the_cli.rs`, where every command the catalog declares
and every command of the reference plugin is run through the CLI and found in the journal,
each decided by a rule of the realm's pack with the world's grant and epoch — by the unit
tests of `server` (among them
`a_watch_and_a_note_wait_for_the_kernel_and_a_display_does_not`, over every state of a
start; `a_watch_waits_the_shorter_of_its_wait_and_the_bound`,
`a_move_of_the_interface_or_a_display_answers_every_watch_and_what_is_held_none` and
`a_watch_waiting_when_a_stop_begins_is_refused_as_stopping`, each wait measured on a
paused clock over an interface's state the test moves; `a_revision_turns_to_zero_after_the_wires_largest_integer`;
`a_persons_connections_watch_note_and_announce_through_the_daemon`,
`a_note_the_kernel_refuses_is_answered_with_its_refusal_and_none_is_journaled` and
`a_watcher_that_went_away_is_found_only_when_its_answer_is_written`, through connections
the server serves over a kernel opened as the daemon opens one;
`the_leader_the_daemon_holds_is_said_held_and_each_press_is_the_persons_tap` and
`a_leader_refused_or_lost_is_held_in_the_watched_state_and_the_status`, over a
registration the test grants, refuses or loses, and
`a_leader_refused_or_lost_is_logged_by_its_code`, through the test binary run as a child
whose standard error is read;
`a_dispatch_is_counted_before_its_state_is_read` and
`a_stop_is_answered_whole_before_its_connection_is_let_go`, whose server runs on a runtime
dropped as soon as the stop has wound down, and whose client takes one byte at a time),
and `a_stop_asked_again_while_it_drains_is_answered_that_it_is_stopping`, through a real
endpoint), `pidfile`, `plugins` (`what_load_answers_of_a_refusal_past_its_bound_is_kept_within_it`,
through `load` beside a kernel opened as the daemon opens one), `schedule`
(`a_stop_waits_for_the_run_it_finds_and_begins_none_after`, over thirty-two schedules,
and `a_run_still_going_at_the_bound_is_given_up_on_and_counted`), `environment` and `run`
(`a_restored_start_stays_down_on_the_mark_and_beside_a_daemon_up`,
`a_run_resumes_above_the_newest_unit_kept_and_a_clean_stop_settles_its_mark`, where a
stop asked again while a real daemon drains is answered that it is stopping and the
daemon ends well, and `a_stopped_runs_record_is_removed_before_its_endpoint_is_let_go`,
where the record is gone while the end of the run still waits on a connection held
open), by
`a_connection_closed_has_sent_its_last_answer` in `crates/nc-ipc/tests/heartbeat.rs`, by
`a_log_told_of_no_store_writes_every_line_to_stderr` in `crates/nc-log/src/lib.rs`, by
`a_directory_held_is_not_held_again_at_once` and
`a_detached_process_leads_a_session_of_its_own` in `crates/nc-platform/src/lib.rs` — which,
with the daemon's own, stands aside on Windows, where no process has such a session, by
`nc_platform::system::POSIX_SESSIONS` — by invariant 23's gate, and by the operators over
those sources in `xtask/mutants.py`, each judged by its own module's unit tests.

**Not answered:** a process running as the same user, which can read the token and dispatch
as the person, stop the daemon, or install a plugin — authorization decides every dispatch
whoever asked (`§14`); a daemon killed, which runs no handler and leaves its record, its
token and its socket for the next start to replace; a daemon that leads its process group
— started by hand as a job of a shell, or spawned as a group's leader — which cannot start
a session and stays in the one it was started in, where that terminal's hangup reaches it;
a stop asked while the start waits for its clock, which abandons the start and leaves the
mark as the last raise wrote it, and whose process ends only once that wait is given up —
`stop_drain_ms` at most, its record and its endpoint already gone and its instance lock
still held, as through any blocking work the runtime's shutdown waits for, a plugin's
`net#fetch` past its call tree's deadline among it: a start meanwhile waits for the lock
before it starts one, and only commands run at once read that tail otherwise, signalling
nothing (the CLI's answer, below); a dispatch, a pump or a sweep still running past
`stop_drain_ms` — trees run one after another, such as a pump's callbacks, that outlast
the drain together — which the stop gives up on, counts in `daemon.stopped`, and closes
the local database under, and whose unit the next start recovers; a connection whose
client has not taken its last answer by that bound, which goes with the daemon's runtime,
the answer unwritten and uncounted; and a client running as the same user that connects and
never sends its first request, which keeps every stop waiting `stop_drain_ms`, since each
connection is owed an answer to its first request. What the daemon leaves undone is under
**What is deliberately not claimed**.

### An agent's session opens only while MCP is on, for an agent of the profile's person, and asks for its tools and its dispatches alone

`apps/daemon/src/server.rs`, whose header has the order, opens the session `noctua mcp`
asks for (`NOCTUA.md §20.3`) and serves it on the connection that asked.

**Guaranteed:**

- **Bound once, from the person, and never back.** A connection is the person's until it
  opens a session, and that session's agent's from then on: only the person's answerer
  opens one, so nothing a session sends rebinds its connection, and a second session asked
  on it is refused as any request no session is answered.
- **Opened in order, the first refusal the answer, and a refused one's connection
  closed.** The kernel must run (`NC-DMN-0009`, `NC-DMN-0010`). The session is counted
  among `[daemon] sessions_max` before anything else is read, so two racing for the last
  place cannot both have it (`NC-DMN-0016`), and the bound is below `ipc_connections_max`,
  which the compiler holds, so the person always finds a connection. MCP must be on
  (`NC-DMN-0014`), read from the kernel's store, where a switch that could not be read
  refuses with the store's code and is never read as on. And the kernel opens the session
  from its directory's answer, for an agent the profile's person made and no other
  (`NC-AI-0001`, whether none is held under the identifier or it is another's, or
  `NC-POL-0006` when the directory does not answer). Every refusal closes the connection
  once it is written, so no client that was refused goes on as the person.
- **A session asks for its tools and dispatches commands, and nothing else.** While MCP is
  on, the registry, the status, a stop, a second session, a watch, a note, a display, an
  attach as the desktop interface and a confirm or a decline of a held call are refused
  (`NC-DMN-0015`) and the session stands: an agent never stops the daemon, reads the
  person's status, learns what the person's interface shows, tells the daemon anything as
  one, attaches as the person's desktop or answers what waits for its person.
  Its tools are the kernel's probe of what its agent may dispatch now
  (`Kernel::dispatchable`), which journals and spends nothing; its dispatches are the
  kernel's, decided for the agent and its person and journaled with both, each admitted as
  running as the person's are, so a stop drains a session's dispatch too.
- **Off ends it.** Once the kernel runs, MCP is read again before each of a session's
  requests, before what it asks is read, so once it is off — by the person's set, undo or
  redo alike — or its switch cannot be read, the session's next request, whatever it asks,
  is refused (`NC-DMN-0014`, or the store's code) and the connection closed once the
  refusal is written. **And once its held call is confirmed**: a session's call held for
  its person's confirmation (*A call that cannot be undone runs once the person confirms
  it*) waits past the request it was read in, so MCP is read again once the person
  confirms it, before the kernel decides it again — off, or unreadable, refuses the call
  the same way, run nowhere and journaled nowhere, and ends the session. A request made
  while the kernel does not run yet or is stopping is refused as the person's is
  (`NC-DMN-0009`, `NC-DMN-0010`), the session standing.
- **What is refused here is logged, never journaled.** Each refusal of an opening, each
  request no session is answered and each ending for MCP off or its switch unreadable is
  logged as `daemon.session_refused` by its code — no command was dispatched — and a
  session opened and ended as `daemon.session_opened` and `daemon.session_ended`, with
  its agent.

**Enforced by** the unit tests of `server` — every request a session may and may not ask,
through a match with no wildcard; a session refused before the kernel runs and once a stop
began, each ending its connection; a refused session's next request lost on a connection
the server serves, while a person's refused registry leaves theirs going; the switch on,
off and unreadable; a session's dispatch counted as running until it answers; the bound
and its places given back; over a kernel opened as the daemon opens one, a session refused
for an agent no person made and while MCP is off, a connection bound to its agent until
MCP is switched off, a session's status refused with the session standing while MCP is
on and ending it once MCP is off, and — deciding with a pack that lets an agent switch MCP,
which no shipped pack does — a session's call held while its person switched MCP off,
refused and ending the session once confirmed
(`a_sessions_call_held_while_mcp_was_switched_off_is_refused_once_confirmed`); and, over a daemon a child of the test binary serves, a
session's opening, each refusal of it and its end logged in order, by its agent or its
code — by `apps/daemon/tests/agent_sessions.rs` against a real daemon — off until switched
on and ended once off again, every refusal with no command dispatched logged and nothing
of it journaled, the session that opened logged once as opened and once as ended with its
agent and the agent no person made never as opened, every dispatch journaled with the agent and
the person, an agent written in another spelling refused by the reader, a revocation
through the CLI seen at the next call, the agent held to its own rights, its tools exactly
what it may dispatch, its budget its own, the bound, a restart, and a plain connection
that is the person — by
`an_agent_and_its_grants_are_held_again_after_the_daemon_starts_again` in
`serves_its_kernel.rs`, and by the operators over `server.rs` in `xtask/mutants.py`.

**Not answered:** a connection that opens no session, which is the person: a process
running as the same user can open one and dispatch as the person, or name any of the
person's agents, whose identifiers are no secret (`NOCTUA.md §11.1`, and **What is
deliberately not claimed**); an idle session once MCP is off, which keeps its connection
and its place until its next request, which is refused and ends it; and a dispatch
admitted while MCP was on that waits on the kernel's bus behind the switch-off, which
runs.

### `noctua mcp` writes nothing but the protocol, sends every call that names a command, and confirms nothing

`apps/cli/src/mcp.rs`, whose header has the order, serves one MCP client on the CLI's own
standard input and output as the session of the agent its command line names
(`NOCTUA.md §20.3`).

**Guaranteed:**

- **Standard output carries the protocol alone.** One writer, the session's loop, through
  one door, `standard_streams` in `apps/cli/src/main.rs`: each answer is one compact line
  of JSON-RPC, written and flushed whole, a response to an id the client sent — or to
  none, for a line whose id could not be read — and never a request of the server's.
  Every refusal of the command line and of the install is said on standard error,
  `--json` or not (`renders_json`), and why a session ended is said there once.
- **A session opens at `initialize`, for the agent named and no other.** The command line
  names the agent by the identifier `agents.create` answered, read back only as
  `to_crockford` writes it (`NC-CLI-0001` and exit 2 otherwise); nothing is asked of the
  daemon before `initialize`, which opens the session (`client::session`), and a refused
  opening is one `-32603` answer carrying the refusal's code, the same refusal on standard
  error, and the run ended in its exit. One process is one session and one connection.
- **The legacy era, tools alone, and nothing asked of the client.** A session is answered
  in `2025-11-25` or `2025-06-18`, and a revision asked that is neither in the newest;
  server/discover and every method not offered are `-32601`; the capabilities are
  `{"tools":{"listChanged":false}}`, with no `instructions`; a notification and a
  client's response are never answered; and no request is ever written to the client.
- **No field is read as a confirmation.** A call reads `name`, `arguments` and `_meta` and
  refuses every other field with `-32602` — a task, a client's input responses and a
  request state among them; `_meta` is never read; the values acted on are `nc_canon`'s
  reading of the whole line, so a key written twice or a fraction is a tool error with
  the reader's code, never sent; and arguments that are not one object are a tool error
  (`NC-VAL-0002`), never sent, since the wire closes a connection on them.
- **Every call that names a command with object arguments is sent, listed or not.** A
  name that `nc_schema::entry::command_of_tool` reads back to no command is
  `-32602 Unknown tool` and never sent; every other call whose line `nc_canon` reads and
  whose arguments are one object (absent or `null` read as `{}`) is the agent's
  dispatch, decided, spent and journaled with the agent and its person. A refusal of a
  tool the session's last list did not hold, or of a command gone from the registry
  since, is `-32602 Unknown tool` with no code and no detail, so a denied command and an
  absent one read alike; a refusal of a listed tool is a tool error with its code, the
  catalog's message in the session's language and its detail; and the daemon's own
  refusals are shown as they are, MCP switched off ending the session in its code.
- **What a person cannot see is escaped.** Every answer's canonical text, every
  refusal's detail and every tool's line and paragraph are written with each character
  `nc_schema::unseen` answers for as `\u{…}`, as the CLI prints them; a tool's schema is
  the one `nc_schema::Schema::compile` judged.
- **Bounded, and off the runtime's thread.** A line is at most `[daemon]
  ipc_frame_max_bytes`, the one bound a request could ever be sent within; requests are
  read and answered one at a time, in the order written; and each line is read, and
  each answer written and flushed, on tokio's blocking pool, so the connection's
  heartbeat runs while the client is silent. Its input ending is exit 0, an input that
  cannot be read exit 1 (`NC-CLI-0015`), an output that takes nothing exit 1
  (`NC-CLI-0006`), and the daemon's connection lost exit 9.

**Enforced by** the unit tests of `apps/cli/src/mcp.rs` — the engine line by line, and the
loop against a daemon in-process that answers as a script says: every revision asked,
nothing but a ping answered and nothing asked before `initialize`, every method not
offered, notifications and responses unanswered, each malformed line by its own code, a
carriage return, the closed fields of a call and `_meta` never read nor sent, an
unlisted call sent and its refusal an unknown tool, a list filled before a first call, a
name that reads back to no command never sent, the one reader's refusals, arguments
that are not an object, every tool projected in the daemon's order and a list this build
cannot read refused whole, refusals in the session's language, every text escaped, every
frame one response to an id sent, a refused opening, no daemon, requests in order, a
connection lost, a switch the daemon could not read seen as that connection lost at the
next request, input and output that fail, and the bound — by the tests of `args.rs` and
`run.rs` for the line and its refusals, by
`apps/cli/tests/the_mcp_server_writes_nothing_but_the_protocol.rs` over the shipped
binary's doors, by the in-house client in `apps/daemon/tests/agent_sessions.rs`, written
from the protocol's `2025-11-25` schema, against a real daemon — off until switched on,
the naming round trip, the tools in the environment's language, every call journaled as
the agent's, a revoked or spent session still journaled, no field a confirmation, an
idle session kept, and a stop ending the session in 9 — and by the `cli_mcp` operators of
`xtask/mutants.py`.

**Measured against a live daemon, with a client this repository did not write.** On
2026-11-22 the MCP Inspector CLI 2.15.0, on Node 26.13.1 on Linux, ran
`noctua mcp <agent>` from a debug build of this tree at commit 1615805, beside a scratch
staging install made as the daemon's suites make one, with the suites' world: MCP switched
on, an agent made with a budget of 50 and granted `naming` for an hour, each through
`noctua`. Each of its runs asked for `2025-11-25`; each made while MCP was on was answered
in it, with `{"tools":{"listChanged":false}}` and no `instructions`, and the frames recorded
between the two ends show standard output carrying one JSON-RPC answer a line and nothing
else.
Its tool list held `naming_propose` and `naming_read` alone, each with `readOnlyHint`
true, in English, and in Portuguese under `LANG=pt_BR.UTF-8`, and its `--strict` check of
their schemas reported nothing. `naming_propose`, called with the title `Inspector run`,
the keywords `mcp` and `agents` and the extension `md`, answered
`--inspector-run__mcp_agents@@<identifier>.md` with an identifier minted for it, and
`naming_read` of that name answered it canonical, with the same identifier, title, keywords
and extension. Once
`settings.mcp.set --on=false` had run, the next run's `initialize` was answered with one
`-32603` frame carrying `NC-DMN-0014`, the refusal was said on standard error, and
`noctua mcp` ended in 3. The daemon logged each session as opened and as ended with its
agent, and that refusal by its code. That client refused on its own a call of a tool its
list did not hold, so the run sent none; the in-house client does.

**Not answered:** a process running as the same user, which can run this binary as any of
the person's agents, whose identifiers are no secret, or reach the daemon as the person
(`NOCTUA.md §11.1`); a call the client cancels, which still runs to its end and is
answered; text a person can read that steers a model — a description, an answer, an
argument echoed back; a host that shortens a tool's name or refuses its schema;
conversations a host interleaves over one process, which share its one session (all
under **What is deliberately not claimed**); and why the daemon ended a session whose
switch it could not read: it refuses with the store's own code, which a dispatch's
refusal can carry while its session stands, so the bridge answers that call as any
refused one and learns of the ending at its next request to the daemon, as the
connection lost (exit 9), or not at all when its input ends first (exit 0).

### A program registered with the service manager runs as it was described, and a test's registration is its own

**Guaranteed.** `nc_platform::service` writes a program's definition in the format its
system's manager reads — a systemd user unit, a launch agent's property list, a scheduled
task's XML in UTF-16 — and asks the manager's own tool, named by an absolute path:
`systemctl` from the absolute entries of `PATH` alone, `/bin/launchctl`, and `schtasks.exe`
under `%SystemRoot%`, so a relative entry of `PATH` never chooses what runs. Every value a
definition carries is escaped for the format that reads it — systemd's `%`, `$`, `\` and
`"` in each quoted argument of `ExecStart`, and its `%` in the program, XML's five
characters in a plist and in a task, and a task's arguments first quoted as Windows splits
a command line — and what no escape carries is refused before anything is written: a
control character anywhere, a program that is not an absolute path, a name outside
letters, digits, `.`, `_` and `-` or not opening with a letter or a digit, so no name is
read by a tool as one of its options, a `%` in what a task runs, which Task Scheduler
expands and has no form for, and a `$`, `\`, `"` or `'` in a unit's program — systemd
undoes no `$$` there, and refuses a program's path holding any of the other three, as
systemd 260's `systemd-analyze verify` measured. A `SystemRoot` that is not an
absolute path is refused rather than read from the current directory. A definition file is
created new and private, never through a link and never over one already there. **A
registration never replaces a service**, whoever registered it: a name the manager already
holds is refused before anything is written, `schtasks` is never handed the `/F` that would
replace a task, and a registration the manager refuses takes back only what it made — its
file, and on systemd the unit its failed start left loaded — never the service of that name
someone else holds; one the manager took is registered whatever becomes of the copy of a
task's XML that Task Scheduler no longer reads, which `unregister` removes when it cannot
be removed then. A task stores no credential: it runs with the user's own interactive
token, at least privilege. What each manager is told to restore is what its system's hosted
runner measured (`NOCTUA.md §17.3`). **Started at login by where it is, never by a
reload**: systemd is told with `enable --no-reload` — a link among the units
`default.target` wants, read as the user's manager starts, or, for a program the person's
graphical session starts (`Wanted::WithTheGraphicalSession`, systemd's alone and refused
by the other two managers before anything is written), among the units
`graphical-session.target` wants, a `Type=oneshot` run ordered after that target and part
of it, never restored, its failure taken as a success — and a unit in the runtime
directory is linked there alone (`--runtime`); `unregister` takes the links away first.
**A daemon's unit stops its own process alone** (`KillMode=process`): a stop, and a
restart after a crash, end the program's process and leave what it started detached — the
desktop interface a daemon starts — running, where systemd's default would end every
process of the unit's group; the generator's text says so, and no scratch service
measures it (below).
launchd starts at login the agents in the folder `Manager::loaded_at_login` names, and
Task Scheduler a task by its logon trigger. **One reload lets an enabled unit go, and only
a real install's**: once the user's manager has read a unit's link, at a login or a boot,
`default.target` wants the unit and keeps it loaded after its links and its file are gone
— measured with systemd 260, a scratch unit a started scratch unit wants — so `unregister`
asks `daemon-reload` once of a unit kept outside the runtime directory that a running unit
still wants, and of nothing else; one kept in the runtime directory, a test's, is never
reloaded away, and its unregistration says what still holds it (`Error::StillWanted`).
**Lingering is asked for by one caller**,
`cargo xtask dev --install`, for a real install: logind is asked whether it keeps the
user's manager, and asked to only where it does not.

**A test's registration never meets a real one.** The service manager's own suites —
`with_the_manager`, in the module's tests — register a scratch program, the test binary
started again, under a name that opens with `noctua-dev-test-`, the checkout's digest and
`-`, and carries the process and a nonce: a shape no install's name takes. The restorer's
suite, `apps/daemon/tests/the_stop_mark_silences_every_restorer.rs`, registers this
checkout's development daemon under the names the layout gives an install of it relocated
under a namespace of `r`, the process in base 36, `n` and a nonce —
`noctua-dev-<checkout>-r<pid>n<nonce>.<uid>.service`, the agent
`io.github.noctua_world.noctua_dev_<checkout>_r<pid>n<nonce>.daemon.<uid>`, the task
`\noctua-dev-<checkout>-r<pid>n<nonce>.<SID>` — which a real install of the checkout, never
relocated, does not take, and whose opening a relocated install a person makes of it may
share. On systemd every test's unit is written only in `$XDG_RUNTIME_DIR/systemd/user`,
which the user manager forgets at the next boot, and the restorer's suite enables its own
there alone; under launchd a test's plist stays in its scratch and is bootstrapped from
there; on Windows a test's task sits in the root folder, and the restorer's suite's carries
the logon trigger every restorer does. A guard unregisters each however the test leaves —
the restorer's suite's directly when its uninstall did not get that far — and each test
first sweeps this checkout's leftovers whose owner is dead — told by a lock the owner holds
for as long as it lives, in a scratch every run of the checkout keeps in `nc-registered`
under the checkout's own `target`, whatever its environment names as temporary, never by a
pid that may since name another process, and only once the scratch is a minute old, so a
scratch whose owner has not yet taken its lock is never read as dead — so a run killed half
way is cleaned by the next run of the same checkout, and a live run, of this checkout or
another, is never touched. The sweep unregisters only a name under its suite's prefix, held
twice: by the listing, which launchd and Task Scheduler answer with every service they hold,
and again before each name is unregistered; **and the restorer's suite only a name whose
lock's directory a run of it made**, before anything was registered under the name, and
kept until its restorer is unregistered, so a person's relocated install, which has none,
is never touched. So `cargo xtask check` on Linux registers short-lived units with the
developer's own user manager, under those names, and needs one running. The tests never
enable lingering, never reload the manager, and never stop, start, enable or mask a unit
they did not register.

**Enforced by** the module's tests in `crates/nc-platform/src/service.rs` — each
generator's text written out whole, each escape and each refusal by its words, each
manager's counters and listing read from recorded answers, the commands each manager is
asked, the tool found only in an absolute entry of `PATH` or under an absolute
`SystemRoot`, a listing narrowed to its prefix, a name the manager holds refused, a
registration the manager took standing whatever becomes of its copy, a task's counters
read by place in another display language, the scratches in the checkout's own `target`,
and the sweep's judgement of a scratch whose owner lives, one dead, one never locked, one
too new and another checkout's, a unit linked to start at login and unlinked without a
reload, a unit still wanted once its file is gone read from systemd's own answer and
reloaded away only where a login reads it, lingering asked for only where logind does not
keep it and its refusal said in its words, and how each manager says an instance ended,
launchd's last exit code an end only once its state is `not running` and never beside a
start it has counted and not yet run — by
the three scratch services in `with_the_manager`, run on
Linux, macOS and Windows in CI: one killed and restored as a new instance, read through the
manager's own counter and the instance's record of itself, having been handed its
arguments character for character through the format's escaping, and registered over in
vain, by the registrar and, under launchd and Task Scheduler, by the manager's own load,
with its instance still the one running, and on Windows killed past a repetition refused
while it ran and read as down until the next, a status Windows writes in English agreeing
with the last result at every wait; one that exits 0 left down, or started again by Task
Scheduler's repetition, then started when asked within twenty seconds, which no
repetition meets; one that a started scratch unit wants, on systemd, unregistered as still
wanted — stopped, its file gone — and forgotten once that unit goes; and each gone from the
manager once unregistered, its definition with it — and by the operators over
`nc-platform`'s sources in `xtask/mutants.py`, whose commands skip the three that
register.

**Not answered:** a process running as the same user, which can rewrite or remove any
definition, register a program of its own under any name — the test prefix included — or
stop the manager's restorer, and can race a registration between the name being asked and
the definition being loaded; whoever the system lets past every access list; that a
process a daemon's unit started detached outlives the unit's stop under
`KillMode=process`, which the generator writes and no scratch service of
`with_the_manager` measures, nor a unit the graphical session starts, which no runner's
manager reaches; registering a
task, on Windows, as a user who is not an administrator, which no runner measures, the
hosted one being elevated; a test's unit the user's manager came to want while it ran —
its links read again by a reload or a re-execution something else asked for, as a NixOS
switch asks — which its unregistration leaves stopped, its file gone, until the manager
next starts; a task's counters on a Windows whose display language is not English, which
no runner has; and a test's task left where nothing runs this checkout's tests again (both
under **What is deliberately not claimed**). Which program is registered for a channel,
and how an explicit stop silences every restorer (`NOCTUA.md §17.5`), are the daemon's and
its installer's (the next answer).

### The CLI sends nothing until both ends are proven, and prints nothing a terminal acts on

`apps/cli` is what a person and a program reach a channel's daemon through, and it
connects through `nc_ipc::client`, whose header has the checks in order (the answer
above): `apps/cli/src/client.rs` names that client for the rest of the crate and holds
what is the CLI's own, the exit code a refusal ends a run with (`exit_of`).

**Guaranteed:**

- **Its install is the one beside it.** `noctua` reads the manifest beside its
  canonicalized executable through `nc_channel::found`: the directory and the file judged
  this user's alone, the text read only as `nc_channel::Install::parse` reads it, and each
  refusal with its `CHAN` code and exit 12 — with no channel built in to fall back to.
- **Nothing is sent before both ends are proven.** The endpoint the layout names is
  reached through `nc_platform::ipc::connect`, which reads the user and the process at the
  other end off the system and closes a connection that is not this user's before a byte
  crosses it; nothing listening is a daemon that is not running (exit 9). The token is
  read only once the runtime directory and the token file are judged this user's alone
  (`NC-CLI-0002`), and only as the 64 lower-case hexadecimal digits a daemon writes
  (`NC-CLI-0003`); it is never printed or sent, and the text it was read from is
  overwritten. The handshake proves the daemon holds it before the client sends anything
  past its own proof, and the welcome the daemon sends beside that proof — which binds the
  two nonces, not the welcome — must name the process the system shows at the other end
  and this install's channel (`NC-CLI-0004`). Each of these is exit 10.
- **What reaches the kernel was read and held first.** A command line is read against the
  schema the daemon's registry lists for its command — a flag is a property's name, a
  value is read as its type, a list's flag is repeated once for each item, a scalar's
  given twice is refused, and `--` takes one JSON object — and the object is held to that
  schema by the call the kernel makes (`NC-VAL-0002`, exit 2). A command line the CLI
  cannot read (`NC-CLI-0001`), a command the registry does not hold (`NC-KERN-0001`, exit
  5) and arguments the schema refuses reach no kernel; an answer of another kind than the
  request's is refused (`NC-CLI-0005`, exit 8), and so is one that is not a frame the
  protocol reads — not one message, or past the bound — from a daemon the handshake proved
  (exit 8).
- **It confirms nothing.** The CLI acts as the profile's person, so a command that cannot
  be undone waits, while the desktop the daemon started is attached, for the person's
  answer there — confirmed it runs, declined it ends in exit 15 (`NC-KERN-0030`), with no
  answer in time in 14 (`NC-KERN-0029`) — and with none attached the kernel refuses it with
  `NC-KERN-0006`, which ends the run in an exit code of its own (4): what it needs is a
  person in the Noctua window (`NOCTUA.md §11.1`). **Nor does it switch MCP on**: the set
  asking for on, and an undo or a redo that would put it back on, wait the same way under
  codes of their own — `NC-KERN-0032` in 4, `NC-KERN-0033` in 14 and `NC-KERN-0034` in 15
  (`NOCTUA.md §12.15`); switching it off never waits.
  `noctua mcp` acts as its session's agent and is refused the same, but that refusal ends
  no session: the call is answered as a tool error, or as an unknown tool when the
  session's last list did not hold it — as no list holds a command that cannot be undone
  — and the session goes on.
- **What the daemon says is escaped.** A command's name, display name, line, paragraph,
  tags and categories — a command of the catalog's as much as a plugin's, since the daemon
  sends both — a refusal's sentence, a result, a schema's annotations, names and options,
  and every word a person typed that a refusal repeats are printed with each character
  `nc_schema::unseen` answers for written as `\u{…}`, so nothing printed moves the
  cursor, clears the screen, starts a
  forged line or turns the rest of one around; and `nc_schema` already refuses those
  characters in a schema's titles, descriptions, names and options once their escapes are
  decoded.
- **Help on a registry of the catalog asks no daemon, and is escaped as the daemon's
  texts are.** `help categories` prints every set and every category the binary's own
  catalog holds, and `help external-identifiers` every external identifier with each
  value it is identified by on each system — the registries themselves, which no daemon
  holds another of — in the environment's language, each text and each value written as
  a daemon's would be: a path, a bundle identifier or a package name is a text the
  compiler judged for its shape alone, and the escape is what keeps one from moving a
  terminal.
- **Help on the keybindings asks no daemon either, and escapes what a file gave it.**
  `help keybindings` prints this channel's keys as `nc_channel::keys::effective` reads
  them — the install's own configuration over the catalog's bindings — with each refusal
  and warning by its code and the catalog's message in the environment's language, and
  its detail and the file's path written as a daemon's texts are; it exits 0, because the
  defaults it shows beside a refusal are the keys in force.
- **`checkouts` asks no daemon, and escapes what the disk gave it.** It lists the
  development checkouts installed beside this one — the listing the Leader settlement
  reads, `nc_channel::checkouts` — each with its directory, its CLI and when it was
  installed written as a daemon's texts are, in one JSON document with `--json` or with no
  terminal, and in a picker at one, whose terminal is left as it was found however it
  ends (`apps/cli/src/terminal.rs`); leaving the picker changes nothing and ends in exit
  13. What a choice changes is the next entry's.

**Enforced by** the unit tests of each module in `apps/cli/src/` — the client's and a
run's against a daemon in-process, in a scratch install, that answers as a script says:
nothing listening, a handshake proving nobody, a token another account can reach and one
in a directory it can reach, each left as it was, a token missing and each way one is
malformed, a welcome naming another process and another channel, each answer of a
dispatch to its row of the exit table and an answer of another kind, help on the exit
codes, on the categories, on the external identifiers and on the keybindings with no
daemon — a configuration refused for each of its parts shown with its codes and the
defaults in force — every text a set
is headed with and a category is listed with escaped, every text an external identifier is
headed with and every value it is identified by escaped, help on everything showing the
grammar and the verbs with none, the checkouts listed and one chosen by its digest with
none, the picker's Esc and Ctrl-C ending the verb in exit 13 with `noctua-dev` where it
was and its Enter choosing the checkout it selects,
and a command line, a command and arguments refused with nothing dispatched — by
`apps/cli/tests/arguments_from_schemas.rs`, by `apps/daemon/tests/exit_codes.rs` — one
test for each way a run ends against a real daemon, `reference.irreversible.do` refused with
`NC-KERN-0006` in exit 4 and in both locales among them — by
`apps/daemon/tests/every_command_from_the_cli.rs`, where help lists every command the
daemon's registry holds by its name, display name and line in the environment's language
and each argument from its schema, by the
tests of `crates/nc-channel/src/found.rs`, by
`a_character_a_reader_sees_nothing_of_is_refused_once_its_escape_is_decoded` in
`crates/nc-schema/src/lib.rs`, and by the operators over those sources in
`xtask/mutants.py`.

**Not answered:** a process running as the same user, which can read the token and answer
as the daemon, relay a connection and rewrite the welcome sent beside the daemon's proof,
or ask the daemon anything the CLI can — authorization re-decides every dispatch (`§14`);
a command's welcome held to the system's word and not to the PID record,
which process control reads, and the `--json` document printed raw to a terminal (both
under **What is deliberately not claimed**).

### The CLI starts and stops its own channel's daemon, and signals nothing it did not hold to its record

`apps/cli` (`control` and `install`, whose headers have the steps in order) is how a
channel's daemon is started, stopped and asked how it is, and how a development checkout
is installed.

**Guaranteed:**

- **A stop is the mark first.** `stop` writes the `stopped` mark in the profile's
  persistent state directory, created private, synced and the directory synced, before it
  asks the daemon anything; a mark or a state directory another account can reach is
  refused and left as it was (`NC-DMN-0001`), and nothing is asked. `start` removes the
  mark the same way, once judged, and only after its wait: once a daemon answers or none
  holds the profile.
- **The daemon is asked before it is signalled, and only a process held to its record is
  signalled** (invariant 25). The PID record is read through `nc_channel::record`, judged
  this user's alone and read only as the daemon writes it, and the process it names held
  to it by `nc_platform::process::Verified` before the daemon is asked — through a pidfd
  opened first on Linux, asked again which process it holds once everything is read; one
  handle on Windows, every part read through it; `ps` and the boot's session on macOS,
  read again before each signal. A daemon that answers it is stopping is waited for
  `stop_within_ms` through what verified it, or, with nothing verified, until its record no
  longer names it, its endpoint answers no more — knocked at only once the record has gone,
  since a draining daemon accepts every connection and logs each that asks nothing as
  `ipc.refused` — and no daemon holds the profile's instance lock, which a daemon lets go
  after its runtime's last work (`nc_platform::try_hold`, let go at once); and never
  signalled: one still there then is a stop that did not finish (`NC-CLI-0008`, exit 11).
  One that does not answer is asked to end and then killed, `kill_within_ms` each, through
  the verification alone — once its record, read again, still names it: a daemon removes
  its record at the end of its own stop and lets its endpoint go only after, so one whose
  record went since it was read is ending a stop asked before, and is waited for as one
  that answered. A record missing or naming a process that runs no more is a daemon stopped
  (exit 0) while nothing holds its endpoint — on Windows, a process the CLI may open
  (**What is deliberately not claimed**); something that holds it and does not answer,
  with no record to hold it to a process, is never signalled (`NC-CLI-0009`, exit 11); and
  a record naming another process — another executable, another user — or one that does not
  read is never signalled (`NC-CLI-0009`, exit 11). **A process gone is judged before
  another** (`nc_platform::process::held_to`), so a record of another start names a process
  that runs no more whoever holds its id now: on Windows a process this user may query and
  may not hold — one above this process's integrity, or another account's where this user
  may query it, as the hosted runner's elevated account, which holds the debug privilege,
  may query any — is read through a handle that may only query it, never held, so a record
  a reboot left stale whose id such a process now holds is a daemon stopped, not a refusal.
  **A process this user may not even read is not its daemon**
  (`nc_platform::process::unread`), which runs as this user — a refusal to read it, never a
  refusal to hand this process a handle on it, which on Linux a filter can make of any
  pidfd and which is a failure to ask (`nc_platform::process::no_handle`): its record is
  read as one whose process runs no more, never signalled, and the next start writes its
  own — on Windows, for an account that is not an elevated administrator, that is every
  process of another account, as the hosted runner measured under a filtered token. A
  daemon read so wrongly still holds its profile's instance lock, which refuses a second.
- **A start runs the install's own daemon.** It is the executable the layout names beside
  this CLI — never one found on `PATH` — started through the system's service manager
  where that manager holds the install's restorer, as `--restored`, and otherwise detached
  (`nc_platform::process::spawn_detached`): standard input and output null, standard
  error a file in the profile's judged log directory written fresh and private, none of
  this process's handles, a process group of its own on Unix and no console on Windows.
  The start waits `start_within_ms` for that daemon to answer the handshake and to run; one
  that ends first or does not answer is a start that failed (`NC-CLI-0007`, exit 11), shown
  with the last lines of its standard error, escaped, for a detached start, and with what
  the manager counts of it for a start through the manager, whose daemon's standard error
  goes nowhere. **Before it starts one, either way, the start waits until a daemon answers
  or none holds the profile's instance lock**, `stop_within_ms` at most and the `stopped`
  mark still there: a daemon still ending holds the lock past its record and its endpoint,
  and one started beside it would be refused by the lock, or not started at all by a
  manager whose unit still runs it. One that holds it still by then is not signalled, and
  nothing is started (`NC-DMN-0008`, exit 11). The lock is taken for a moment and let go at
  once, while the mark is there where the command has one — a stop writes it first, and a
  start removes it only after the wait — so a restorer's start meeting that moment has
  ended at the mark; with no mark, it reads the moment as a daemon already up and ends, and
  the start goes on to start one itself. A daemon found answering that it is stopping is
  waited for as a stop waits for one, and then started; one found answering otherwise runs
  already.
- **The development install is copied, never linked, and named once.** `nc_cli::install`
  makes a missing base directory one private level at a time, and writes the binaries out
  of the target directory cargo built them in — the paths `cargo xtask dev --install` reads
  off the build's own messages, never derived from the checkout — under the layout's names
  — each into a private file beside its name,
  synced and renamed over it, runnable by its owner alone — then the manifest the same
  way, and reads the install back through `nc_channel::found` as its binaries will; the
  daemon's own `--initialize` draws the device and copies the world; each plugin is
  written into a private directory of its own. A daemon an earlier install runs is
  stopped first, through the same stop, and an earlier install whose manifest this build
  cannot read — of another format, or one another account can reach — refuses the install
  before anything is stopped or replaced: only a manifest missing is no earlier install.
  The environment — the base directories and
  `NC_INSTALL_NAMESPACE` — is read by the installer alone, so a relocated install is a
  test's, with no central log and no stack. `PATH` is not changed.
- **The mark has one reader** (`nc_channel::stopped::on_purpose`): the daemon's restored
  start, the installer and the interface's alert line read it the same way — the state
  directory and the mark judged this user's alone, one another account can reach refused
  and left as it was, and no mark where either is not there. **The interface's alert line
  restores only as a restorer does** (`nc_ipc::alert`): a daemon lost with no mark is said
  not running, offers restore now and counts down `[daemon] interface_restore_countdown_ms`
  to one deadline, in milliseconds since the Unix epoch, from the moment the line first
  says so, and at the deadline — once in an absence, never after a person acted on the
  line — starts the install's own daemon `--restored`; with the mark it says the daemon is
  stopped, offers start and counts nothing down, a mark read while counting cancelling
  the countdown and a mark that does not read taken for a stop. Either offer runs the
  install's own CLI, `start`, which removes the mark. What the line reads of the mark says
  what it says; what keeps a stopped daemon down is the daemon's own restored start, so a
  mark written after the interface last read it holds the daemon down all the same.
- **The development daemon has a restorer, and every restorer honours the mark.**
  `cargo xtask dev --install` registers the copied daemon with the system's service
  manager (`nc_platform::service`) under the name the layout gives it there, its one
  argument `--restored` (`nc_cli::install::Registered`): a restorer starts the daemon only
  through that entry, which ends at once, and well, while the `stopped` mark is there, so
  no manager has to know of a stop. The plan is `nc_cli::install::development_plan`'s,
  the one place a development install asks for its restorer. **The manager is found, and
  asked whether it holds the restorer an earlier install registered, before an earlier
  daemon is stopped**, so an install that cannot reach it stops nothing and writes no
  mark. That restorer is unregistered before any binary is replaced, and the mark is read
  before the install stops an earlier daemon, whose stop writes one. Its definition is
  kept where the manager reads it at each login — systemd's user-unit folder under the
  configuration base the manifest holds, enabled there without the reload `enable` asks
  for by default; the Library's `LaunchAgents`; Task Scheduler's own copy, started at
  logon by its trigger — and a relocated install's, a test's, in systemd's runtime
  directory, enabled there alone (`--runtime`), or in its own profile: where no login
  reads it on systemd and launchd, while on Windows its task starts at each logon as a
  real one does (`nc_cli::install::kept_in`). The install then starts the daemon through
  the manager, **unless the profile carried the `stopped` mark before the install**: a
  person's stop, which an install keeps, or the mark an install or an uninstall that did not
  finish left after its own stop, which it cannot tell from one — an uninstall that finishes
  removes the mark its own stop wrote, and keeps one that was there before it. A `start` with the restorer
  registered goes through the manager too, so the daemon it starts is one the manager
  restores; one the manager does not hold, or a manager that does not answer, is a
  detached start. **An uninstall reaches the manager before anything else**, as an
  install does: from an environment that names no tool for it — no absolute entry of
  `PATH` holding `systemctl`, no absolute `SystemRoot` — or of a manager that cannot say
  whether it holds the restorer, it stops nothing and removes nothing, since that manager
  may hold the restorer; it then stops the daemon and unregisters its restorer — its
  links and its definition with it, and the one reload a unit the user's manager has read
  needs to go. **On systemd the announcer is registered beside the restorer**, before
  the daemon is started: `nc_cli::install`'s unit the person's graphical session starts,
  under the layout's `announcer_unit`, which runs the install's own CLI as
  `leader --announce` once each time the session comes up — and once as it is enabled —
  so a daemon lingering started before any session learns the session's environment and
  starts its desktop interface in it (`NOCTUA.md §11.1`); one that cannot be registered or
  enabled takes the restorer back with it, and an uninstall, or the install made again,
  lets an earlier announcer go first. launchd and Task Scheduler start their programs
  inside the person's session already, and register none.
  Lingering, which starts a user's manager at boot, is enabled only by
  `cargo xtask dev --install` for a real install, where logind lets this user — logind is
  never asked for a relocated one (`lingering_asked` in `xtask/src/dev.rs`) — and is
  otherwise printed as `loginctl enable-linger <name>` and NixOS's
  `users.users.<name>.linger`; no test asks for it.

**Enforced by** `apps/daemon/tests/the_stop_mark_silences_every_restorer.rs`
(acceptance 8) — the interface's countdown, asked of `nc_ipc::alert` by the test standing
in for the desktop, restoring a daemon killed at the deadline as a new instance and
saying a stopped one stopped, counting nothing down and leaving it down when its restorer
runs anyway; and this checkout's development install, relocated, registered with the
system's own manager and started through it, then killed and restored as a new instance
read through the manager's counters and the daemon's record, stopped and started by the
manager as a restorer starts it and left down on the mark, installed again over the
daemon that runs — stopped, its restorer let go and registered again, and started
through it — and over the stop, which it keeps, started again through the manager,
marked and killed and restored once to end well and stay down, and unregistered by its
uninstall, on Linux, macOS and Windows in CI — by the unit tests of the installer's and
the start's decisions (the name each manager knows a restorer by, where a real install's
and a test's definitions are kept, a person's stop kept, the development plan's
restorer, a reinstall's order recorded by a stand-in restorer — its manager asked before
the install's own stop, the earlier one let go while the earlier binaries are there, the
mark read before that stop — a reinstall over an install it cannot read stopping and
replacing nothing, an install and an uninstall that cannot reach their manager,
or whose manager cannot say whether it holds the restorer, stopping nothing, a start
through the manager only where it holds the restorer), of `nc_platform::service`'s links,
the reload that lets an enabled unit go, lingering and counters, and of what
`dev --install` says of lingering and that it asks logind for a real install alone, by
`apps/daemon/tests/a_recycled_pid_is_never_signalled.rs` (acceptance 4),
by `apps/daemon/tests/channels_side_by_side.rs`, where production, staging and two
development checkouts are installed through the installer from one scratch environment,
started, held apart on disk, each stopped while the other three answer, and one restarted
— its record another start's, the process it named gone, the mark removed and the other
three untouched (acceptance 3), by `process_control_that_does_not_finish_is_exit_11` and
`a_start_whose_daemon_ends_before_it_answers_is_exit_11_with_what_it_wrote` in
`apps/daemon/tests/exit_codes.rs`, by the unit tests of `apps/cli/src/control.rs` — the
decision that signals only a verified process, and only while its record still names
it, the mark written, kept, removed and
refused, a stop nothing answers ending well over a record of a process gone and refusing
one of another executable or one that does not read, a stop that ends a child held to its
record, a daemon that said it is stopping left running and unsignalled once the wait is
over, and one nothing verified waited for until its record, its endpoint and its profile
let it go, its endpoint knocked at only once its record has gone, one whose record went
while a stop asked it waited for through what verified it and never
signalled, a stop something answers otherwise refused with no record or a record of a process gone, an
endpoint no socket can have taken for nothing there, a stop and a status the daemon
answers, a status with nothing answering, a start with no daemon beside it and one with a
daemon running, a start that says its daemon runs only once its status says so, one that
waits for a daemon that says it is stopping before it starts one, one that waits until no
daemon holds the profile and removes the mark only then, one that gives up on a profile
still held and starts nothing, one that gives up at
once on a daemon that ended and shows the last lines it wrote, a daemon that
does not answer asked to end first and killed only when it has not ended, and a restart
that stops before it starts and starts nothing after a stop refused — of
`apps/cli/src/install.rs`, whose copies of this test binary run, whose uninstall leaves
every file of an install whose daemon did not stop, and whose uninstall removes the mark
its own stop wrote and keeps a person's, of
`crates/nc-channel/src/record.rs`, of `crates/nc-channel/src/stopped.rs` — no mark, a
mark, and a mark or a state directory another account can reach refused and left as it
was — and of `crates/nc-ipc/src/alert.rs` — a lost daemon counted down to one deadline
and restored at it and not before, once, a stop counting nothing down and cancelling a
countdown, one gone counting afresh, a person's act never raced by the countdown, a mark
that does not read taken for a stop, and each act's program and argument, held to the
daemon's and the CLI's readers by their own unit tests — by the
`Verified` and `spawn_detached` tests of `crates/nc-platform/src/process.rs` — a child
held to its record and refused for each part of one that is not its own, asked to end and
killed through what held it, a record of another start gone and one of the process as it
is another, never held, for a process this user may read and may not hold — process 1 on
Unix, and on Windows a child whose access list lets this user query it alone, read on a
thread holding no privilege — a process this user may not read gone for its record, a
handle on a process the system refused a failure to ask and never that, a detached child
leading a group of its own, and the decision for every system's reading —
and by the operators over those sources in `xtask/mutants.py`.

**Not answered:** a process running as the same user, which can stop the daemon, remove
the mark, rewrite the record or replace an install's binaries; on macOS the moment between
the last reading of a process and its signal; on Windows a terminate as hard as a kill; a
daemon that does not answer and whose record was lost, which no stop can end; a daemon
that said it is stopping and does not end, which no stop signals; on Windows, for an
account that is not an elevated administrator, a record a reboot left whose id another
account's process now holds, which is read as a daemon stopped without that process being
read; a start that gave up, which leaves the daemon it started running; commands run at
once, each failing closed and none signalling; with nothing verifying the daemon, the
moment between its lock let go and its process's end, in which a start through a manager
whose unit still runs it starts nothing (`NC-CLI-0007`); a daemon the interface's
countdown restored, which runs outside its manager's unit; what a daemon a
manager started writes to its standard error, which goes nowhere; an uninstall from an
environment that does not reach the manager, which removes nothing; the mark an install
or an uninstall that did not finish left after its own stop, which the next install reads
as a person's stop; and lingering, which is the machine's (each under **What is
deliberately not claimed**).

### A query is matched in bounded work, and an item's text is read up to its bound

**Guaranteed.** Matching a query against a list costs work bounded by the query and the
fields, whatever either holds. A query holds at most `nc_fuzzy::QUERY_CHARS_MAX`
characters, separators included, and is refused past that by `Query::parse` before
anything is matched, counting no further than the bound — and so holds at most
`nc_fuzzy::WORDS_MAX` words. Each field is read to `nc_fuzzy::FIELD_CHARS_MAX` characters
and no further, and a field cut there is never matched whole. A word laid scattered costs
one table of at most the product of the two bounds, which the `Matcher` keeps and reuses.
The matcher is a pure function of what it is handed: no I/O, no regular expression, no
`unsafe`, no system `cfg`, and no panic on any character. Its score is packed into widths
derived in `const` from the same bounds and kept below `2^53`, so a JSON client reads it
exactly and no count of word starts lifts a match past a higher tier. The scattered
alignment's table weighs one word start past every count of adjacent pairs and one pair past
every earliness, each weight derived from the bound it must clear.

**Enforced by** the bounds in `crates/nc-fuzzy/src/lib.rs` and the `const` assertions in
`crates/nc-fuzzy/src/score.rs`; the unit tests beside them —
`a_query_longer_than_its_bound_is_refused`, `a_field_is_read_to_its_bound`,
`a_field_cut_at_its_bound_is_never_matched_whole`,
`the_widest_score_a_query_can_reach_is_a_safe_json_integer`, and, in
`crates/nc-fuzzy/src/align.rs`, `an_adjacent_pair_outweighs_any_earliness` and
`a_word_start_outweighs_any_count_of_pairs`, over fields of real length; and
`crates/nc-fuzzy/tests/properties.rs`, whose laws run over arbitrary characters and
fields past the bound (`no_text_and_no_query_makes_the_matcher_panic`) and whose last law
holds the alignments of fields up to ten characters to an exhaustive search.

**Not answered:** how many items a caller hands it — a list of a million costs a million
matches, and a surface that matches a registry bounds the list and its answer itself, as
`kernel.find` bounds its answer and not the registry —
and which characters look alike, and a text's graphemes, which **What is deliberately not
claimed** says the core does not read.

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

### An entry shows everything it holds, within its bounds, judged once for every reader

**Guaranteed.** Every text a command's entry shows — its display name, line description,
paragraph and tags, in each locale — holds no character `nc_schema::unseen` answers for and
no placeholder, and is bounded in characters: a display name 64, a line one terminal line of
80, a paragraph 1,024, a tag 40, at most 32 tags — and a locale's texts together are bounded
in bytes by `[kernel] entry_bytes`, each tag counted with the quotation marks and the comma it
is listed in, the bound a plugin's are held to. A tag holds no brace
at all, and no comma,
which a list of tags reads as two; a paragraph never says what its line says, no two of
the catalog's commands share a display name in a locale, and no plugin command uses one a
catalog command shows, nor one another command of its plugin shows in that locale. A glyph is a code point the Symbols-Only font
draws, written one way: `U+` and upper-case hex, four digits at least and no leading zero
past them. A set of categories and a category carry a display name and a line description
alone, held to the same rules, and no two categories of one set share a display name in a
locale; a category is named `<set>.<slug>` under the set it is in, a standard's category
carries its standard's name in printable ASCII with no space and, in a set its standard
ranks, its role — the platform's own carries neither — and a command names only categories
the registry holds, each once and at most four. An external identifier carries a command's
four texts under the same rules, names categories, a glyph and a wiki text as a command does, and no two
external identifiers share a display name in a locale; its name is lower-case words joined
by single hyphens, it is identified on at least one system with no system's table empty,
and every value it is identified by — a command's name, a desktop file, a Flatpak or Snap
name, a bundle identifier, an executable, an `App Paths` key, a path — has its field's one
closed shape, holds nothing a reader sees nothing of and no `..`, is at most 260
characters, sits once in its list, and as a path begins at `/` on Linux and macOS and at
`%ProgramFiles%\`, `%ProgramFiles(x86)%\` or `%LOCALAPPDATA%\` on Windows, never at a
drive or a directory one machine has. A name exempt from the untranslated rule is
one the locales share, or the exemption is refused. Every wiki text sits in a closed tree — under a locale and a registry this
catalog has, named for an entry of that registry, in the format its lower-case extension names, in every
locale and in one format across them — is held to 64 KiB with nothing a reader cannot see
but its line breaks, and every path of the tree is read as what it is, never through a link:
a link, or a file where a directory belongs, is refused as misplaced, so the walk reads
nothing outside `catalog/wiki/`; and a locale's directory is told by its name byte for byte,
never by a lookup a file system folds, so one named for a locale in another case is
misplaced on every system. A text missing from a locale names its entry and the text
it is. Anything else fails the build, and nothing is generated while a problem stands.

**Enforced by** `crates/nc-schema/src/entry.rs` — `judge`, `glyph` and `judge_wiki`, a unit
test per rule and a property in `crates/nc-schema/tests/properties.rs` — the one judge,
which `crates/nc-catalog-compiler/src/metadata.rs` hands the texts of every command, every
set of categories, every category and every external identifier and every wiki file, and
whose refusals `crates/nc-catalog-compiler/tests/the_gates_fire.rs` makes fail one at a
time, `every_text_an_external_identifier_carries_is_judged_naming_it` breaking each of an
identifier's four texts alone; `crates/nc-catalog-compiler/src/registries.rs` holds a set's,
a category's and an external identifier's shape, and
`a_category_named_outside_its_set_or_in_no_set_fails_the_build`, its neighbours and
`an_exemption_over_a_name_the_locales_no_longer_share_fails_the_build` break each rule alone,
`every_identification_field_refuses_what_it_cannot_be` holds each field's shape both ways,
`every_bound_a_refusal_states_is_the_bound_its_check_holds` each bound to the fix that
states it, and `every_identification_field_is_held_to_its_shape_naming_the_identifier_and_the_field`
holds every field of every system to it in the build;
`a_wiki_text_names_an_entry_of_the_registry_it_sits_under` plants a command's text under
the identifiers' directory and an identifier's under the commands'. What the catalog's message rules already report — a key absent, blank or
untranslated, a placeholder — is dropped from the judge's answer by name rather than said
twice, so `declared_params` in `crates/nc-catalog-compiler/src/load.rs` giving each of the
three sentences the empty set is what refuses a placeholder in one, and
`a_command_message_may_not_interpolate_anything` and
`an_external_identifiers_texts_may_not_interpolate_anything` break each sentence alone; `judge_tags`
refuses a tag holding a brace, half a placeholder included. A link is planted where the
tree, a locale's directory, a registry's and a text belong, by every door
`nc_platform::testing` makes one with, in
`a_link_where_a_wiki_path_belongs_fails_the_build`. The page escapes every cell and links a
wiki text rather than inlining it, writes a value an entry is identified by as a code span
whose fence no backtick in it ends and whose cell no bar in it ends, and the generated crate
writes a glyph as its escape.

**Not answered:** whether a text is true, or a translation faithful beyond not being the
source sentence again; whether a category's standard name and role are its standard's
today, beyond the version its set records; whether a value an external identifier is
identified by is what a system holds today, or the thing it names — identification is data,
which nothing in this build runs, expands or resolves, and a name, a path or a bundle
identifier can be copied by anything installed beside the real one, so a reader that one
day matches one against a system holds the match as a hint, never as proof (**What is
deliberately not claimed**); whether a glyph depicts what its comment names; whether a wiki
text's Markdown or Typst is well formed, since nothing in this build parses or prints one;
and two display names or tags that differ only by look-alike characters, which needs the
confusable folding `BLANK_RANGES` says this crate does not carry.

### A page says what the code does, and nothing a value it publishes writes

**Guaranteed.** Every page under `docs/` is what `nc_catalog_compiler::compile` emits, so a
page edited by hand, a page nothing emits any more and a page two generations write
differently each fail the gate. A page about what the catalog cannot see — the CLI's
reference and the keybindings — publishes the tables the CLI and the channels hold, handed
to the compiler as a `Surfaces` that carries no command, so every command a page shows,
with every text and argument of it, is the catalog being compiled and never the one the
tool that compiles it was built with; a surface with no verb, no exit code or no channel's
Leader is refused rather than published as a page that says nothing. What help says of a
command and what `docs/cli.md` says are one layout's, which escapes every text it is
handed, so a page can show nothing help would not; and an MCP tool on `docs/mcp-tools.md` is
the command's own name with each dot an underscore, its line and paragraph, its argument
schema as written, once it compiles inside the subset the kernel holds a dispatch to, and
`readOnlyHint` exactly when the command is not `mutating` — a display name and tags are a
person's words, and no tool carries them. Every cell goes through `cell`, so no value ends
its cell or its row. A person's words — a locale's sentence or tag, on every page that
shows one, a bound command's display name on the keybindings page included — have each
backtick escaped as well, so a text holding a bar, a backtick, a backslash or `<` shows as
written; a description a registry writes for a developer, an error's or a log event's, may
set a name as code with Markdown's code span, by design, and shows as one. A standard name
and an identification value, the values set as code that may hold any printable character, go
through `code`, whose fence is longer than any run of backticks in it and whose text no
Markdown escape reaches; the values the limits page sets as code are not held to a grammar
by the compiler — a table's key and its `order_by` are held by xtask's limits gate to a
table and a column the migrations declare. Every fenced block is fenced longer than any
run of backticks in it, and a tool is JSON written by `serde_json`, so a text there holding
a backtick, a quotation mark or a line break shows as that text.

**Enforced by** `a_hand_edit_to_any_generated_page_is_drift` in `xtask/src/check.rs`, which
edits every page the compiler emits, one at a time, and holds the emitted pages to the
committed ones in both directions;
`the_pages_read_the_catalog_they_are_handed_and_not_the_one_xtask_links` in
`xtask/src/build.rs`, over a copy of the catalog that declares a command and renames
another; `a_surface_with_no_verb_no_exit_code_or_no_leader_fails` and
`the_cli_page_lays_each_command_out_with_the_layout_it_was_handed` in
`crates/nc-catalog-compiler/tests/the_gates_fire.rs`; the unit tests of
`crates/nc-schema/src/entry.rs`, which hold a tool to its four fields and refuse a schema
outside the subset; `a_fenced_block_is_closed_by_no_run_of_backticks_inside_it` in
`crates/nc-catalog-compiler/src/docs.rs`, with
`a_value_written_as_code_is_shown_as_its_system_writes_it` beside it, and
`a_run_of_backticks_in_a_schema_ends_no_block_on_any_page`,
`a_standard_name_reaches_its_page_as_its_standard_writes_it` and
`a_backtick_in_a_persons_words_reaches_the_page_as_written` beside the compiler's gates,
over the commands page, the keybindings page, the MCP page and the categories page; and
`xtask/tests/the_pages_reach_the_catalog.rs`, which holds each page to the CLI's tables,
`nc_channel`'s Leaders, `nc_schema::keys`' vocabulary and system shortcuts, and help
itself. The files a surface is built from are read by the determinism gate as the emitter
is (`EMITS`).

**Not answered:** whether a page is read — the in-app pages that render them arrive in
Stage 16; a store's name and a rate limit's surface, which the limits page sets as code
and nothing holds free of a backtick, so one holding a backtick would end its code span
early; and the four things **What is deliberately not claimed** lists under the
generated pages: that `docs/mcp-tools.md` is any session's tool list, that `docs/cli.md`
names any channel's CLI but production's, that a function a surface hands the compiler
reads nothing of its own, and that `docs/keybindings.md` shows anybody's keys but the
defaults.

### A keybinding is one of three forms, written one way, and names one key and one command

**Guaranteed.** Every keybinding the catalog declares is one of `NOCTUA.md §12.8`'s three
forms — `Leader <key>` in the leader menu, `Leader+<key>` with the Leader held and
`Leader+Chord+<key>` with the chord modifier held as well — its key one of a closed
vocabulary of physical keys named as `KeyboardEvent.code` names them, the modifiers, the
lock keys, the numpad, the media keys and `Escape` left out. It is written the one way the
grammar writes it: a text read as a binding is the text that binding writes back, so lower
case, an alias, an Emacs chord, a bare key, a modifier written inside a binding, two keys, a
doubled space and a character a reader cannot see are each refused, and no two spellings
are one binding. One key of one table runs one command, and the same key in another table is
another binding. A binding carries no argument: it resolves to a command's name, which
dispatch holds as it holds any, so a binding reaches nothing the command list could not.
A Leader holds a modifier, so no bare key is taken for the whole computer; each channel's
default Leader is a `const` built through the grammar, so a default it refused is a build
that fails, and the three defaults keep every rule of `§12.8` with each channel's chord
modifier, Shift on every channel. A chord a system keeps for itself is answered with a
warning naming the system, never a refusal, and every chord warned of is a row copied with
the source that says so. A refusal quotes the text it read, and the part of it where a key
or a modifier belongs, each cut at 64 characters and escaped as every diagnostic of
`nc-schema` escapes a name. The judge reads and writes nothing: what
it settles depends on the parts it is handed alone, and a part is judged against the parts
settled before it and the defaults after it, so no channel is refused for what one after it
configures and two parts both on their defaults are never judged against each other.

**Enforced by** `crates/nc-schema/src/keys.rs` — `Binding::parse` and `Chord::parse`, the
one reader `check_keybindings` in `crates/nc-catalog-compiler/src/load.rs` hands every
binding the catalog declares to, and `intra`, `apart`, `settle`, `tables` and `warnings`,
each with its unit tests — and the laws in `crates/nc-schema/tests/properties.rs`: no text
but a written binding or chord is read as one, everything the grammar writes is read back,
no settled Leader meets another channel's, a channel is never refused for one ranked after
it, settling does not depend on the order claims are listed in, and a refusal returns to
the default — each over draws `the_texts_read_against_the_grammar_are_bindings_often_and_not_often`
and `the_claims_settled_are_accepted_often_and_refused_often` hold to reaching every
outcome. In the compiler's suite `a_keybinding_in_any_grammar_but_the_leaders_fails_the_build`
breaks the grammar each way an author might, `two_commands_bound_to_one_key_of_one_table_fail_the_build`
and `one_key_in_two_tables_is_two_bindings` hold the duplicate to the binding read, and
`A_TYPO_REFUSED` refuses `keybinding`, singular, by name;
`crates/nc-channel/tests/the_names_are_disjoint.rs` holds the defaults to the rules
(`the_defaults_of_the_three_channels_break_no_rule`), the chord modifier to Shift and each
Leader to the spellings `§12.8` writes.

**Not answered:** a person's choice of keys — where it is read from, whose files are read
and how privately — which the next entry answers; whether `SYSTEM_SHORTCUTS` holds every chord
a system keeps (**What is deliberately not claimed**); and whether a Leader takes a chord a
program relies on, which no table here lists.

### A person's keybindings are read only when private, refused part by part, and bind nothing but command names

**Guaranteed.** A person's choice of keys is `<config>/keybindings.json` in their
channel's configuration directory (`Layout.keybindings`), read by `nc_channel::keys` alone —
`effective` for the keys in force, `judged` for a choice weighed, and `proposed` and
`written` for the choice a shortcut setting makes and writes (*A shortcut setting writes the
channel's keys through the judge that reads them*) — and shown by `noctua help keybindings`,
which asks no daemon. The file and its
directory are judged this user's alone by `nc_platform::judge` before a byte is read, a
link refused however it points (`NC-CHAN-0009`), and each is left as it was; a file is
read through `Read::take` no further than one byte past `BYTES_MAX`, one longer than
`BYTES_MAX` is refused (`NC-CHAN-0011`), and a disk that does not answer is
`NC-CHAN-0010`. A choice is held to the same bound when `Configuration::parse` reads it,
before `judged` weighs it, and its canonical form is never longer than its text, so a
choice judged in force is a file its reader reads. The text is read only as `nc-canon`
reads JSON, a key written twice refused, into a closed shape: `format` required and this
build's, `leader` a chord and `chord` a modifier in the Leader's grammar, and `bindings`
each command's name — one a command can have — beside bindings in the grammar; anything
else, a later format among it, refuses the file whole and every default stands
(`NC-CHAN-0011`). The Leader part and the
bindings part are then judged apart by `nc_schema::keys`, the rule the catalog compiler
reads every binding by: a Leader holding no modifier, holding its own chord modifier, or
meeting another channel's Leader refuses the Leader part alone (`NC-CHAN-0012` to
`NC-CHAN-0015`), and a key of one table bound twice refuses the bindings part alone
(`NC-CHAN-0016`). A part refused keeps its defaults — the channel's Leader with Shift, the
catalog's bindings — which are apart across channels by construction, and the other part
stands. Each piece of the file a refusal quotes as it was written — a key, a word, a chord
or a binding the grammar cannot read — is cut at 64 characters, each character that ends a
line or a field written as its code point; a command's name is repeated whole only once it
is one a command can have, at most `NAME_MAX` characters of lower-case letters, digits, `-`
and `.`; and the CLI escapes every text it prints. A
chord a system keeps, and a held key that is the Leader's own, are warned of and kept
(`NC-CHAN-0018`, `NC-CHAN-0019`), over the keys in force: a refused part's choice raises
nothing, and the default that stands in its place is warned of instead.

**Other channels are read, never written.** A configured Leader is judged against the
channels installed under this install's own bases and namespace: production and staging
by the names `nc_channel::layout` gives them, and each development checkout by a profile
directory under the state base whose name is that checkout's leaf in this namespace and no
other, so a test's install never reads a real one's — each only while its install
manifest is there. They are settled in one order, production, staging, then the checkouts
by digest, a configured Leader against the Leaders settled before it and the defaults
after it, so production reads no other channel's file and no channel reads one after it.
A Leader holding no modifier or its own chord modifier is refused for that before any
other channel is listed or read, so what another channel's file says never changes that
refusal. A file before it that its own reader would refuse is that channel's defaults; one this
build cannot judge — of a later format, or one the disk did not answer for — a manifest
the disk did not answer for, a state base that will not list or holds more than
`PROFILES_MAX` development profiles, and more than `CHANNELS_MAX` installed checkouts
refuse the configured Leader rather than guess (`NC-CHAN-0017`); a profile an uninstall
left is listed and binds nothing, so it counts against the listing's bound alone. Nothing of another channel is written, locked or
removed: its profile directories are listed, its manifest asked whether it is there, and
its file judged and read.

**A binding is data.** It names one key of a closed vocabulary in one table and one
command's name, and carries no argument: what it runs is a dispatch the decision point
holds as it holds any, so a forged file reaches nothing the command list could not.

**Enforced by** `crates/nc-channel/tests/a_configuration_that_collides_is_refused.rs`,
every install in it a scratch's relocated under a namespace — no file is the defaults; the
warnings are those of the keys in force, each with its own code; a configuration overrides the catalog's bindings in its own channel alone; two checkouts
share the development Leader until one is configured, and the collision is refused;
production keeps its Leader when a checkout configures the same; each rule a Leader breaks
is refused with its own code, the rule that needs no other channel even beside a file this
build cannot judge or past the checkouts' bound; a chord modifier configured alone and
refused keeps both defaults, the refusal naming the part it judged, and no locale's
sentence says a Leader was chosen; a channel is judged against the Leaders before it and the
defaults after it, and a file after it is never read; an uninstalled channel binds
nothing; a file and a directory another account can reach, and a link to either, are
refused and left as they were; every way a file is malformed is refused whole, naming what
it could not read; a choice past the file's bound is refused before it is written, and one
a writer judges is the file its reader reads; what a refusal quotes of the file is escaped
and cut, and a command's name is repeated whole once it is well-formed; a refused bindings part
keeps the configured Leader, and the reverse; a file before it that its reader refuses is
that channel's defaults, while one of a later format, more installed checkouts than are
read, more profiles than a listing reads and a base that will not list refuse the
configured Leader, and as many profiles an uninstall left as the checkouts' bound and one
more refuse nothing; another namespace's installs are
never read; and a read leaves every file and directory under the bases as it was — by the
unit tests of `crates/nc-channel/src/keys.rs`, which hold a profile directory's name to
the leaf `layout` writes in both directions and a configuration to the file's bound, and
hand each answer a disk can give — for a manifest, for another channel's file and for the
channel's own — to the decision that reads it, a disk that does not answer refused with
its own code (no test makes a disk fail; the decision is what is held), by
`help_on_the_keybindings_needs_no_daemon` and
`help_on_the_keybindings_says_why_a_configuration_was_refused_and_shows_the_defaults` in
`apps/cli/src/run.rs`, and by the operators over `keys.rs` and the CLI's help in
`xtask/mutants.py`.

**Not answered:** a process running as this user, which can write the file as a person's
settings would; channels installed under other base directories, which this reader cannot
find; and what lies between a read here and a key bound by the interface, which reads the
file again (Stage 16). What the reading leaves narrowed is under **What is deliberately
not claimed**.

### The Leader is one chord asked of the system, and a chord another program holds is refused

**Guaranteed.** How the Leader reaches a process is `nc_platform::leader`'s to say and to
ask for, and no caller names a system: `delivery` answers a hotkey the system registers —
X11, Windows, macOS — or a compositor's binding, on Wayland, and `register` asks for the
chord where there is a registration. **What is asked for is one chord and never a bare
key**: a registration takes a `nc_schema::keys::Leader`, which cannot be built without a
modifier, and reads nothing else of the keyboard — no event is selected on any window, no
hook or event tap is set, and no permission of the person's is asked for. A session that
cannot be identified is refused (`Refused::NoSession`) and nothing is asked; a Wayland
session is refused as having no registration (`Refused::ByBinding`), whatever X display it
also names, since Xwayland's grabs are not global; and a chord another program holds is
refused as `Refused::Taken` — X11's `BadAccess`, Windows' `ERROR_HOTKEY_ALREADY_REGISTERED`,
Carbon's `eventHotKeyExistsErr` — never shared and never taken from it. The key is a place on
the board, asked of each system by that place from one table the compiler holds to the
vocabulary. What is reported is that the chord went down, once for each time it does, and
never that it went up; dropping the registration gives the chord back; and a registration
that stops delivering while it is held says so, once, through `on_lost` — on X11, the one
system where one can, when the server goes away or the chord is lost to another client.

On X11 the client is this crate's own, in safe Rust: it dials the display `DISPLAY` names
on this machine's sockets — `:N` or `unix:N`, or a socket's path read as X's own libraries
read one, where a name that begins with `/`, or with `unix:` and then `/`, is the path whole, a `:` in it
included, the first display of the server there, and a last `.<screen>` is read off it only
where no file is at the whole of it and one is at the rest — and refuses a name with a host
in it, a path that is not absolute, one that is set and is not
a text, and a socket whose file is neither this user's nor root's — asked before a byte is
sent, and never gone round by the abstract socket, which is tried only where no file is at
the path and never for a display named by its socket's path, so a file nothing listens at,
which a server that was killed leaves, is a display with no server; sends the
`MIT-MAGIC-COOKIE-1` of this machine and this display, from the file `XAUTHORITY` names or
`.Xauthority` under `HOME` — a variable set to nothing naming no file, so never one in the
directory the process is in — read up to a bound and believed only when it ends where an entry
does; bounds every read and write of a registration to five seconds, and lifts the bound
once the chord is granted, since the wait for a press has none; reads every length the server sends under
a bound, before a byte of that length is read or allocated; takes for the answer to a
request the reply that carries the request's own number, read past the events a server
sends every client unasked and past any other reply (`reply_to`); reads which modifier Alt, Super
and Num Lock are from the server's own map, by physical key, once, as the chord is grabbed,
and refuses a map that cannot tell the chord's modifiers apart — Alt or Super in no modifier
of its own for a chord that holds it, the two in one modifier for a chord that holds
either, and neither asked of a chord that holds neither; grabs the one key with exactly the chord's modifiers on
every root window, bare and with each lock — Num Lock being one only where the map keeps it out
of Alt's modifier and out of Super's, held by the chord or not, since the chord with that
modifier added would be another chord; and at a press hands back the keyboard the grab
handed over and lets the grabs go, asking the server only whether the Leader's own key is
still down until it is not — one bit read of each answer — and then makes them again, so a
held key is one press and the keys pressed with it reach the window that has the focus.
Where the server answers that another client was granted the chord meanwhile, whatever part
of it was granted to this one is let go and the chord is asked for again, every quarter of a
second and twenty times in all, after which the connection is closed and the caller is told
the chord is lost.

**Enforced by** the unit tests of `crates/nc-platform/src/leader.rs` — the delivery from
each system and environment, every key's code in each system's own list written out in that
list's order, the modifiers' flags, each system's refusal, a panic in the caller's function
caught, no two keys sharing a code in any system's list, and `Held` refused as `Send` by a
`compile_fail` doctest beside one that holds the same path to compiling, since a
`compile_fail` block passes for any error — and of
`crates/nc-platform/src/x11.rs`: every request against its bytes, the display and the
authorization file read and cut short at every length, a display named by a path read whole
and its screen read off only where no file is at the whole of it
(`a_screen_is_read_off_a_path_only_where_no_file_is_at_the_whole_of_it`), the file the
environment names (`the_authorization_file_is_the_one_the_environment_names`) read
from a directory of the test's own and up to its bound
(`the_cookie_is_read_from_the_file_the_environment_names_up_to_the_bound`), a message longer
than any asked for refused unread, the answer to a request read past an event and another
request's reply (`the_answer_to_a_request_is_the_reply_that_carries_its_number`), the masks
read from the map, and the connection driven
end to end against a server written beside the tests at the other end of a connection made
in memory, which keeps every byte it was sent and runs on every system
(`a_chord_is_grabbed_pressed_once_and_let_go`, `a_key_the_server_repeats_while_held_is_one_press`,
`a_chord_another_client_holds_is_refused_as_taken`,
`a_chord_taken_while_the_key_was_down_is_asked_for_again`,
`a_chord_another_client_keeps_is_lost_and_its_loss_is_told`,
`a_server_gone_with_the_chord_held_is_a_loss_told`, and
`what_a_server_sends_unasked_is_read_past_to_the_answer_asked_for`, where the server sends
`MappingNotify` and another request's reply before its map and a reply that says every key
is up before each answer about the keys); against sockets of the test's own,
where a socket is a file — one that never answers holding a registration for its bound
alone, a bound lifted from a socket no longer ending its reads
(`a_bound_lifted_from_a_socket_no_longer_ends_its_reads`), a display named by a path
reached at that path whole, a `:` in it included, and at what stands before a screen only
where no file is at the whole
(`a_display_named_by_a_path_is_reached_at_that_path_whole`), and not
reached by an abstract socket of that name, a socket file nothing listens at refused with
something listening at the abstract name of its path, which answers once the file is gone
(`a_socket_file_nothing_listens_at_is_not_gone_round_by_an_abstract_name`), a socket
dialled as another account refused, and a `DISPLAY` that is not a text refused as
unreadable; the figures written here — five seconds, a quarter of a second, twenty times —
by `the_usual_pace_is_the_one_written_down`; by
`this_system_grants_the_chord_hears_it_and_gives_it_back` in `leader.rs`, which every
system runs against what it has — on Linux a real `Xvfb`, started with no socket in
`/tmp` and spoken to only where no file is at its display's path there, since a display is
dialled at its file first and a file there is another server's, where what each press is handed to panics once it has said so and every press
after the first is heard all the same, the panic caught by the registration; the chord is
heard once, held past the server's repeat and with Caps Lock
on; a chord short of a modifier, with one more or on another key is not heard, pressed
once the grabs are made again and followed by a press that is; a key pressed with the
Leader held reaches a window that has the focus; a second registration is refused as taken
while the key is up and granted while it is down, after which the first has the chord back
when the second lets it go and is told of its loss when the second keeps it; a dropped
registration is given back; a registration left idle past the bound on its answers still
hears the chord and is told of no loss; a server killed is a loss told, where what the loss
is handed to panics once it has said so and what the panic carries is let go on the thread
that told it with the registration still held, the panic caught by the registration; and a
server started to admit by cookie alone refuses a
registration that names no file and grants one whose `XAUTHORITY` names the server's; on
Windows a registration, the message Windows sends for it
posted to its thread, a second registration refused and the first given back; on macOS a
registration made and given back through Carbon, twice, the press standing aside there by
`nc_platform::system::A_TEST_PRESSES_A_HOTKEY`, which the gate names (*A suite that needs
what a system lacks asks `nc_platform::system`, and the gate names it*); by the operators
over both sources
in `xtask/mutants.py`; and by invariant 17's gates over the two files that write `unsafe`.

**Not answered:** a process running as this user, which on every system can register the
chord first, and on X11 and Windows send a synthetic press or read the keyboard itself;
on X11, another account answering at the display's abstract socket name while no server
holds it and no file is at the socket's path in `/tmp`, which is sent this display's cookie
if the file still holds one — an abstract name has no owner to ask after — and **the chord
while the Leader's key is down**: the grabs are let go from the press until the key is up,
which is for as long as the Leader is held, because kept they fire again for each repeat
and take the keyboard from the window each time, so a program that asks for the chord then
is granted it and not refused, and keeps it if it does not let it go within the five
seconds this client asks again — `Refused::Taken` is the answer only while the holder's key
is up, and "held" means until `on_lost` says otherwise; **a modifier map changed after the
registration**: the masks are read once, and the `MappingNotify` that tells of a change is
dropped with every event that is not the Leader's press, so where a later `xmodmap` or
layout moves Alt, Super or Num Lock to another modifier the grabs stay on the old map's
bits and no loss is told, until the registration is dropped and made again; a display
named by a path that is not absolute, which X's libraries read after `unix:` and this
refuses; an X server that does not
number its keys as Linux's do; on macOS, whether the system refuses a chord another
application registered, which was not measured, and a press itself and the refusal of a
chord held, which a test may not send or ask there: that part stands aside on macOS, named
by the gate beside its verdict, and the Linux and Windows jobs ask it; and on Windows, a
layout switched after the registration, which leaves the
chord at the virtual key the old one had. Whether the Leader was tapped or held is the
window's to decide (`NOCTUA.md §12.8`), and who pressed it is nobody's to know.

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
  **A commit's words are written as text**, since an install carries the changelog as a
  page the one reader of Markdown reads (`crates/nc-pages`): a code span the words close is
  kept, every other character that reader takes as markup is escaped, and where the words
  begin an entry, so is any mark that would open a block there; and a changelog that reader
  refuses all the same refuses the release before anything is written.
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

- **A process running as the same user that tampers with the desktop interface confirms
  what it likes** (decided for Stage 16): reading its secret out of its memory, driving it
  with synthetic input, replacing its executable or its units, or announcing a display of
  its own. What is held is that nothing that only speaks the protocol confirms a call (*A
  call that cannot be undone runs once the person confirms it in the desktop the daemon
  started*). Not taken: sealing the desktop against its own user, which a desktop
  operating system does not offer between two processes of one user (`NOCTUA.md §2.4`).

- **Wayland's GlobalShortcuts portal is not built**: the Leader reaches a Wayland session
  by a key binding of the compositor's that runs `noctua leader`, and by nothing this tree
  registers (`NOCTUA.md §12.8`). Not taken: a client of D-Bus, which the portal is spoken
  over — a dependency for one call (`NOCTUA.md §2.3`).
- **The daemon asks for the Leader on Windows alone, and the Windows row was read, not
  run**: the daemon's interactive session and `RegisterHotKey`'s thread are read from the
  system's documentation and from how `cargo xtask dev --install` registers the daemon, and
  no test registers the real chord from a daemon, since a test's install asks for nothing.
  A daemon started over SSH may be granted a chord that is never delivered, and a Leader
  configured while the daemon runs by anything but a shortcut setting — the file edited by
  hand — is asked for at its next start. Not taken: a daemon on
  Linux or macOS that asks, which would hold a grab past the X server it was made on, or
  need the main thread's run loop the daemon does not turn.
- **A note is decided, not journaled**: each fact is decided as its command would be by
  the probe that journals nothing, so a refused note leaves no entry, and the interface's
  frame-rate changes are in no audit row. Not taken: a journal row for each — one fsynced
  row in a table nothing prunes, at the rate a window draws.
- **`cargo xtask dev --install` writes a compositor's line in niri's syntax alone**, the one
  compositor whose own documentation it was written from, and names a key by the keysym a US
  board types there; elsewhere it says the chord and the command to bind, and it edits no
  configuration of the person's.

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

- **Every command of the interface is one journal row for good**, in a table no pruner
  takes from: what moves at the rate an interface draws — a scroll, the draft, a hover —
  is meant to reach the daemon as a note, which is no command, and the commands are the
  same changes asked one at a time, for the CLI, MCP and a plugin. The state's revision
  is one kernel's start's, and a plugin that runs as its person may drive that person's
  interface from inside its command, as the decision point allows the person. Not taken:
  a bound of the interface's own on the journal, and a refusal of these commands below the
  top of a call tree (`crates/nc-kernel/src/shell.rs`).

- **The kernel decides a person as themselves and an agent through its session, through
  test doors for the rest.** A caller is decided unmarked with an empty actor chain, and an
  agent only through the session `Kernel::session` opened, whose chain no caller writes; a
  chain longer than one link comes only from `Session::through_for_tests`. An approval
  comes only from `approval_for_tests`, so a shipped kernel denies every command a quorum
  must approve until the approval system exists (Stage 23); a confirmation from
  `confirmation_for_tests`, which keeps working beside the real one, or from the person's
  answer to a call the kernel held (`Kernel::settle`), which the daemon gives the desktop it
  started alone. A confirmation is bound to the whole principal, kind and
  identifier, and to its command by the plugin that held it, so one that took an unloaded
  plugin's namespace redeems none issued before. A person's history is read by identifier
  alone, so the directory is what keeps two principals of different kinds from sharing
  one.
- **Plugins can time one another**: they share one process, one machine and one bus, one
  top-level call tree at a time, and nothing pads, batches or randomises a wait — the
  reason the kernel's crate header names timing between plugins as unanswered.
- **Recovery finds what was left open, and not what is broken.** A kernel opening asks the
  journal's store which units of its realm are open (`nc_journal::Store::open_units`): the
  columns that bracket a unit, never a read of the history, so a row nothing can decode
  hides no unit opened after it. Its cost still grows with the history, and not only with
  what was left open: four columns of each entry visited are compared and none is decoded
  — the local journal and PostgreSQL decode nothing but the unit each open one names, from
  its text — yet the local journal's index `(realm, work_unit)` visits every entry of the
  realm inside a unit, whatever the data, since invariant 18 leaves the planner's
  statistics ungathered there; and PostgreSQL, which indexes the realm and the unit apart
  (`journal_entries_realm`, `journal_entries_work_unit`) and the two together nowhere,
  reads the opening brackets and their closes each through those indexes or over the whole
  table, by the cost its planner estimates, so a recovery there may visit every entry of
  every realm. It does not look for a broken row, and logs nothing about a history it
  never reads: `journal.verify` still stops at it and reports it (`journal.broken`), and
  nothing runs that verification on a schedule. An opening bracket it cannot name keeps
  the kernel shut, as the decision point's **Not answered** says. Two kernels over one
  journal would each take the other's open units for a stopped process's; one daemon keeps
  one profile.
- **The schedule is the running daemon's, and nothing put off outlives the kernel.**
  Timers and events are kept on no disk. A daemon pumps its kernel every `pump_every_ms`
  — the timers due, the events queued, and `nc-prune` over the local database — and
  sweeps the blobs every `sweep_every_ms` (`apps/daemon/src/schedule.rs`, `NOCTUA.md
  §15.8`); `limits.prune` runs when a guardian holding a quorum's approval dispatches it.
  A kernel opened by anything but a daemon runs none of them on its own, and nothing runs
  `journal.verify` on a schedule. A table the pruner skips is escalated by the run of pumps
  that met it (`daemon.prune_stuck`), never by the table, which `Pumped` does not name.
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
- **The local database stops other accounts, not the account it belongs to.** SQLite has no
  roles: a process running as the same user can `DROP TRIGGER` and rewrite the images, or
  race the owner and mode checks between check and open, and the next open's refusal of a
  changed schema is detection, not prevention. Through a statement, undoing an agent or a
  grant takes such a change first, since the triggers refuse a replace before it deletes
  the row it meets; a trigger put back before the next open is not seen. Nor is what runs
  no statement: SQLite's incremental BLOB I/O, which fires no trigger and rewrites in place,
  at the same length, an agent's or a grant's text column no index holds — its realm, so
  its own realm no longer reads it, or an agent's operator — and the file's bytes written
  directly. An administrator, and a disk read offline
  without full-disk encryption, are not answered — the file is not encrypted. SQLite's
  `DEFENSIVE` flag and an authorizer are out of `sqlx`'s safe API's reach, and nothing here
  runs SQL this repository did not write — the premise the bundled SQLite's floor (3.50.2,
  CVE-2025-6965) and the later FTS5 advisories rest on. The bundled SQLite (3.51.3) is
  below 3.54.0, which fixes CVE-2026-119375: a database opened with a `-wal` whose page
  size differs from its own returns uninitialized page-cache memory. The pair that needs is
  written only by the account the process runs as, the one this bullet does not answer:
  every database the shipped code opens goes through `nc_local`, which holds the directory,
  the file and each companion to this user's alone before SQLite reads them, and the gate's
  own scratch is a directory `xtask/src/local.rs` makes private and refuses to find.
  `deny.toml` leaves RUSTSEC-2026-0459 out of the audit on that ground until the lock can
  reach `libsqlite3-sys` 0.39. A test suite's scratch directory is one it made itself, with
  the mode its umask gives, so a group that umask lets write there is not answered for the
  tests. The blobs' directory stops the same people and no more. An open leaves a `-shm` it
  gave a `-wal` that had none (an index holding no data) beside a file it then refuses, and
  leaves what it made before it judged the path it refuses: the database file, when it was
  missing and a companion is then refused, and on Windows the profile's lock file
  (`nc_local::LOCK_NAME`), made before the database file is judged. It waits without bound
  for another open of the profile, so a process stopped mid-open holds every other.
- **`chrono` is linked unmaintained**, through `cedar-policy-core`, for Cedar's `datetime`
  extension, which the packs use for a grant's validity and the request's `now`.
  RUSTSEC-2026-0543 says so and names no defect, and `deny.toml` leaves it out of the audit
  until `cedar-policy-core` moves off `chrono`: what `chrono` parses is what this tree's packs,
  its world and the kernel's clock give Cedar, and a defect found in it is an advisory of its
  own id, which the audit still fails on. Not taken: a datetime extension of this tree's own.
- **GTK 3's Rust bindings are linked on Linux, with an unsound iterator in them**, under the
  desktop interface's window and webview (`tao`, `wry`'s WebKitGTK), whose bindings stop at
  `glib` 0.18. RUSTSEC-2024-0429 is unsoundness in `glib`'s `VariantStrIter`, fixed in 0.20,
  and `deny.toml` leaves it out of the audit because the iterator is made by one function,
  `Variant::array_iter_str`, which no crate of this tree's Linux graph calls — so the code is
  linked and never run; and RUSTSEC-2024-0370 says `proc-macro-error`, which those bindings'
  macros run inside the compiler, is unmaintained, naming no defect. Both leave when `wry`
  moves off GTK 3's bindings. Not answered: a crate of that graph that begins to call the
  iterator, which no gate here sees — the search behind the claim is the one `deny.toml`
  describes, and it is made again when the lock moves.
- **The desktop's window, where no test of it can look.** What its webview paints where the
  document has not — the dark surface in a dark window, the engine's own white in a light
  one — is set and read by no test, since no engine says what it paints outside a document;
  what the probe reads is the page's background at its first frame and once settled. A
  window made smaller by the system's own gesture is held at its least by the system, from
  the hint the window is built with, which no automated test can drag: the probe asks
  through the interface's own door (`Opened::resize` in `apps/desktop/src/host.rs`), which
  fits the least to the page's scale itself, so a hint gone is not seen there. A preference
  for dark or light the system changes while the interface runs is taken at the next
  start, the document being stamped once. The typefaces the design system's sheet asks for
  are not served, since `noctua-design-dist` ships none, so the page draws in the system's
  fonts; and the footer's accessible name is the product's, `Noctua`, in no locale, until
  the interface's own texts exist in both — the drawer and the sidebar named by the word
  the catalog files every command of each under, its first tag (`drawer`, `sidebar`), and
  the drawer's controls rail by what the drawer shows, so no two of the three share one.
  The window is handed a home of its own by both its suites, which refuse anything written
  in it; the GL driver's shader cache, the driver's and no webview's, is turned off there,
  and what a system's webview writes outside the user's directories by its own name — on
  macOS a path the webview takes from the system rather than from `HOME` — is not looked at. The window is sized and placed as the state
  asks (`window.resize`, `window.move`), and a size or a place the system's own gesture
  gives it is not said back: the state keeps the last a command asked, so a window started
  again comes back at that size and place, not at the one a hand left it at.
- **The desktop's seats, past the one a system gives.** A seat other than the system's own
  exists only where a command names it, since no system's second pointer reaches the
  window yet (`NOCTUA.md §5.12`); `apps/desktop/src/seat.rs` writes it onto the page as
  the design system's attributes, and whether each is drawn exactly as the native
  pseudo-class it stands for is the design system's claim and its browser suite's — the
  real window's suite holds only that the attribute changes how its element is drawn,
  where the system's own seat is not in that state there. A second seat's selection on an
  option of a list has no twin there: a list marks the system's own keyboard with
  `data-nd-active`, which is no pseudo-class, so that seat's keyboard is drawn as its focus
  on the option, which the design system styles on the list around it. A touch screen's
  own tap, long press and drag, and a touchpad's two fingers, are made by no runner: a tap
  is the engine's click, which every control runs on, and the real window's suite holds
  every control drawn at least `--noctua-target-min` each way at the smallest window.
- **The desktop's footer, where the probe does not reach.** `--probe` exercises the footer
  in the system's own webview with events a script dispatches — typing, a control
  activated, a draw that keeps the focus, a draft written — which an engine marks
  untrusted; text composed by a real input method, a real press, and what a screen reader
  says of the prompt and of the info line's announcements are not run by any test here,
  and the block cursor's own behaviour is the design system's, held by its browser suite.
  A draft is told to the daemon as a note, and one the daemon refused, or one in flight
  when a connection is lost, is not told again until the person types once more; a draft
  set elsewhere while the person types is written over what they typed, the last writer
  winning. The channel is said by its word in the window's locale, the footer named by
  its own, the modeline's connection glyph by the connection's word — offline, the only
  state there is before a realm's server — for a screen reader, and another seat's info
  line row by the seat's word and its number (`catalog/interface.toml`); what a screen
  reader then says of them is not run by any test here, only the names the markup holds.
  The info line is a live region, so a hover the state holds is
  announced as a notice is. And Dioxus's interpreter, which `apps/desktop/src/assets.rs`
  serves, is loaded by no page: the footer is brought to new markup by the document's own
  script. The expanded info line's box that scrolls is a stop of the keyboard, which the
  real window's suite holds by the element's own tab index; that the engine then scrolls it
  by the arrows is the engine's, and no test presses a real key on it. A draw that takes
  away the element the keyboard is on, or leaves it unable to take it, puts it on its
  part's own element, and where the part is gone whole the page says nothing: it rests on
  the state having moved the keyboard already, which the kernel does for the drawer — the
  one part a draw empties whole — by sending every keyboard on a drawer left showing
  nothing to the prompt line. The info line's box collapsed under the keyboard is a case
  the suite cannot reach, since only a real key or a press puts the keyboard on that box:
  the suite holds the rule where a command leads to it, the leader menu's list become the
  command list, on the same branch of the page's script, and that the system's webview
  drops a keyboard left on an element that cannot take it a moment later, and keeps it on
  the chevron once the rule moves it there, was measured by hand under `Xvfb` with real
  keys.
- **The three settings of how the interface is shown, past the footer.** The kernel holds
  the language and the appearance, every window is answered them with the state, and the
  desktop reads both (`Settings` in `apps/desktop/src/mirror.rs`) — and draws by neither
  yet: the window speaks the locale it was started in, English, and is drawn in the mode
  the system preferred when it opened, until the step that draws the settings pages makes
  the window follow them. The footer's visibility is followed: `hidden` draws the design
  system's folding footer with the state's "shown now" on it, which under `hidden` is a
  reveal — a Leader tap or `footer.show` begins it, and the kernel ends it once no seat's
  keyboard is on the footer or the drawer (`footer_let_go` in
  `crates/nc-kernel/src/shell.rs`), the window reporting where the keyboard went as it
  does; the pointer at the window's bottom edge or over the footer, and a keyboard coming
  back to it, open it by the design system's own rules, which no test here presses, and
  begin no reveal. `footer.hide` under `always` changes the state's flag and nothing a
  person sees. A setting set is no step, so nothing undoes one; and the appearance is the
  mode alone — the palette stays the design system's default.
- **The interface's own texts, where nothing says them yet.** `catalog/interface.toml`
  holds the daemon alert line's sentences and offers, the confirmation surface's labels,
  the prompt's send control and the sentence for a text nothing handles, the words for
  the connection and a seat, and the names of each setting's choices, in both locales;
  on this branch the desktop says the footer's name, the channel's word, the connection's
  word, a seat's word and the line naming a compositor's binding, and the rest wait for
  the parts that draw them: the alert line's, the confirmation surface's and the undo
  notice's for the desktop's second chain, the prompt's send control's, the settings'
  choices for the settings pages, and the connection online for a realm's server.
  `every_interface_text_is_said_by_the_desktop_or_waits_for_its_part` in
  `xtask/tests/the_registry_reaches_the_code.rs` holds each text to a use in the desktop's
  own code or to a row of its list of texts whose part has not arrived, which fails for a
  text on it that a part says, so the list only shrinks; a text used only in a test, or
  named and never drawn, is what it cannot see.
- **Markdown, past the subset.** `nc_markdown` is held to its own corpus and to nothing
  else: no suite compares what another reader of Markdown — a forge, CommonMark's
  reference — makes of a text of the subset, so "a page reads the same wherever it is
  rendered" is the subset's intent, measured only by what it refuses. One difference is
  known: a bare `https://` address in a page is text here and a link where a reader
  autolinks, as `docs/keybindings.md`'s sources are. Emphasis follows CommonMark's
  flanking rules for `*` and `_`, and a character beside a run that is neither a letter,
  a digit, whitespace nor punctuation of the basic Latin set — punctuation to one version
  of CommonMark, a symbol to another, and no table of Unicode's categories in this crate —
  is read both ways, the run refused where the two readings differ. A source outside the subset in two
  places is told of the first the reader meets, which is not always the first in the
  source: every line's control characters and trailing space are read before any block,
  and a block's shape before its text. The reader bounds its depth and not
  a text's length, which is its caller's: a wiki text is held to 64 KiB by its judge, and
  a page an install carries by nothing but the build that embeds it. A link's target is
  read, never opened: what an `https` address leads to, and whether a page a path names
  exists, are its reader's to decide. And no part of the interface draws the tree yet:
  the help and dev pages that render `nc_pages::PAGES` arrive with the desktop's pages.
- **A generated page is English, with a column for each locale** (`NOCTUA.md §12.16`):
  its prose says what the code does in the language of every artifact here, and what a
  person is told — a command's names, an error's message, an interface text — sits in
  one column per locale, side by side, so the help and dev pages show the same page in
  either language. Not taken: generating each page once per locale.
- **A surface a plugin shows, as the desktop draws it.** `apps/desktop/src/surface.rs`
  reads a tree's shape strictly — its kinds, its keys, the type and the closed choices of
  each value, and the kernel's count of nodes and depth — and takes the kernel's
  judgement of the rest: a text's bytes and the characters it may hold, an input's name
  once in a tree, a bound name being an input's, and a command being registered. What
  it does not draw yet is drawn as the nearest it can: a column's or a row's `gap` as
  none, since the design system's Tailwind bridge maps no spacing token; a `danger` or a
  `success` text in the default tone, since `Paragraph` holds none of either; an image by
  its `alt`, since no blob reaches the webview; and an integer input without its bound,
  which `NumberInput` does not take. No surface is drawn in the window yet: asking the
  daemon which surfaces are open and what each shows, and a control activated as
  `surfaces.activate`, arrive with those commands; a control is found in the page by its
  surface and its node's pointer, which no closed set of the window reads, so a press of
  one runs nothing.
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
- **A journal entry's authorization fields are the kernel's to fill**: `actor_chain` is
  empty but in an agent's tree; an entry another writer makes carries what that writer
  gives it.
- **A proposed name reserves nothing.** Two proposals are two identifiers, an unused one
  is a gap, as a Snowflake's gaps already are, and nothing records which names were handed
  out; a proposal whose name does not fit, or whose entry the journal refuses, spends its
  identifier all the same.
- **A kept identifier is not vouched for.** `naming.propose` formats the `id` it is given
  and does not ask whether this machine minted it or any file carries it, so a caller can
  claim another file's identity, exactly as renaming by hand can; whether a file of that
  name exists, or would collide on disk, is the saving feature's question.
- **A plugin may mint as many identifiers as its tree's dispatch count allows**, a person's
  or an agent's: an agent's budget counts its session's calls, not what each mints.
- **Whether Denote removes an `=` inside a signature part** is open
  (`crates/nc-ids/src/naming.rs`), and the commands answer what the builder does today.

**Channels and installs**

- **The clipboard's tools are found on `PATH`**: `wl-copy`, `xclip` or `xsel`, `pbcopy`,
  `powershell.exe`, as each system's shell would find them, so a program of the same name earlier
  on a person's `PATH` is handed the path a choice copies. A path not copied is said, and
  undoes nothing.
- **A checkout installed before checkouts recorded themselves lists without its
  directory**: its digest and its CLI's path, dated by its manifest's last write, saying
  that installing it again records its directory; it can still be chosen. Its binaries,
  of an older build, know no `checkouts` verb and no launcher.
- **A launcher on Windows is copied once, when it is missing**: a running program cannot
  be replaced there, and what it does — read its pointer and run the CLI named — is every
  build's, so a choice rewrites the pointer alone.
- **The picker counts a character as one column**: a character a terminal draws two
  columns wide makes its line that much wider than the width it was cut to.
- **The picker matches a word of a directory within `nc_fuzzy::FIELD_CHARS_MAX`
  characters**: a longer directory is read again from each word the read before does not
  hold whole, so every word is reached, but one word longer than that is matched within
  its first that many alone.
- **`noctua-dev` is the development channel's alone**: putting `noctua` and
  `noctua-staging` on `PATH` is the installers' (Stage 19), and nothing edits a shell's
  start-up files, the registry or `PATH` itself.

- **A checkout is 32 bits of its path's digest**, kept because it already names every
  developer's compose volumes: two checkouts whose paths share a digest are one checkout
  to every name, and two whose digests fall on one pair of the port block
  (`nc_channel::PORT_PAIRS`) are derived the same stack ports, which the second
  `cargo xtask dev` refuses by name until one of them is moved.
- **The stack is the checkout's, not its user's**: two accounts working in one checkout
  directory share one compose project and its ports, on a Docker daemon and a loopback
  the machine shares, and none of the three carries the user.
- **Only the development channel has a restorer, and only the service manager restores
  it**: `cargo xtask dev --install` registers one; production and staging have no
  installer yet (Stage 19), and the tray and the interface's countdown, which restore a
  daemon too (`NOCTUA.md §17.5`), arrive with the interface (Stages 16 and 17): the
  countdown is decided in `nc_ipc::alert` and run by nothing until the desktop interface
  is built. A daemon that dies at each start is restored until its manager gives up:
  systemd after its start limit, five starts in ten seconds; launchd never, once a second;
  Task Scheduler never, once a minute. The interface's countdown restores once in an
  absence of its daemon and no more.
- **A daemon the interface's countdown restored runs outside its service manager's
  unit**: the desktop interface starts it as `noctua-daemon --restored` itself
  (`nc_ipc::alert::Run::restorer`), so where `cargo xtask dev --install` registered a
  restorer, the manager does not restore that daemon after a later crash — the
  interface's next countdown does — until a stop and a start put it back under the
  manager; a person's restore now, `noctua start`, goes through the manager. A clock moved
  back while the line counts makes the countdown longer by as much, because the deadline
  never moves.
- **A daemon its manager started writes its standard error nowhere**: each manager is told
  to hand it none (`nc_platform::service`), so a start through the manager that fails says
  what the manager counts and not what the daemon wrote, and a refusal of a daemon a
  restorer started is read in the central log or by starting it by hand.
- **Lingering is the machine's**: `cargo xtask dev --install` enables it where logind lets
  this user and prints the command and the NixOS option otherwise. A NixOS configuration
  that sets `users.users.<name>.linger = false;` takes it away again at each switch — and
  so does one that leaves it unset on a release whose option is a plain `bool`, false by
  default — after which the daemon starts at the first login and not at boot; where the
  option is `null` by default, a configuration that leaves it unset keeps lingering as
  `loginctl` left it.
- **Review alone keeps lingering from the tests**: `nc_platform::service::Registrar::linger`
  is public and no gate counts its callers, so a test that called it would enable lingering
  on the machine `check` runs on and still pass; its one caller is `cargo xtask dev --install`,
  through `lingering_asked` in `xtask/src/dev.rs`.
- **Review alone wires `dev --install`**: no test runs the verb, so one that asked logind
  without `lingering_asked`, or built its plan without `nc_cli::install::development_plan`,
  would pass `check`; the tests hold the two functions, not that the verb calls them.
- **No test runs the reload that lets a real install's unit go**: no test may reload the
  user's manager, so it rests on the one measurement with systemd 260 and on the unit tests
  of what is reloaded and when (`to_let_go`, `wanted_once_gone` in
  `crates/nc-platform/src/service.rs`); a systemd that kept the unit after that reload would
  fail `cargo xtask dev --uninstall` with the name still registered.
- **A start at login is read back on systemd alone**: the link `default.target` wants is
  read from the disk, while a launch agent's start at login rests on the folder
  `Manager::loaded_at_login` names and a task's on the logon trigger in its XML, each held by
  unit tests; no run logs in to see either start.
- **An install keeps a person's stop by the mark alone**: the mark is what tells a stop
  asked for from a daemon the install found stopped, so a profile whose mark someone
  removed by hand is started by the next install.
- **An install reads every mark as a person's stop**: the one an install's own stop wrote
  before that install failed — a binary not replaced, a restorer its manager would not let
  go or take, a profile the daemon would not initialize — and the one an uninstall's stop
  wrote before that uninstall failed are read by the next install as a stop asked for,
  which leaves the daemon down and says that `start` starts it. An uninstall that finishes
  removes the mark its own stop wrote and keeps one that was there before, so only a
  person's stop, or an install or an uninstall that failed after its own stop, leaves the
  mark then. An install that cannot reach its service manager, or whose manager cannot say
  whether it holds the earlier restorer, stops nothing and writes no mark.
- **An uninstall needs the service manager it registered with**: from an environment that
  names no tool for it — no absolute entry of `PATH` holding `systemctl`, or no absolute
  `SystemRoot` — `cargo xtask dev --uninstall` stops nothing and removes nothing, and says
  so, since that manager may hold a restorer that would start what was removed; it is run
  again from an environment a login made.
- **A stop ends only a daemon it can find**: a daemon that does not answer is signalled
  through the process its PID record is held to, so one that hangs after its record was
  removed, or replaced by hand with one naming another process or one that runs no more,
  is never signalled, and the stop says so (`NC-CLI-0009`, exit 11) for as long as
  anything holds its endpoint; and a record a crash left, naming a process that runs no
  more, stays until the next start writes its own.
- **A daemon that said it is stopping is never signalled**: `stop` waits `stop_within_ms`
  for it, and one still running then — draining a call that does not end, or hung after it
  answered — is a stop that did not finish (`NC-CLI-0008`, exit 11), left running with the
  `stopped` mark written; a stop asked again waits for it again — a stopping daemon answers
  at its endpoint until its record is removed, and a stop that finds the endpoint gone and
  the record it read gone with it waits for it too — and nothing in the CLI ends it harder.
- **On macOS a process is read, then signalled**: `ps` and the boot's session say what a
  process is, and are asked again just before each signal, but nothing holds the process
  between the two as a pidfd does on Linux and a handle on Windows, so an id handed out
  again in that moment — after the daemon ended, before the signal — would be sent it.
- **On Windows a terminate is a kill**: no signal there is one a process can answer, so a
  daemon that does not answer `stop` is ended by `TerminateProcess` at once, with what a
  crash leaves for its next start to read past; a graceful stop is always the one asked
  through the endpoint.
- **On Windows a stale record may name a process this user may not read**: a record a
  reboot left, whose id another account's process now holds, is told from the daemon by
  that process's start, read through a handle that may only query it, and an account that
  is not an elevated administrator may not even query another account's process. Such a
  process is not this user's daemon, so its record is read as one whose process runs no
  more — never signalled, and replaced by the next start — without its start ever being
  read: a refusal to read a process is taken for its being another account's, so a daemon
  of this user's that the system refused to show would be read as stopped too, while its
  instance lock still refuses a second daemon. Reading a process's start with no handle
  on it is `NtQuerySystemInformation`, in a windows-sys feature
  (`Wdk_System_SystemInformation`) outside the ones decided for this tree.
- **On Windows no end is read through a handle that may only query**: a daemon of this
  user's that the CLI may not open — one running at a higher integrity level than the CLI —
  is not read as ended while anything still holds its process, so a stop that meets it
  ended, with nothing at its endpoint, ends in `NC-CLI-0009` (exit 11) rather than as a
  daemon stopped. A start, which waits on the instance lock, is unaffected.
- **On Windows a stopping daemon's last connections keep its pipe busy**: once its record
  is removed the daemon accepts no more, and a connection it still serves holds the pipe's
  one instance, so a stop asked then waits two seconds for an instance and ends in
  `NC-CLI-0009` (exit 11), instead of finding the daemon stopped, for as long as that
  connection lasts.
- **Only the dead-code lint holds the Linux refusal of a pidfd as a failure**: the test of
  `no_handle`, in `crates/nc-platform/src/process.rs`, calls it directly, so a call site that
  stopped sending a refused `pidfd_open` through it would pass every test and every operator
  in `xtask/mutants.py`, and only clippy, which `check` runs with warnings denied, would see
  the function go unused.
- **A start that gave up leaves its daemon running**: one that does not answer within
  `start_within_ms` is reported as a start that failed and is not ended, since what it is
  doing is its own to say; `stop` ends it.
- **Commands run at once meet each other's moments, and each fails closed**: a stop asked
  beside another, once the daemon's record and endpoint are gone and its process not yet,
  says the daemon is stopped; a start beside another start, or beside a restorer's start
  that read the moment the start's wait holds the profile's lock as a daemon already up,
  can have the daemon it starts refused by the lock, or ask a manager to start a unit
  still running one about to end well, which starts nothing (`NC-CLI-0007`); none of them
  signals anything. And with nothing verifying the daemon — a PID record that does not
  verify, as where `pidfd_open` is refused — a stop's wait ends when the daemon lets its
  instance lock go, a moment before its process ends with its last line logged: a start
  through a manager in that moment starts nothing for the unit still running it, and fails
  the same way.
- **An uninstall keeps the profile**: `cargo xtask dev --uninstall` stops the daemon and
  removes the binaries and the manifest, and leaves the device, the database and the world
  — and the `stopped` mark where it was there before the uninstall's own stop, a person's —
  so an install made again is the same device; they are removed by hand.
- **A daemon's grant is the one its world file names, and nothing revokes it.** The world
  file names the realm, the person every dispatch is decided as, the grant that person holds
  and the realm's epoch, and a daemon reads it at each start (`nc_authz::LocalWorld`): the
  directory it opens over it holds that grant and that epoch until the file changes and the
  daemon starts again, and no grants table is read and no epoch moves until the directory
  over the grants table (Stage 22). A world naming no grant is a person who holds none, every
  dispatch of whom is denied (`NC-POL-0003`). The suites' world names `g.by_policy`, the
  membership the family pack confers, which has begun and has no end: the kernel's suites'
  `g.current` ends on 2027-01-01, and a daemon decides on the system's clock. An agent's
  grants are not the world file's: they are the profile's database's, given and ended by
  the person's `agents.grant` and `agents.revoke` and seen at the agent's next decision.
- **A daemon a suite started outlives a suite killed outright.** Each suite stops its
  daemons through the endpoint and kills them on the way out, and stops one the CLI's
  `start` spawned through the CLI's own `stop`, which reaches a daemon only once its PID
  record is written, so one hung before that outlives its test too. A test process killed
  with no destructor run leaves its daemon running, holding none of the runner's output,
  with its scratch install under the temporary directory, until it is stopped by hand.
- **Another account is kept off the endpoint by what no test can be**: no hosted runner
  has a second account and an unprivileged test cannot make one, so a connection from
  another user is never made. What stands in for it, on every system, is the endpoint read back
  as made (`0600`, or the pipe's one entry), each way another user can reach the runtime
  directory refused, the peer predicate refusing a forged user
  (`nc_platform::testing::someone_else`), and the process read held to a child's rather
  than this process's own — so a reader answering this process's credentials is caught,
  and one answering a third process's would be caught only where that process ran as
  another user.
- **A peer that pings and reads nothing is dropped late**: a write waits only once the
  system's buffer for the connection is full, and between requests the daemon writes
  nothing but a ping each `ipc_heartbeat_ms`, so such a peer holds its connection, and one
  of `ipc_connections_max`, until those pings fill the buffer — minutes on a Linux socket,
  hours on a Windows pipe — and only then does `NC-IPC-0017` end it. Only this user
  reaches the endpoint, so this is what a process running as that user can do, as it can
  speak slowly inside every bound.
- **A device's seed is wiped as far as safe Rust can say**: the key `ed25519-dalek` holds
  zeroizes itself, and the copies read from the file are overwritten and passed through
  `core::hint::black_box`, which no compiler promises to honour.
- **Two development checkouts share the development Leader** until one is configured, and
  a relocated install shares its channel's — the default is one chord per channel
  (`NOCTUA.md §12.8`), and the configuration `nc_channel::keys` reads refuses the
  collision; what writes one is the interface's (Stage 16).
- **A Linux machine without a running user manager cannot pass `check`**, as one without
  docker cannot: the service manager's suites register a scratch unit with it, and a
  container or a login that has none refuses them, in `systemctl`'s own words.
- **Windows restores whatever stopped**: Task Scheduler cannot tell a crash from a clean
  exit, so its restorer starts a program that exited 0 again within a minute, and what keeps
  a program down there is the program's own mark (`NOCTUA.md §17.3`).
- **launchd's domain is the graphical one**, `gui/<uid>`: a session with none, a login over
  SSH alone, has no domain to bootstrap an agent into, and registering fails there.
- **A test's task outlives a run killed half way, on Windows**, and keeps running: unlike a
  unit in the runtime directory, which the next boot forgets, or an agent bootstrapped from
  a scratch, which the next logout does, a task in the root folder survives reboots and
  starts what it runs every minute and at each logon until this checkout's tests run again
  and sweep it, and without end once the checkout is deleted, since no other sweep reaches
  it. The service manager's suites leave tasks that start the test binary, named
  `\noctua-dev-test-<checkout>-<pid>-<nonce>`; the restorer's suite leaves one that starts
  the relocated development daemon `--restored`, named
  `\noctua-dev-<checkout>-r<pid>n<nonce>.<SID>`, which ends at once on its mark or, its
  install removed, fails to start, every minute and at each logon. Each is removed by
  hand: the names `schtasks /Query /FO CSV /NH` lists under either prefix — a person's own
  relocated install, whose namespace may open with `r`, among them, so each is read before
  it is removed — with `schtasks /Delete /TN <name> /F`.
- **A task's counters are measured in English alone**: `Counters` reads a task's columns
  by their place, held to the task's own name, and whether it runs by its last result, so
  no word Windows translates is judged — and the hosted runner writes English, so a display
  language that orders those columns otherwise, or writes a last result some other way,
  is not one any run has shown.

**The command line**

- **A command's welcome is held to the system's word, not to the PID record**: the
  process a proven daemon names must be the one the system shows at the other end of the
  connection; process control reads the record and holds a process to it before it signals
  one, but a command's connection does not, so a record naming another process refuses no
  command.
- **`--json` is the canonical text of what the daemon sent**: canonical form escapes the
  C0 controls and writes every other character as itself, so a person who prints the
  document raw to a terminal prints a direction override or a C1 control a plugin's text
  held. The rendering for a person escapes them; the document is for a program to parse.
- **A quorum's refusal reaches the CLI as a denial**: a command a pack's rule holds to a
  quorum is refused by the decision point like any other denial (`NC-POL-0003`), so it
  ends the run in exit 3; only `NC-KERN-0006` and MCP's switch on's `NC-KERN-0032` say the
  interface is what is needed, and only they end a run in exit 4 — a call that waited for
  the interface ends in 14 or 15. Only an approval the kernel issued meets a quorum, and
  nothing issues one until the approval system (Stage 23), so a quorum's denial is a denial
  until that system gives the CLI something it can read, and Stage 23 chooses its exit code
  then.
- **A flag is a top-level property**: an argument nested in an object, or a property whose
  name holds `=`, is given as JSON — the object's flag, or `--` and one JSON object.
- **The registry answer is bounded by the frame, not below it.** One plugin's texts take at
  most half of `ipc_frame_max_bytes`, which the compiler holds; the whole answer — the
  catalog's commands, every plugin's texts, every command's categories and glyph, every
  argument schema, which no bound of its own holds — is not, so a profile with enough
  plugins installed asks for a registry the daemon refuses to send (`NC-IPC-0016`). The CLI
  asks for the registry before it runs a command, so it then neither lists nor runs any
  command, the kernel's own included, until a plugin is removed; the verbs that control the
  daemon and the help topics the binary carries still answer. Not taken: the answer paged,
  dispatch without the registry, or a schema bounded on its own.
- **The status is bounded per plugin, not in how many are installed.** What it keeps of
  each refused plugin's sentence is `REFUSAL_BYTES_MAX` bytes in `apps/daemon/src/plugins.rs`,
  so no one plugin takes it past a frame; enough plugins installed, each refused at that
  length, still would, and the status is then refused to the daemon (`NC-IPC-0016`) as the
  registry is. Not taken: a status that lists a count past some number of plugins.

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
  a start whose clock is behind its mark by more than the lease
  (`ids.clock_backwards`), a daemon started and stopped (`daemon.started`,
  `daemon.stopped`), a plugin it refused (`daemon.plugin_refused`), a pruner stuck
  (`daemon.prune_stuck`), an agent's session opened, ended or refused
  (`daemon.session_opened`, `daemon.session_ended`, `daemon.session_refused`) and a
  connection it refused (`ipc.refused`) — each a struct
  `catalog/log-events.toml` generates; nothing else of the platform logs yet. A daemon hands the kernel `nc_log::Log` behind a mutex, the
  one `nc_kernel::logging::LogSink` it writes its own events to too — to its standard
  error alone when its install names no store; the kernel's suites
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
  maximum, `with_label()` all three). A refusal carries no values, so the CLI shows a
  code whose message has parameters by its English detail alone, and the interface that
  could fill one is Stage 16's.
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

**The catalog's entries**

- **A wiki text is compiled in and linked, and never read.** No surface in this build
  prints one — the CLI shows no wiki text, and `docs/commands.md` and
  `docs/external-identifiers.md` link the file — so
  whether its Markdown renders as its author meant is not checked, and Typst is not
  rendered anywhere (`NOCTUA-PLAN.md`, Stage 27+).
- **A glyph's name is a comment.** The table holds code points the font draws, not what
  each depicts, so a glyph that draws something other than its comment says is accepted.
- **A category is what its standard said when it was read.** The `freedesktop` set is every
  category the Desktop Menu Specification 1.1 registers, main, additional and reserved, and
  the `apple` set Apple's list as published on the day it was read; nothing reads either
  again, so a category a standard adds or retires later is in neither until
  `catalog/categories.toml` is edited. Apple describes none of its values, so every `apple`
  line description is this catalog's own words. A `freedesktop` line description says what
  the specification's says, and is not its wording: one longer than a line is summarized,
  one that is not a plain sentence is rewritten as one, and `freedesktop.construction`'s,
  which the specification leaves blank, and those of `accessibility`, `ide` and `mixer`,
  which say little more than their names there, are this catalog's own; the
  specification's wording is at its address.
- **An external identifier is what was measured on the day it was seeded, and no more.**
  `catalog/external-identifiers.toml` holds Google Chrome and Chromium, each value measured
  on GitHub's hosted runners — ubuntu-latest, macos-latest and windows-latest — or read from
  Chromium's own source tree on that day, as its header says value by value; nothing reads
  either again, so an installer that renames a file or moves a path is not seen until the
  file is edited, and what a distribution packages under names of its own is in it only
  where a runner showed it. A value neither source confirmed is left out — the header and
  `NOCTUA-PLAN.md`'s Stage 27+ list which — so the registry is not complete for either
  product, and filling it is a pending task. Its texts are this catalog's own words.
- **Identification is a hint, never authentication.** Nothing in this build matches a value
  against a system. When something does, an executable's name, a desktop file, a bundle
  identifier and a path are each what any program the person installs can also carry, so a
  match says what a thing claims to be and never what it is, and no decision of the kernel
  may rest on one.

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

- **A feature's realm is the decision point's slice, not the policy's.** The permits in
  `policy/packs/family/features.cedar` ask no membership, so Cedar alone allows a person of
  another realm a feature — `policy/packs/family/tests/features.toml` pins that answer —
  and only the slice of the asking realm, which holds no other realm's people, refuses it.

**Agents and MCP**

- **A process running as the person goes around every agent.** The intersection binds what
  is dispatched through an agent's session, and nothing else. Such a process — an MCP
  host's own tool that runs shell commands for its model among them — runs the CLI as the
  person would, so it switches MCP off, makes, grants and ends the person's agents and undoes
  their steps — switching MCP on, and an undo that would, wait for the person in the Noctua
  window, which such a process can reach only by tampering with the desktop (*A call that
  cannot be undone runs once the person confirms it*); it reads the token and opens a connection that names no agent, which is the
  person; it runs `noctua mcp` as any of the person's agents; and it writes the profile's
  local database, where the switch is the kernel's entry and `{"on":true}` reads as on —
  only an entry holding anything but the two texts the set writes is refused at the next
  read, rather than read as on or as off. A caller naming a person is decided as that
  person, and naming an agent without a session decides it as itself, which the core
  refuses (`NOCTUA.md §11.1`).
- **Only a person's own call delegates, so no plugin sets MCP up for its person.** The
  switch and the three agents' commands that change run only at the top of a call tree: a
  plugin run as the person and a timer's or an event's callback are refused before any
  policy decides them, and journaled under `kernel.top_level_only` (`[NC-KERN-0025]`), and
  an agent and a device by the permits, which name a person alone.
- **An agent's identifier is neither a secret nor authentication.** It is the text of a
  minted value, matched and never parsed, and naming an agent a person made is never more
  than that person can do.
- **Nothing on the endpoint is encrypted**: a session's dispatches and their answers are as
  readable to a process running as the person as the person's own.
- **A grant to an agent is what keeps it from the household's shared files.**
  `family.a_member_reads_shared_content` permits any principal to read a `public` or
  `realm` file, and an agent the directory holds is refused one only because its grants are
  scoped to features and compared whole, so it holds no grant for content and
  `core.provenance` refuses; a grant scoped wider, or compared through an ancestor, would
  open every such file.
  `a_runtime_agent_granted_every_feature_reads_no_shared_content_its_pack_lets_any_principal_read`
  in `crates/nc-kernel/tests/the_decision_point.rs` holds that scope as it is.
- **Grants are per feature, not per command**: a feature granted reaches every command that
  selects it, so `naming` grants `naming.propose` and `naming.read` together.
- **The person's own half is decided on their world's grant, which nothing revokes while
  the daemon runs**: only its end on the kernel's clock is seen mid-session, as **A
  daemon's grant is the one its world file names, and nothing revokes it** says under
  **Channels and installs**. The decision point asks for it at every call, so a directory
  that withholds it refuses the agent's next one
  (`an_agent_loses_what_its_operator_loses_at_the_next_dispatch`).
- **An ending the keeper refused holds only until the directory is opened again** from what
  was kept; the refusal says so with the keeper's code.
- **No agent and no grant is ever deleted, and the bounds are the profile's, not a realm's
  or a person's.** `agents` and `agent_grants` are each counted whole, every realm's rows
  together and every grant ever given, ended ones included: a person in one realm can fill
  them for every realm the profile serves, and a profile that has made as many agents as
  `[tables.agents]` allows, or kept as many grants as `[tables.agent_grants]` allows, makes
  no more (`[NC-AI-0003]`). Not taken: an agent retired, and a bound per realm or per
  person, which `per` in `catalog/limits.toml` cannot say — it counts each plugin's rows or
  each unit's.
- **Making, granting or ending an agent is a step undo stops at**: the change is the
  profile's database's, outside every unit, so the step that made it left the machine, and
  the reverse of a grant is `agents.revoke`. Not taken: an undo that ends a grant, which
  would be a revocation under another name and bury every step before it.
- **The widest list of a person's agents fits one frame for the features the shipped world
  holds, and not a payload**: an agent holds one live grant per feature, so a realm of many
  more features can make the list of every agent at its bound longer than a frame, which the
  daemon then refuses whole (`NC-IPC-0016`) rather than send in part. Within the shipped world
  the widest list is already longer than `[kernel] payload_bytes`, so `agents.list`
  dispatched from inside a command can hand the plugin more than a payload, which that plugin
  cannot then answer with as its own.
- **An agent's `autonomy` is carried as `supervised` and read by nothing**: the schema
  requires one, and nothing in this repository defines what its values permit.
- **The operator's grant is decided and not named in the entry**: `grant_used` holds no
  grant but the agent's, so the entry says which half refused and not which of the person's
  grants decided theirs.
- **The deciding text tells an auditor which half refused, and the caller never**: the
  caller is told one sentence and the code.
- **No chain longer than one link exists outside the test door**: an agent another agent
  calls is a session of its own, of its person, one link deep.
- **An agent's command that cannot be undone waits only while the desktop the daemon
  started is attached**, and is refused at once otherwise; nothing an agent holds or sends
  confirms it, and an agent's session is refused an attach, a confirm and a decline
  (`NC-DMN-0015`).
- **What is refused before it reaches the kernel's dispatch is never journaled, and only
  the daemon logs what it refuses**: a session asked before the kernel runs or once a stop
  began, past `sessions_max`, while MCP is off or its switch cannot be read, for an agent
  the person did not make or while the directory does not answer, a request no session is
  answered, and a session's request once MCP is off or its switch cannot be read are each
  logged as `daemon.session_refused` by their code, and no journal entry holds them; nor
  does one hold a session's tools, which the kernel's probe answers without deciding a
  dispatch. A call is sent only when its line is read and its arguments are one object:
  what `noctua mcp` refuses before a command is named — a line that is not one request or
  that the platform's one reader refuses, a call that is not one object or holds a field
  besides its name, its arguments and `_meta`, a name that reads back to no command, and
  arguments that are not one object — is answered to its client and logged nowhere.
- **An agent's steps are in its person's history**: they can push the person's own steps
  past `undo_history`, end their way back to redo, and the person's next undo may take back
  their agent's newest step rather than their own — unreachable in the family pack, where
  an agent changes nothing.
- **The person's undo of a switch-off that is their newest step waits for them in the
  Noctua window**, as the switch on does (Stage 16): with no desktop attached it is refused,
  and the steps before it stay out of reach until a desktop is there to confirm it. Not
  taken: a switch-off made a barrier to undo, which would put every step before it out of
  the person's reach for good. `kernel.history` names the step before it is undone, and says
  whether an agent's call tree kept it.
- **An agent's steps have no bound of their own** (Stage 16): they count among their
  person's newest `undo_history`, as the person's own do, so an agent that writes often
  pushes the person's own steps out of reach sooner — what the history shows of each step
  is the answer, not a quota.
- **Switching MCP off ends a session at its next request, not before.** The daemon reads
  the switch when a session opens and before each of its requests, whatever it asks, so an
  idle session keeps its connection and its place among `sessions_max` until its next
  request is refused, and a dispatch the daemon admitted while MCP was on runs even when
  it waits on the kernel's bus behind the switch-off — at most one for each open session.
- **What an agent's tree set going keeps going until something refuses it**: a repeating
  timer it set fires as the agent for its person, decided again at each firing, until its
  session's budget, its grants or their end stop it, and the person's `agents.revoke` has
  its next firing refused; nothing ends it when the session ends, nor when MCP is switched
  off.
- **A session is a connection, not a conversation**: whatever runs over one connection
  shares its agent, its budget and how the journal names it.
- **The budget is a session's, and a count**: a session opened again counts from nothing,
  and the budget counts calls, firings and events, not their cost, and bounds no rate;
  `sessions_max` bounds how many sessions are open at once, not how many open over time,
  and the local journal takes an entry for every call of every session.
- **What an agent may list is a snapshot of what its session may dispatch, and is never
  pushed.** A session's list is what was true when it was asked: a grant ended, a budget
  spent or MCP switched off is seen at the next call or list, never announced. A command
  that takes its resource from an argument is not listed and not found, though a dispatch of
  it is decided as any, and every call is decided again whatever the list said.
- **A refusal reads alike for a tool denied and a tool absent, and takes about as long.**
  Both are sent and both answer an unknown tool; what tells them apart is the
  microseconds the kernel spends deciding one, against a model's round trip, and the
  registry is no secret — `docs/mcp-tools.md` publishes the catalog's.
- **`noctua mcp` learns late that the daemon could not read MCP's switch.** The daemon
  ends that session with the store's own code, which a dispatch's refusal can carry while
  its session stands, so `noctua mcp` answers the call as any refused one, and learns of
  the ending at its next request to the daemon, as the connection lost (exit 9), or not at
  all when its input ends first (exit 0); the daemon refused the request and closed the
  connection, so nothing more was dispatched.
- **What a person can read can still steer a model.** `noctua mcp` escapes only what a
  reader cannot see; a plugin's description, a command's answer and an argument echoed
  back reach the client as they are, and a tool's hint and a host's own prompt are
  hints, never a confirmation.
- **A call the client cancels still runs.** A cancellation is read and ignored: the call
  in flight runs to its end in the kernel and is answered.
- **A host that bounds a tool's name or a schema's names more tightly than the catalog
  does**: a name keeps its command's length, up to `nc_schema::NAME_MAX`, which a host
  that cuts a name with its own prefix at 64 characters can make collide, and a plugin's
  property names are not held to what such a host accepts.
- **A session's language is its environment's, read once.** The protocol carries none,
  so a host that hands its child no `LANG` gets English, and a change of the environment
  reaches the next session only.
- **An answer to a line whose id could not be read is not valid in `2025-06-18`'s
  schema.** Such an error carries no `id`, as `2025-11-25`'s schema allows, where the
  older revision's requires one; every answer to a request whose id was read is valid
  in both.
- **The shipped binary meets a live daemon in one recorded run, not in the gate**: the
  binary's suite meets no daemon, and the in-house client drives the same run in process,
  over pipes the system makes. The run `noctua mcp`'s security answer records as measured
  is the MCP Inspector CLI 2.15.0's, on Node 26.13.1, on 2026-11-22: it negotiated
  `2025-11-25`, listed `naming_propose` and `naming_read` alone, had a name proposed and
  read back canonical, and was refused with `NC-DMN-0014` once MCP was off. It is one
  client, on Linux, against a scratch install, and that client refused on its own a call
  of a tool it had not been listed, so the run sent none; what changed since is proven in
  process alone.

**What did not close as written.** Where the design of agents and MCP said otherwise, this
is what holds, each a maintainer's to reopen:

- **The widest `agents.list` is held to one frame, not to a payload**: at the bounds, for
  the features the shipped world holds, it is 1,867,532 bytes, which
  `the_widest_list_of_agents_fits_one_frame` holds within a frame and above a payload,
  against a design that held it within `[kernel] payload_bytes`.
- **The dev pages' reads have no feature of their own** (Stage 16). The Stage's plan named
  `feature:dev` and `feature:dev-records`, and the world holds neither: an agent may be
  granted every feature its realm holds, so one feature more takes the widest `agents.list`
  to 2,164,236 bytes, past a frame of 2,097,152
  (`the_widest_list_of_agents_fits_one_frame`). So the five reads of how the machine stands
  are decided on `Feature::"interface"` and the two records on the realm — under the family
  pack the same people the two features would have let in — and a pack cannot yet give the
  dev pages to fewer people than the interface, nor the records to somebody who may not
  read the realm. A feature of their own waits on that bound being moved: the frame, or
  what one person's agents may be granted.
- **A line of the live log carries no time**: the ring keeps the line the central log is
  sent, which its store stamps on arrival, so `dev.logs.read` orders lines by the serial
  each was recorded at and says nothing of when.
- **The live log is this daemon's since it started**: nothing of the run before it, nothing
  another service logged, and never the central log, which is read from its own store.
- **A page cut for its size is held by the cut's own test**: `fitting` is driven at its
  boundary, and no suite builds half a payload of rows to drive `next` through
  `dev.logs.read` or `dev.journal.read` that way.
- **`dev.limits.read` compares nothing.** It answers a table's rows beside the bound the
  catalog declares, and for a table whose bound is counted for each plugin or each unit
  that is the whole table's rows beside a number that does not bound them — `Counted over`
  in `docs/limits.md` says which. A limit that is a size or a time is not answered at all.
- **What a plugin uses is what the kernel counts of it** — its commands, surfaces, timers,
  the lines of its own log and its kills — never the memory or the fuel an instance has
  taken, which the engine holds for one call and no read reports.
- **A person who is not a guardian reads no log of their own machine**: the log names
  principals by their identifiers, so it is read with the realm's records or not at all.
- **`dev.journal.read` filters nothing**: it pages the realm's entries in order, and a
  reader looking for one command or one person filters what it was answered.
- **MCP is read before every request of a session, whatever it asks**: the design read what
  a session asks first (`a_sessions`), so a request no session is answered kept a session
  standing once MCP was off; the first review wave set the switch first.
- **A switch the daemon cannot read mid-session ends the session with the store's own
  code**, as designed, but `noctua mcp` cannot tell that code from a dispatch's refusal by
  the store, so it learns of the ending at its next request, as the connection lost.
- **The `2025-03-26` revision is not offered**: the schemas this server was written against
  are `2025-11-25`'s and `2025-06-18`'s, none shows its answers valid in `2025-03-26`, and
  the batching that revision reads is refused, so a client asking for it is answered in
  `2025-11-25`.
- **An error answering a line whose id could not be read omits `id`** rather than writing
  `"id": null`: `2025-11-25`'s schema allows an error with no `id` and none whose `id` is
  null, and `2025-06-18`'s requires one, so that answer is valid in the newer revision
  alone, as the bullet above says.
- **The third-party run is the MCP Inspector CLI's alone**, against a scratch install: no
  MCP host's run is recorded.

**The fuzzy core**

- **The browser's matcher follows the corpus as it last copied it, and nothing here
  compares the two.** `noctua-design` vendors `crates/nc-fuzzy/tests/fuzzy-corpus.tsv`
  with a `SHA256SUMS` record of its own and replays every row of its copy; its gate holds
  the copy to that record and never reads this repository, and this one pins the corpus by
  `FUZZY_CORPUS_SHA256` and never reads that one. So a row added here is one that matcher
  is not held to until the copy is refreshed there by hand. What the pin buys is that the
  change is seen: the corpus cannot change while this checkout's gate is green and the pin,
  which is the line that copy's record must hold, stays where it was.
- **Even then, two implementations agree only on the corpus's letters.** The core folds by
  `unicode-normalization`'s tables and Rust's case mappings, and a browser by its own, so a
  letter newer than the oldest of them may fold differently in each. The corpus keeps to
  letters they all agree on; once both replay it, a disagreement there is a defect, and
  anywhere else it is not claimed.
- **A match is not aware of graphemes.** A position is a character, so in an `e` written
  with a combining accent only the `e` is matched and coloured; drawing a cluster whole is
  the interface's (Stage 16).
- **No lookalike is folded.** A Cyrillic `а` does not find a Latin `a`, on purpose: a name
  that only looks like another is not found by it.
- **The ranking reads text alone.** Whoever writes an item's names and tags decides what
  finds it and how high, within the bounds its registry holds those texts to; `kernel.find`
  says of each command it found whether a plugin wrote it.
- **A person's `kernel.find` finds every registered command, not what they may
  dispatch.** Its answer is the registry `Request::Registry` already lists to the same
  person; an agent's is cut to what its session may dispatch now.
- **A search does not bound the registry it reads.** Its work grows with the commands
  registered — the kernel's, and each plugin's a person installed, each held to
  `commands_per_plugin` — and nothing caps how many plugins are installed; a search is
  matched inline in its dispatch.
- **Matching is not constant-time.** It handles nothing secret.

**The keys**

- **A system's shortcuts are what their sources write, and no more.**
  `nc_schema::keys::SYSTEM_SHORTCUTS` holds Microsoft's list for Windows, Apple's for
  macOS, GNOME's window manager's defaults and KDE Plasma's launcher's and window manager's,
  as each was fetched once; a chord another desktop, a later release or a person's own
  settings keep is not warned of, and a chord a row names is warned of on every system of
  its keyboard, whichever desktop runs there — the warning names the system it came from.
- **The vocabulary is a US board's keys, labelled by their place on one.** A key outside
  it — the `Intl` keys of an ABNT2 board among them — cannot be bound until the vocabulary
  grows, and a key's label is its US position's, so a person on another layout reads `[`
  where their board prints something else; the interface shows the layout's own label
  (Stage 16).
- **A person's configuration is read again only when a setting wrote it.** The daemon
  reads `<config>/keybindings.json` for the shortcut settings and, on Windows, for the
  Leader it asks for; the interface registers the Leader it reads, and reads the file again
  when the daemon's count of its settings' writes moves — so a file written by anything
  else is in force only from the next start of whoever holds the keys, and what happens
  between a read and a key bound is the interface's.
- **Only the channels under this install's bases are weighed.** A channel installed
  under other base directories — the same person's, configured another way — is not
  found, so a configured Leader is judged as if it were not there.
- **A checkout counts while its manifest is there, whether or not its daemon runs**, so
  one installed and never started holds its default Leader against a configuration of the
  development chord until it is uninstalled.
- **A file of a later format before a channel refuses that channel's configured Leader**
  until the earlier channel's file is written in a format this build reads, or this build
  is updated: two installs whose formats differ judge each other no further than that.

**The generated pages**

- **`docs/mcp-tools.md` is no session's tool list.** It is every command the catalog
  declares, projected by the function `noctua mcp` lists a session's tools with; which of
  them a session sees is decided when it asks, for the principal it is — only those whose
  resource is known without their arguments and that are not irreversible — and a plugin's
  tools are the daemon's to list. Each tool is named as its command with each dot an
  underscore and marked `readOnlyHint` when the command changes nothing; each locale has
  its own definition, and a session is handed the one its environment names.
- **`docs/cli.md` writes production's CLI**, `noctua`; every other channel's is the same
  but for its name (`NOCTUA.md §17.1`). Its commands are the catalog's alone: a plugin's
  are what `noctua help` lists while its daemon runs.
- **A function a surface hands the compiler is held by a test, not by its type.**
  `Surfaces` carries no command, but `Cli::described` is a function, and that it reads
  nothing it is not handed is held over the catalog
  `the_pages_read_the_catalog_they_are_handed_and_not_the_one_xtask_links` changes, and
  over no other table it could read.
- **`docs/keybindings.md` is the defaults**: a person's own keys are
  `noctua help keybindings`'s, and a key is labelled by its place on a US board, as
  **The keys** above says.

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

### A page about what the catalog cannot see is still the compiler's

Drift, staleness and the second generation read `Compiled.files`, so a page another step
wrote would be one none of them reads — and so would its prose, which only
`xtask/tests/the_pages_reach_the_catalog.rs` pins. The CLI's reference and the keybindings
page publish tables the catalog does not hold, and still come out of `compile`: the compiler
links neither `nc-cli` nor `nc-channel`, both of which read the crate it generates, so
`cargo xtask` hands it a `Surfaces` typed through `nc_schema`, and the files it is built
from are rows of `EMITS`, read by the determinism walk as the emitter is.

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
- **Every `cargo install` is one crate, locked, at an exact version**, `cargo install
  --locked name@X.Y.Z` and no other word, in every file CI reaches, read as a command is
  however its line is written: a crate named with no version, or with a range such as
  `@0.20`, is whatever crates.io holds on the day, and its code runs in the job that
  installs it, the release's gate among them; a second crate on the line is one no version
  names, and `--git`, `--path`, `--registry` or `--index` takes the code from where no
  version on crates.io describes it. `cargo-deny`'s version is written in six places and
  moved in one commit: the three installs of `ci.yml`, the gate job of `release.yml`,
  `THE_RELEASE_STEPS` in `xtask/src/workflow.rs`, which holds that line for line, and that
  line's row in `WHAT_ELSE_THIS_TOOL_SAYS` in `xtask/src/check.rs`, the census of the
  sentences the tool ships. The rule's test takes its install from `THE_RELEASE_STEPS` and
  fails when `ci.yml` holds no such line, as the release's own rule fails when
  `release.yml`'s gate does not; the three lines of `ci.yml` are not held to one another.
- **Every `npm install` is one package, global, at an exact version**, `npm install -g
  name@X.Y.Z` and no other word, in every file CI reaches, wherever `npm` stands on the line
  — a wrapper's argument included — **and each package at one version across them**
  (`every_package_names_its_version` in `xtask/src/workflow.rs`): pnpm installs and runs
  Tailwind inside the gate, so a pnpm with no version is whatever the registry holds on the
  day, and one moved in a job and not the others is two gates. Its version is written in
  six places and moved in one commit: the three jobs of `ci.yml`, the gate job of
  `release.yml`, `THE_RELEASE_STEPS` and its row in `WHAT_ELSE_THIS_TOOL_SAYS`, and the
  rule holds the files CI reaches to one another.
  `deny.toml`'s comment on `unsound` names the version whose default it read, and the
  paragraph above `yanked` the version whose `advisory-not-detected` warning it measured,
  and both are read again against the new one.
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
made, `A_POSIX_SHELL`, `POSIX_SESSIONS`, a session of the POSIX kind for a process to
leave, `NAMES_REACH_BY_LINKS`, a name in the bin directory reaching its program by a
symbolic link, and `A_TEST_PRESSES_A_HOTKEY`, a press of a registered hotkey that a test
may send — never a probe of the machine, so a Linux machine cannot stand a suite aside, and
`linux_lacks_nothing_a_suite_stands_aside_for` holds every answer on every system. That module is compiled in every build, not behind `test-support`, because the
gate reads it too. Each guard prints `skipped, not applicable on <system>:` and why — and
cargo counts the test as passed and captures that line with the rest of a passing test's
output, which the gate prints none of. So the gate's test step says it instead, from the
same constants: before the tests, a `not applicable` line for each suite this system stands
aside (`THE_SUITES_A_SYSTEM_STANDS_ASIDE`); after them, `the ones that need what this
system lacks` on the count line; and beside the verdict, `not applicable here: <suite>`.
`finish` holds those lines to the list as it holds the stack's parts, so on macOS (the
order on disk, and the part of the Leader's suite that presses a registered hotkey) and on
Windows (the order on disk, the checkout script's guards, the two suites about
leaving a session, `nc-platform`'s and the daemon's, and `nc-platform`'s about the link a
name in the bin directory reaches its program by) a suite that stopped saying so is a
refusal, and
`the_suites_a_system_stands_aside_are_the_ones_written_down_for_it` holds the list per
system, empty on Linux. `ANNOUNCES_WHAT_THE_SYSTEM_LACKS` in `xtask/src/check.rs` counts
the guards into the same line as the database suites, read by the same rule — a count of
the guards in the sources, the same on every system — with
`every_source_that_asks_what_the_system_lacks_is_one_the_census_names` naming every source
outside `nc-platform` whose code names a door; a suite of `nc-platform`'s own, where the
doors are defined, is a row written by hand and held to its count of guards the same way.
It looks for the door by its own name, as a
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
`unicode-normalization` was the developer's choice on 2026-11-14, over a table written
here, and `crossterm` on 2026-11-17, for the `checkouts` picker. The desktop interface's —
`wry`, `tao`, `dioxus`, `dioxus-ssr` and `dioxus-interpreter-js`, the design system's crates
and the stand-ins — were the developer's for Stage 16, over a licence audit of the three
systems' trees.

| Crate | Why it is not written here |
|---|---|
| `cedar-policy` | The authorization engine, `NOCTUA.md §4` [CLOSED]. `nc-policy` validates the pack with it and `nc-authz` decides with it, and a pack validated by one Cedar and decided by another is validated against a second implementation of its semantics. Cargo already resolves compatible requirements to one version (`cargo tree -i cedar-policy`), so the one `[workspace.dependencies]` line is not what unifies them: it is the one place the requirement is written, so raising it moves both, and it keeps either member from moving to another major version — a second Cedar beside the first — or to an exact `=` pin that silently decides the other's version |
| `sqlx` | Driver, pool and migration runner in one. `macros` is **on**, because `sqlx::migrate!` needs it and needs no database; what is refused is the `query!` family, which checks SQL against a live database *while compiling* — and `gates::no_compile_time_query` fails the build on one, in every Rust file in the tree. A feature list cannot say "this macro and not that one", so the rule is a source gate. From 0.9, text built at run time reaches `query`, `query_as`, `query_scalar`, `raw_sql` or an `Executor` only inside `sqlx::AssertSqlSafe(..)`, a claim the compiler takes on trust, so `gates::ASSERTED_STATEMENTS` names each file that makes one, how many, and every value their text is built from — a new wrap, or an old one built from a new value, is a finding until somebody has read where the value comes from. What stays outside what the compiler refuses is listed on `gates::Asserted` with how each is held — a reading of `sqlx`'s signatures, not a proof there is no other: `QueryBuilder`, whose `new` and `push` wrap whatever they are handed inside `sqlx` and which the same walk therefore refuses by name; `copy_in_raw` and `copy_out_raw`, which take a whole `COPY` statement as a plain `&str` and are read the way a wrap is, so a `COPY` built from values is counted in its file's row and one written as a literal needs none; `sqlx-sqlite`'s `SqliteConnectOptions::pragma`, whose key and value are any `Into<Cow<'static, str>>` written unquoted into a batch it runs on every connection, read the same way, so a pragma set from values is counted and one set from literals — every one this tree writes — needs no row; a migrator built when the store runs — `Migrator::new` over a path, `with_migrations`, a `MigrationSource`, `ResolveWith`, `resolve_blocking` — which reads its `.sql` then and hands the text to the database vouched for inside `sqlx`, refused by name in the sources `WRITERS` in `xtask/src/local.rs` names, where `sqlx::migrate!("../../local-migrations")`, embedded when the crate compiles, is the one migrator (`a_store_migrates_with_what_the_build_embedded_wherever_it_runs` opens a store from another working directory); and, held by nothing, a `String` leaked into a `&'static str` and the migrator's own table and schema names, which no migrator here sets. **SQLite, bundled, for the local database** (`NOCTUA.md §4`): `crates/nc-local`, `nc-prune`'s second target and `xtask`'s schema gate each take the `sqlite-bundled` feature — not `sqlite`, which in 0.9 also compiles extension loading, `deserialize` and unlock notification — and the workspace line stays without it. Which SQLite is linked is `Cargo.lock`'s choice, not a manifest's: `sqlx-sqlite` accepts `libsqlite3-sys` `>=0.30.1, <0.38`, `cargo update -p sqlx` alone keeps an old one, and `cargo tree -i libsqlite3-sys` says which; `nc-local`'s self-check at every open and the local schema gate in `check` refuse one below 3.50.2 |
| `tokio` | Arrives with `sqlx`, and drives the kernel and its tests — and `nc-local`, for its blocking pool alone, where an open waits for its profile's lock; `nc-platform`, for the daemon's socket and named pipe (`net`), because `create_with_security_attributes_raw`, the one call that makes a pipe with a descriptor of its own, is `unsafe` and that crate is where `unsafe` may be written; and `nc-ipc`, for the frames' reads and writes, the handshake's deadline, the heartbeat and the two tasks a connection runs on — and, behind that crate's `test-support` feature alone, which is off by default and which only a suite's `[dev-dependencies]` turn on, `net` for the runtime the scripted daemon of `nc_ipc::testing` binds its endpoint on, with `nc-platform`'s own `test-support` doors beside it; `nc-daemon`, whose run is a multi-threaded runtime — so a plugin running on one worker never holds up the heartbeat of a connection on another — with `time` for its schedule and `sync` for its state, its stop and the state a watch waits on (and `test-util` in its tests alone, for the paused clock a watch's waits are measured on); and `nc-cli`, for the one-thread runtime a run of the CLI makes its one connection on (`rt`, `net`, `time`), and, for `noctua mcp`, the blocking pool its standard streams are read and written on, which `rt` already carries — a direct dependency there, already in the graph so taking it adds nothing to the lockfile, and still the developer's to confirm (`§2.3`): `wasmtime` instantiates and calls a plugin asynchronously, so that a host function can await the journal, the directory and the store, which are asynchronous themselves. The kernel takes `sync`, for the journal and the bus a call tree holds across an await, and `time`, for the deadlines the directory and every host call a plugin makes are held to — and runs a nested dispatch as a task of its own, off the plugin's stack |
| `wasmtime` | The plugin runtime, `NOCTUA.md §4` [CLOSED]: the Component Model, fuel and epoch interruption. Every default feature is off, and the feature list beside the `wasmtime` line in `Cargo.toml` names the ones the kernel runs, with what was left off and why — `call-hook` among them, which is how the epoch counts a plugin's own running and not the host's — and `cache` above all of what is off, whose reading half is `Component::deserialize`, which is `unsafe`, so the kernel compiles a component on every load |
| `wit-bindgen` | The guest half of the same world (`§4`: the Component Model "with `wit-bindgen`"), in `crates/nc-guest` only, `macros` only. Its `build.rs` links two prebuilt archives into every `wasm32` build — `deny.toml` says which and why its build-script rule does not see them |
| `wit-component` | What turns a plugin's core module into a component, inside `cargo xtask` rather than as a tool somebody installs, and what reads the component back. The encoder refuses only an import no embedded world describes, and a dependency with bindings of its own embeds one — `wasip2`'s `wasi:random` encoded without a word — so `xtask/src/plugins.rs` decodes each component and refuses any import the world `plugin` does not declare |
| `wit-parser` | The world as a parser reads it, for what a module and the kernel's generated bindings cannot say about the world they came from: its declared version, what the world `plugin` imports for `cargo xtask build --plugins` to hold a component to, and every function it holds for the capability census |
| `ureq` | The HTTPS client behind `net.fetch`, decided on 2026-09-28 — real HTTPS, with every rule about what may be asked the kernel's and the client only carrying a request already judged. Blocking, so the kernel runs it on the runtime's blocking pool under the host call's deadline; `rustls` alone, with `ring` and `webpki-roots` — Mozilla's root bundle compiled in, because a root store read from the platform reads `SSL_CERT_FILE` and `SSL_CERT_DIR` and the environment would choose whom the kernel trusts. `crates/nc-kernel/src/net/ureq.rs` configures it and gives it a resolver that dials only public addresses; its `Resolver` is outside ureq's semver promise, so the lockfile decides the version (`cargo tree -i ureq`) |
| `ureq-proto` | `ureq`'s own HTTP/1.1 protocol, already in the graph under it at the version `ureq` requires, so it adds no package (`cargo tree -i ureq-proto`). Taken directly only because the kernel matches on its `Error`, which `ureq::Error::Protocol` carries and `ureq` does not re-export: an answer with more headers than its parser holds is told as an answer too large (`[NC-PLUG-0014]`), not as a network that did not answer. A `ureq` moving to another release line of it carries an `Error` that match does not name, and the kernel fails to compile |
| `url` | The WHATWG parser a plugin's URL is held to before anything is sent, as a browser reads one, so every spelling of an address parses as an address the kernel refuses rather than as a name. Already in the graph under `sqlx`; taken directly because the kernel matches on its `Host` |
| `blake3` | The journal's chain and the machine flake both hash (`§14.9`, `§16.2`), a confirmation is bound to its dispatch's arguments by their digest, a blob is named by the hash of its bytes (`§13`), computed in `nc-kernel` as they are written, and each end of an IPC connection proves it holds the channel's token by a hash keyed by it, compared in constant time (`crates/nc-ipc`). Already `noctua-colors`' |
| `unicode-normalization` | The canonical decomposition `crates/nc-fuzzy` folds a letter to its base letter by (`NOCTUA.md §12.4`): `é` to `e`, `ç` to `c` — the rule `noctua-design`'s matcher asks the browser for as `normalize("NFD")[0]`. Not written here because the table is Unicode's data, and a transcription of it is a defect this tree has measured once: `crates/nc-ids/src/naming.rs` records a hand-written table two entries out of step. A direct dependency of `nc-fuzzy` alone, `default-features = false`, since the decomposition it calls needs nothing of `std`. Already in the graph at this version under `stringprep` and `unicode-security`, with `tinyvec`, so it adds no package (`cargo tree -i unicode-normalization`). Its Hangul path holds `unsafe` of its own, which this workspace's `forbid` does not reach, as for every dependency |
| `crossterm` | The terminal the `checkouts` picker runs in (`NOCTUA.md §17.1`): raw mode entered and always left, the alternate screen, keys read as events and lines drawn, on Linux, macOS and Windows, whose console is set through calls of its own — a terminal's modes are each system's to get right, and getting them wrong leaves a person's shell unusable. A direct dependency of `nc-cli` alone, used by `apps/cli/src/terminal.rs` alone, `default-features = false` with `events` and `windows`, without which it refuses to build on Windows. It brings `crossterm_winapi`, `winapi` and its two import crates for Windows, `signal-hook`, `signal-hook-mio` and `signal-hook-registry` on Unix, and `document-features` with `litrs` while it compiles; `mio`, `rustix`, `parking_lot` and `bitflags` were in the graph already (`cargo tree -i crossterm`). Its own `unsafe` is outside this workspace's `forbid`, as every dependency's is |
| `serde`, `serde_json`, `toml` | Reading the registries and writing canonical output, and reading a command's argument schema for `crates/nc-schema` to validate — and the arguments the CLI holds to it, read back from their canonical text as the kernel reads them |
| `proptest` | `§16.3` and `§16.4` have algebraic laws, and `§19.2` puts those under property test |
| `clap`, `anstream`, `anstyle` | The command surface, in `xtask` only |
| `rustix` | Unix's process calls the standard library does not make, in `crates/nc-platform`'s Unix half and, for one call, its X client, and for a Unix target alone — decided on 2026-11-08: a session of the daemon's own (`setsid`), and the session a test reads back (`getsid`); a process held to its PID record and signalled through it — a pidfd on Linux (`pidfd_open`, `pidfd_send_signal`), signal 0 and `kill` on macOS — and the user the process runs as (`getuid`, and `geteuid` where no private directory is at hand to probe, and in `x11.rs`, the X client every system compiles, which asks it for whose socket file a server's may be); and the group a test reads back (`getpgid`), all through safe wrappers, so no FFI is written on Unix. `default-features = false`, with `std` and `process` and nothing else. Already in the graph at this version under `wasmtime`, so it adds no package (`cargo tree -i rustix`) |
| `ed25519-dalek` | A device's key (`NOCTUA.md §16.2`): the Ed25519 public key a profile's flake and its journal's keeper derive from, and the key iroh takes as the device's identity in Stage 25 (`§13.2`), at the major iroh pins — a cryptographic primitive, which `§2.3` names as not viable in-house. `default-features = false` and `zeroize` alone: no `rand_core`, because the seed is drawn by `nc_platform::random`; no `fast`, because the one public key a start derives needs no precomputed tables. Its family arrives with it and adds nothing to write: `curve25519-dalek` (and `curve25519-dalek-derive`, its SIMD backend's macro), `ed25519` (the signature's type), `signature`, `fiat-crypto` (a backend compiled for some targets only), and `rustc_version`, which `curve25519-dalek`'s build script reads; `sha2` 0.11 and `subtle` are in the graph already. `cargo tree -i ed25519-dalek` says who takes it |
| `windows-sys` | The Win32 security calls of `crates/nc-platform`'s Windows half, and only there, compiled for a Windows target alone — decided on 2026-11-02: who the process token names (`OpenProcessToken`, `GetTokenInformation`), an access list built (`InitializeAcl`, `AddAccessAllowedAceEx`) and read (`GetSecurityInfo`, `GetAce`), a file and a directory created with a descriptor of their own (`CreateFileW`, `CreateDirectoryW`), the system's random bytes, which a device's seed and the daemon's token are drawn from (`BCryptGenRandom`), and the other end of a named pipe — its process from either end (`GetNamedPipeClientProcessId`, `GetNamedPipeServerProcessId`, feature `Win32_System_Pipes`), the user that process's token names (`OpenProcess`, `OpenProcessToken`), and the pipe's owner (`GetSecurityInfo` over a kernel object); when this process started, which its PID record holds (`GetProcessTimes`); a process held to its PID record through one handle — opened to query, terminate and wait for it (`OpenProcess`), its creation time, its image's path (`QueryFullProcessImageNameW`) and its token's user read through it, and it ended and waited for through it (`TerminateProcess`, `WaitForSingleObject`), and one this user may not hold read through a handle opened to query it alone, which is never held; and its own standard handles kept from every child but as the handles a spawn hands one (`SetHandleInformation`); and, behind the test doors alone, a descriptor written onto one path or one process through its handle (`SetKernelObjectSecurity`), from its text when it holds a conditional entry (`ConvertStringSecurityDescriptorToSecurityDescriptorW`), and a thread made to act as this user holding no privilege (`CreateRestrictedToken`, `ImpersonateLoggedOnUser`, `RevertToSelf`); and the Leader's hotkey, registered on a thread that reads its own message queue (`RegisterHotKey`, `UnregisterHotKey`, `MapVirtualKeyW`, `PeekMessageW`, `GetMessageW`, `PostThreadMessageW`). Not written here because the declarations — every structure's layout and every function's signature — would be ours to get right, with no compiler to check them against the system's. Version 0.61, which `tokio`'s `mio` already brings, so it adds no package; its features are the modules those calls are in, pinned in that crate's manifest. It is the one dependency whose calls are `unsafe`, and invariant 17 says how that is held — the other `unsafe` this tree writes is around Carbon's hotkey calls, which the macOS half declares itself |
| `noctua-design`, `noctua-design-dist` | The only cross-repository dependencies this repository has, the design system's own crates, at one release from crates.io, where it publishes them (`NOCTUA.md §10.11`) — a release resolves in a clone made anywhere and on every runner, which a sibling's path does not, and needs no `allow-git` in `deny.toml`, which a git tag would. `nc-desktop` takes both: `noctua-design` for its components — the footer, its lines and its corners, the drawer and the sidebar, the drawer's rails, a toolbar and a tab strip in their rail forms, and the option list, its groups, its options and the text a query's words are marked in, and what a plugin's surface is drawn with: a paragraph, a heading, a button, a switch, a field of one line, of a number and of several lines, a list and its items, and a divider — and its theme model, the palette and the mode a document's root is stamped with; `noctua-design-dist` for every file it ships, served by name through the interface's custom protocol (`apps/desktop/src/assets.rs`), the order its sheets are linked in, read from its own `base.html`, and each palette's values, the dark surface a dark window's webview is painted with read from them. `xtask` takes `noctua-design-dist` for the Tailwind bridge the desktop's sheet is built against (`xtask/src/tailwind.rs`). `noctua-design` 0.3.0 brings its own crates — its grammars, icons, lineage, theme and tokens — and nothing of a third party's. `noctua-colors` is reached **through** them and never directly (`§8.6`), which a gate holds |
| `wry`, `tao` | The desktop interface's window and the system's own webview in it (`NOCTUA.md §4`, "Desktop app shell") — WebKitGTK on Linux, WKWebView on macOS, WebView2 on Windows — hosted directly rather than through `dioxus-desktop`, whose tree brings `image` (an AV1 encoder under BSD-2-Clause), a tray, a menu bar, a global hotkey, a file dialog and an edit socket bound on loopback, which `apps/` may not write. Neither is viable in-house (`§2.3` names a browser engine). Pinned exactly: `wry` with `os-webview` and `protocol` alone — the custom protocol every asset the webview loads is served through — and `tao` with `rwh_06` alone, the root manifest saying why each, and the one call each system makes its own way — GTK's box on Linux, the window's handle elsewhere — is `nc_platform::webview`, behind the feature only `nc-desktop` turns on, so the daemon and the CLI link no webview. On Linux they bring GTK 3's Rust bindings, `glib` 0.18 among them, which `deny.toml` answers for |
| `dioxus`, `dioxus-ssr` | The interface's markup, written as `rsx!` (`NOCTUA.md §4`, "Rust UI framework"), its first frame rendered into the document the webview is handed before the window is shown (`apps/desktop/src/page.rs`), and its footer rendered again from each state the daemon answers, which the page is brought to (`apps/desktop/src/footer.rs`). Pinned exactly at the release `noctua-design` 0.3.0 is built with, every crate of the family at it in the lockfile; `dioxus` with `macro`, `html` and `signals` alone, each a feature `noctua-design` 0.3.0 enables itself, so the crate that takes the design system adds none |
| `dioxus-interpreter-js` | Dioxus's interpreter, the module that applies a virtual DOM's edits in a webview, served through the interface's custom protocol beside the design system's files (`apps/desktop/src/assets.rs`). Pinned exactly with the family, and with no feature: what it ships ready-made as text, and none of the bindings its features generate, which take `sledgehammer_bindgen`, `sledgehammer_utils` and `rustc-hash` — crates this list does not hold. Its build script's `lazy-js-bundle` is already in the graph under `dioxus-html`. No page loads it yet: the footer is brought to the markup each state draws by the document's own script, which changes only what the markup changed (`apps/desktop/src/page.rs`) |
| `dirs`, `kuchikiki` (stand-ins) | Not dependencies: members of this tree's own, in `stand-ins/`, put in place of two of `wry`'s by `[patch.crates-io]`. `dirs` answers the one function `wry` calls with nothing, and `kuchikiki`, which `wry` names for Android alone, is empty; the crates they stand in for take `option-ext`, `cssparser` and `selectors`, MPL-2.0, which `deny.toml` admits nowhere. The root manifest says why, and a release of `wry` that needs another version of either is refused by cargo until the stand-in moves with it |

**Four things are written here rather than taken**, all four a maintainer's call: the
HTTP client that ships log lines to a store on the same machine, the harness that proves a
dynamic string does not compile into a log event, the argument-schema validator in
`crates/nc-schema` — a closed subset, so a keyword it does not implement is refused rather
than ignored (decided 2026-09-28) — and the Leader's delivery in `crates/nc-platform`
(decided for Stage 16): a client of the X11 protocol for the seven requests a grab needs,
in safe Rust, and the declarations of Carbon's six hotkey functions, where a crate would
bring a C library's bindings or the rest of Carbon with it.

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
