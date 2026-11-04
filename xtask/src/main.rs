//! The repository's command surface: five verbs, and the surface does not grow.
//!
//! `NOCTUA.md §10.2` fixes the five. A new capability goes behind one of them —
//! a flag, a subcommand of a verb, another step inside `check` — and never
//! becomes a sixth. The reason is the same one that made the other two
//! repositories converge on it: a developer and CI must be able to hold the
//! whole surface in their head, and every verb added is one more thing that can
//! be true locally and false in CI.
//!
//! Every verb returns `Result<(), String>`. A plain `String` and not a
//! diagnostic type, because the only consumer is `main`, which prints it and
//! exits — a richer error here would be a richer error nobody reads.
//!
//! **Every line this tool prints goes through `ui`, and the compiler holds it.**
//!
//! A test scanning for `println!`, `stdout()` and four more spellings cannot
//! hold it. A needle list cannot win: `let handle = std::io::stdout;` then
//! `handle()` contains none of them, and past such a list it would put a fifth
//! `warn` line on the terminal under a verdict saying four, with exit 0 — the
//! exact failure the scan exists to prevent. `dbg!` and a `printf` child
//! process walk past such a list too.
//!
//! Name resolution is not a substring problem, so it is given to the thing that
//! resolves names. `ui.rs` carries the one `expect` that earns it.
//!
//! **And the same deny is written at every other crate's root.** A lint level
//! does not cross a crate boundary, and `print_stdout` is `restriction`, which
//! neither `all` nor `pedantic` turns on — so this attribute binds `xtask` and
//! nothing else. Without the same deny at that crate's own root, one `println!`
//! in `crates/nc-catalog-compiler`, inside a function the gate calls twice,
//! would put two `warn` lines on the terminal that the verdict's
//! `with N warn line(s) above` does not count, with exit 0 and clippy silent.
//! The verdict's warn count and its skipped list are true only if every line
//! goes through `ui`.
#![cfg_attr(
    not(test),
    deny(
        clippy::print_stdout,
        clippy::print_stderr,
        clippy::dbg_macro,
        clippy::disallowed_methods
    )
)]
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod build;
mod check;
mod corpus;
mod counts;
mod database;
mod determinism;
mod dev;
mod docs;
mod gates;
mod limits;
mod local;
mod members;
mod plugins;
mod policy;
mod references;
mod release;
mod sections;
mod store;
mod wall;
mod workflow;

pub(crate) mod ui;

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "The noctua-central build tool.",
    long_about = "Five verbs, and the surface does not grow (NOCTUA.md §10.2).\n\
                  \n\
                  build    compile catalog/ into its generated crate and pages\n\
                  check    THE gate. CI runs it, `dev` for the stack it needs,\n\
                           the `cargo install` for `cargo deny`, and `build` twice\n\
                  dev      bring up PostgreSQL and the central log store\n\
                  docs     regenerate the self-documentation surfaces under docs/\n\
                  release  write a version everywhere it appears"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile the catalog and write the crate that carries it.
    Build {
        /// Accept a ledger with no checksum, and reserve every code it then
        /// holds. For a new catalog, once — never to get past a checksum.
        #[arg(long)]
        reserve: bool,
        /// Build every plugin for wasm32 and encode it as a component instead.
        ///
        /// Each is linted and documented for that target too. `check` does
        /// it before the tests; this is the same step alone.
        #[arg(long, conflicts_with = "reserve")]
        plugins: bool,
    },
    /// The whole gate.
    ///
    /// CI runs four cargo commands and this is the one that judges: `cargo xtask
    /// dev` brings up the stack it needs, `cargo install --locked cargo-deny`
    /// installs the auditor, and `cargo xtask build` runs twice in the job that
    /// diffs two builds. `crate::check::WHAT_CI_RUNS` is that set, held against
    /// the workflow and against every sentence stating it.
    Check {
        /// Skip the nine steps that need the local stack or the network. CI
        /// never passes this.
        #[arg(long)]
        offline: bool,
    },
    /// Bring up the local stack and apply the migrations.
    Dev {
        /// Tear the stack down and remove its volumes instead of bringing it up.
        #[arg(long)]
        down: bool,
    },
    /// Regenerate the self-documentation surfaces under docs/.
    Docs,
    /// Write a version everywhere it appears. Prints the git commands; runs none.
    Release {
        /// The new version, as three numbers.
        version: String,
        /// Say what would change and change nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = repository_root();

    let outcome = match cli.command {
        Command::Build { plugins: true, .. } => plugins::run(&root),
        Command::Build { reserve, .. } => build::run(&root, reserve),
        Command::Check { offline } => check::run(&root, offline),
        Command::Dev { down } => dev::run(&root, down),
        Command::Docs => docs::run(&root),
        Command::Release { version, dry_run } => release::run(&root, &version, dry_run),
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            ui::failure(&error);
            ExitCode::FAILURE
        }
    }
}

/// The repository root, from the manifest rather than from the working
/// directory.
///
/// `cargo xtask` can be run from any subdirectory — that is the entire point of
/// the alias — so `$PWD` is not the root and reading it would make every path in
/// this tool depend on where the developer happened to be standing.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ always has a parent")
        .to_path_buf()
}

/// A repository-relative path with forward slashes, so a message reads the same
/// on every platform.
pub(crate) fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
