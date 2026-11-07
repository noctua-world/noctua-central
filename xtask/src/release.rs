//! The release: a channel's version and changelog, computed from the history,
//! written, read back, and published.
//!
//! `cargo xtask release --channel staging|main` (`NOCTUA.md §22.6`) computes
//! the version from the conventional commits since the last production release
//! (`NOCTUA.md §22.3`), regenerates the changelog whole from the history and
//! the release tags (`NOCTUA.md §22.4`), runs the gate before anything is
//! written, writes the version everywhere [`CARRIES_THE_VERSION`] names,
//! relocks, and reads every file back through a reader that is not the
//! writer's. With `--publish`, which only CI's release job passes, it then
//! makes the release on GitHub through the `gh` CLI the runner carries: the
//! release commit, the tag, the GitHub release with its notes, and the pull
//! request that carries it back to the branch below. That job passes
//! `--after-the-gate-job` too: the gate ran in the job before it, on the same
//! commit, where neither the App's key nor its token is, and the verb refuses
//! the flag anywhere else ([`the_gate_ran_in_the_job_before`]). `--dry-run`
//! runs no gate, writes nothing, and says what the release would be.
//!
//! # A version is computed, never handed in
//!
//! Nothing a caller passes chooses the version: the channel is the branch the
//! release is made from, and the rest is git — the tags `vX.Y.Z` and
//! `vX.Y.Z-rc.N`, and the messages of the commits between them, read through
//! the history step's own reader (`crate::history`), which starts git over
//! this tree's `.git` alone and refuses a shallow or grafted clone. A version
//! this release cannot order is refused rather than guessed at: ordering
//! anything but those two shapes is semver's, a dependency this repository has
//! not taken (`NOCTUA.md §2.3`).
//!
//! # Why the list of files is written down
//!
//! [`CARRIES_THE_VERSION`] names every file that holds one, and a file in it
//! that does **not** hold the current version is an **error** rather than
//! something to skip. The alternative — search the tree and rewrite what matches
//! — is the shape that leaves one file behind on the day somebody adds it, and a
//! release where one file disagrees is worse than a release that refused: the
//! disagreement is discovered by whoever is debugging something else.
//!
//! # Publishing is carried on, never made twice
//!
//! The release commit is made through GitHub's `createCommitOnBranch` with
//! the commit the job checked out as the branch's expected head, so a branch
//! that moved meanwhile fails the release rather than receiving it. A job that
//! died part way runs again on the push its own release commit causes, and
//! there `HEAD` is that commit: the verb asks GitHub what exists — the tag, the
//! release, the pull request — and makes only what is missing, in order, so a
//! run on a release already made ends with `already released`. A refusal at
//! the release commit or after it puts back every file the run wrote, as any
//! refusal after the first write does.
//!
//! # What a release that cannot be carried back says
//!
//! The pull request back has the released branch as its head, and that branch
//! takes nothing but pull requests and the release App: a conflict on it is
//! resolved on another branch. When GitHub says the pull request conflicts
//! with the branch below, or refuses to turn auto-merge on for it, the release
//! stops and says how a person carries it back ([`carried_back_by_hand`]).

use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::history::Own;
use crate::ui;

/// Every file that names the repository's version.
///
/// One entry today. It grows in the change that adds a file naming a version —
/// never afterwards, because "afterwards" is a release that shipped with two.
const CARRIES_THE_VERSION: [&str; 1] = ["Cargo.toml"];

/// Where the changelog is written, at the root of the repository.
pub(crate) const CHANGELOG: &str = "CHANGELOG.md";

/// The flag that stands the verb's own gate aside, because the job before
/// ran it ([`the_gate_ran_in_the_job_before`]).
///
/// **Refused in every file CI reaches but the release workflow**, by
/// `crate::workflow`, which reads it from here.
pub(crate) const AFTER_THE_GATE_JOB: &str = "--after-the-gate-job";

/// The workflow the release job runs in, as GitHub names it in
/// `GITHUB_WORKFLOW_REF`: from the root of the repository.
const THE_RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";

/// The job of [`THE_RELEASE_WORKFLOW`] that releases, after the job that ran
/// the gate, as GitHub names it in `GITHUB_JOB`.
const THE_RELEASE_JOB: &str = "release";

/// The page where GitHub publishes its limits on a release's assets.
///
/// **The two numbers below were read there on 2026-11-02, and that page is
/// where they are checked again.** The documents name this page and the
/// command that prints the assets against it, never the numbers, so a
/// limit GitHub moves is moved here once.
const GITHUBS_LIMITS: &str =
    "https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases";

/// How many assets one release may carry, as [`GITHUBS_LIMITS`] says.
const MOST_ASSETS: usize = 1000;

/// What every asset must stay under, as [`GITHUBS_LIMITS`] says: 2 GiB.
const LARGEST_ASSET: u64 = 2 << 30;

/// Every asset a release carries, with its size: none before Stage 13, which
/// builds the first binaries.
///
/// **Judged against GitHub's limits however short it is**
/// ([`within_githubs_limits`]), so the day it gains its first binary the
/// limits are already being asked, before anything is published.
const ASSETS: [(&str, u64); 0] = [];

/// What a release commit's subject opens with, before its version.
const RELEASE_COMMIT_OPENS: &str = "chore(release): v";

/// How much of a commit's hash a changelog entry shows.
const SHORT: usize = 7;

/// What the changelog opens with, above its sections.
const OPENING: &str = "# Changelog\n\nWhat each release of this repository adds, changes, \
                       fixes, speeds up, secures and removes, newest first (`NOCTUA.md \
                       §22.4`). `cargo xtask release` writes this file whole from the \
                       conventional commits between the release tags every time it \
                       makes a release, so an edit by hand is gone at the next one.\n";

/// What a release's section says when none of its changes is one the
/// changelog lists.
const NOTHING_TO_LIST: &str = "Nothing in this release changes what a person or a developer \
                               sees: every change in it is of a type the changelog leaves \
                               out.";

/// The parts a release's section is written in, in the order they are
/// written: what a person must act on first.
const PARTS: [Part; 6] = [
    Part::Changed,
    Part::Security,
    Part::Added,
    Part::Fixed,
    Part::Performance,
    Part::Removed,
];

/// What the release asks GitHub before it makes anything: whether the tag,
/// the release and the pull request that carries it back already exist.
///
/// **Each pull request says where its head is**, because `headRefName`
/// matches a branch of that name in every fork as well as here, **and how
/// close it is to merging**, because GitHub refuses auto-merge on one it
/// would merge now.
const WHAT_GITHUB_HOLDS: &str = "query WhatGitHubHolds($owner: String!, $name: String!, \
                                 $tag: String!, $ref: String!, $head: String!, $base: String!) \
                                 { repository(owner: $owner, name: $name) { ref(qualifiedName: \
                                 $ref) { target { oid } } release(tagName: $tag) { tagName } \
                                 pullRequests(headRefName: $head, baseRefName: $base, first: \
                                 100, orderBy: {field: CREATED_AT, direction: DESC}) { nodes { \
                                 number id title state isCrossRepository headRepository { \
                                 nameWithOwner } mergeStateStatus autoMergeRequest { \
                                 mergeMethod } } } } }";

/// The release commit, made on the branch by GitHub, which signs it.
const THE_RELEASE_COMMIT: &str = "mutation ReleaseCommit($input: CreateCommitOnBranchInput!) \
                                  { createCommitOnBranch(input: $input) { commit { oid } } }";

/// Auto-merge, by merge commit, on the pull request that carries a release
/// back.
const AUTO_MERGE: &str = "mutation AutoMerge($id: ID!) { enablePullRequestAutoMerge(input: \
                          {pullRequestId: $id, mergeMethod: MERGE}) { pullRequest { number \
                          autoMergeRequest { mergeMethod } } } }";

/// The merge, by merge commit, of a pull request that carries a release back
/// and whose checks have passed already.
const MERGED_NOW: &str = "mutation MergedNow($id: ID!) { mergePullRequest(input: \
                          {pullRequestId: $id, mergeMethod: MERGE}) { pullRequest { number \
                          merged } } }";

/// Whether the branch below already holds a release commit: GitHub compares
/// it with that branch's head.
///
/// **A commit's hash is a head GitHub compares**, as a branch's name is:
/// measured on 2026-11-05 against this repository, `dev` compared with its
/// own head answered `IDENTICAL`, with a commit it does not hold `AHEAD`, and
/// with one of its older commits `BEHIND`.
///
/// **A branch below that moved on since answers `DIVERGED`**: each holds a
/// commit the other does not, as `dev` does once it takes a merge `staging`
/// has not. Measured on 2026-11-06 with a branch here and a commit it does
/// not hold, made on an older head of it.
const WHAT_THE_BRANCH_BELOW_HOLDS: &str = "query HeldBelow($owner: String!, $name: String!, \
                                           $below: String!, $at: String!) { \
                                           repository(owner: $owner, name: $name) { \
                                           ref(qualifiedName: $below) { compare(headRef: \
                                           $at) { status } } } }";

/// A part of a release's section of the changelog (`NOCTUA.md §22.4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// A breaking change, and what to do about it.
    Changed,
    /// A change to the security posture.
    Security,
    /// A `feat`.
    Added,
    /// A `fix`.
    Fixed,
    /// A `perf`.
    Performance,
    /// A `revert`.
    Removed,
}

impl Part {
    /// The heading the part is written under.
    const fn heading(self) -> &'static str {
        match self {
            Self::Changed => "Changed",
            Self::Security => "Security",
            Self::Added => "Added",
            Self::Fixed => "Fixed",
            Self::Performance => "Performance",
            Self::Removed => "Removed",
        }
    }
}

/// A channel a release is made on, named as the branch it is made from
/// (`NOCTUA.md §22.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Channel {
    /// Release candidates, `X.Y.Z-rc.N`, published as pre-releases.
    Staging,
    /// Production, `X.Y.Z`.
    Main,
}

impl Channel {
    /// The branch the release is made from, which is the channel's name.
    const fn branch(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Main => "main",
        }
    }

    /// The branch a release made here is carried back to.
    const fn below(self) -> &'static str {
        match self {
            Self::Staging => "dev",
            Self::Main => "staging",
        }
    }
}

/// How far a run of the verb goes once it knows the release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Say what the release would be: no gate, and nothing written.
    DryRun,
    /// Write the version, the changelog and the lockfile, read them back, and
    /// stop.
    Write,
    /// Write them, and make the release on GitHub.
    Publish,
}

/// A version this release can order: `X.Y.Z`, or `X.Y.Z-rc.N`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Version {
    /// The three numbers.
    core: [u64; 3],
    /// The release candidate's number, or `None` for a production release.
    rc: Option<u64>,
}

impl Version {
    /// `text` read as a version, or why it is not one this release can order.
    ///
    /// **Each number written the way it parses back.** `"01".parse::<u64>()`
    /// is `Ok(1)`, so `01.02.03` would be read as a version, and cargo's semver
    /// parser refuses a leading zero: written into the manifest, it leaves a
    /// workspace nothing builds, this tool included, so the tool could not be
    /// run again to undo its own write. A candidate is numbered from one.
    fn parse(text: &str) -> Result<Self, String> {
        let refused = || {
            format!(
                "`{}` is not a version this release can order. It writes `X.Y.Z` and \
                 `X.Y.Z-rc.N` and orders nothing else, each number written as cargo writes \
                 it, so `01` is not `1`",
                escaped(text)
            )
        };
        let number = |part: &str| {
            part.parse::<u64>()
                .ok()
                .filter(|number| number.to_string() == part)
        };
        let (numbers, rc) = match text.split_once('-') {
            None => (text, None),
            Some((numbers, candidate)) => {
                let counted = candidate
                    .strip_prefix("rc.")
                    .and_then(number)
                    .filter(|counted| *counted > 0)
                    .ok_or_else(refused)?;
                (numbers, Some(counted))
            }
        };
        let parts: Vec<Option<u64>> = numbers.split('.').map(number).collect();
        let [Some(major), Some(minor), Some(patch)] = parts.as_slice() else {
            return Err(refused());
        };
        Ok(Self {
            core: [*major, *minor, *patch],
            rc,
        })
    }

    /// The tag that names this release.
    fn tag(self) -> String {
        format!("v{self}")
    }
}

impl core::fmt::Display for Version {
    /// The version as a tag and a manifest write it, without the tag's `v`.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let [major, minor, patch] = self.core;
        match self.rc {
            None => formatter.write_fmt(format_args!("{major}.{minor}.{patch}")),
            Some(rc) => formatter.write_fmt(format_args!("{major}.{minor}.{patch}-rc.{rc}")),
        }
    }
}

/// Versions in the order this release knows them.
///
/// **The numbers first, compared as numbers, and then a candidate below the
/// release it is a candidate for**, and below a later candidate of the same
/// numbers: `0.9.0` < `0.10.0-rc.1` < `0.10.0-rc.2` < `0.10.0`.
impl Ord for Version {
    /// How `self` stands to `other`.
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.core.cmp(&other.core).then(match (self.rc, other.rc) {
            (None, None) => core::cmp::Ordering::Equal,
            (Some(_), None) => core::cmp::Ordering::Less,
            (None, Some(_)) => core::cmp::Ordering::Greater,
            (Some(mine), Some(theirs)) => mine.cmp(&theirs),
        })
    }
}

impl PartialOrd for Version {
    /// The one order there is, [`Ord`]'s.
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A commit as the release reads it: what its subject says, and what its body
/// says a breaking change asks.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    /// Its hash.
    hash: String,
    /// Its type: one of the history step's.
    kind: String,
    /// Its scope, when the subject names one.
    scope: Option<String>,
    /// Whether it breaks something: a `!` after its type or scope, or
    /// a `BREAKING CHANGE:` footer.
    breaking: bool,
    /// What it does: the subject after its `: `, GitHub's ` (#N)` included.
    description: String,
    /// What its `BREAKING CHANGE:` footer says a person or a developer must do.
    note: Option<String>,
}

/// How far a change moves the version (`NOCTUA.md §22.3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Bump {
    /// Not at all: `docs`, `test`, `chore`, `ci`, `build`, `refactor`, `revert`.
    Nothing,
    /// A `fix` or a `perf`.
    Patch,
    /// A `feat`.
    Minor,
    /// A breaking change — a minor while the major is 0.
    Major,
}

/// A release tag, with the commit it names and whether `HEAD` reaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tag {
    /// The release it names.
    version: Version,
    /// The commit it names, through an annotated tag's object when it is one.
    commit: String,
    /// Whether `HEAD` reaches that commit.
    merged: bool,
}

/// A release the changelog has a section for: its version, and the commit it
/// is at.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Point {
    /// The release.
    version: Version,
    /// The commit its section is read up to.
    at: String,
}

/// One release's section of the changelog: the release, and every change it
/// carries, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Section {
    /// The release.
    version: Version,
    /// Every commit it is the first release to reach.
    changes: Vec<Change>,
}

/// The commit `HEAD` is, as the release reads it.
#[derive(Debug)]
struct Head {
    /// Its hash.
    hash: String,
    /// How many parents it has.
    parents: usize,
    /// Its message.
    message: String,
}

/// What a run of the release is: where, on which channel, how far, and with
/// what.
struct Releasing<'a> {
    /// The repository the release is computed from and written into.
    root: &'a Path,
    /// The branch it is made from.
    channel: Channel,
    /// How far it goes.
    mode: Mode,
    /// This process's environment, which git and cargo are handed less the
    /// variables of `gh`'s own ([`ghs_own`]), and git less its own too.
    inherited: &'a [(OsString, OsString)],
    /// Where it is published, under `--publish`.
    github: Option<&'a GitHub>,
}

/// Where a release is published, and the environment `gh` is started with.
#[derive(Debug)]
struct GitHub {
    /// Its owner and its name with a slash between, as `GITHUB_REPOSITORY`
    /// names it.
    repository: String,
    /// What `gh` is handed: [`GitHub::of`] says what it holds and why.
    handed: Vec<(OsString, OsString)>,
}

/// What GitHub holds of a release already.
#[derive(Debug, Default)]
struct OnGitHub {
    /// The commit the release's tag names, when the tag exists.
    tag: Option<String>,
    /// Whether the GitHub release for the tag exists.
    release: bool,
    /// The pull request that carries the release back, when one exists.
    carried: Option<CarriedBack>,
    /// Whether the branch below holds the release commit already: an earlier
    /// release's pull request, open when this one was made, merged it since.
    below: bool,
}

/// A pull request from the released branch into the one below.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CarriedBack {
    /// Its number.
    number: u64,
    /// Its node, which auto-merge is enabled on.
    id: String,
    /// Whether it is open, merged or closed.
    state: Carried,
    /// Whether auto-merge is on.
    auto_merge: bool,
    /// Whether GitHub would merge it now: its checks have passed and nothing
    /// blocks it, which is when GitHub refuses to turn auto-merge on.
    mergeable: bool,
    /// Whether GitHub says it conflicts with the branch below, which no merge
    /// on GitHub resolves: its head is the released branch, which takes nothing
    /// but pull requests and the release App ([`carried_back_by_hand`]).
    conflicting: bool,
}

/// Where a pull request that carries a release back stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Carried {
    /// Open: it carries whatever the released branch holds when it merges.
    Open,
    /// Merged: the branch below holds the release.
    Merged,
    /// Closed without merging: the release was not carried back.
    Closed,
}

impl GitHub {
    /// Where CI's release job publishes, from `inherited`: the
    /// repository `GITHUB_REPOSITORY` names and the token in `GH_TOKEN`.
    ///
    /// **The token reaches `gh` through its environment and nowhere else** —
    /// never an argument, which a process listing shows, never a line this
    /// prints, and never another child ([`ghs_own`]), though this process
    /// holds it in its own environment. **And it is the one `gh` can use**:
    /// every other variable of `gh`'s own is removed, so `GH_HOST` and
    /// `GH_REPO` cannot point it at another place, `GH_DEBUG` cannot put the
    /// traffic on the log, and `GITHUB_TOKEN` — the workflow's own token,
    /// which `gh` falls back to — cannot stand in for the App's: a pull
    /// request opened with it starts no checks, and auto-merge would wait on
    /// them for ever. The host is set to GitHub's.
    ///
    /// # Errors
    ///
    /// Returns which of the two is missing, or why the repository is not
    /// one.
    fn of(inherited: &[(OsString, OsString)]) -> Result<Self, String> {
        let named = |name: &str| {
            inherited
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        let repository = named("GITHUB_REPOSITORY")
            .and_then(|value| value.into_string().ok())
            .ok_or_else(|| {
                "--publish publishes to the repository GITHUB_REPOSITORY names, and it is not \
                 set. CI's release job sets it; a release is published from there and nowhere \
                 else"
                    .to_owned()
            })?;
        let well_formed = repository.split_once('/').is_some_and(|(owner, name)| {
            [owner, name].iter().all(|part| {
                !part.is_empty()
                    && part.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "-_.".contains(character)
                    })
            })
        });
        if !well_formed {
            return Err(format!(
                "GITHUB_REPOSITORY is `{}`, and a repository is its owner and its name with a slash \
                 between",
                escaped(&repository)
            ));
        }
        let token = named("GH_TOKEN")
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                "--publish publishes with the token in GH_TOKEN, and it is not set. CI's release \
                 job sets it to the release App's installation token; the workflow's own token \
                 is never used, because a pull request it opens starts no checks"
                    .to_owned()
            })?;
        let mut handed: Vec<(OsString, OsString)> = inherited
            .iter()
            .filter(|(key, _)| !ghs_own(key))
            .cloned()
            .collect();
        handed.extend([
            (OsString::from("GH_TOKEN"), token),
            (OsString::from("GH_HOST"), OsString::from("github.com")),
            (OsString::from("GH_PROMPT_DISABLED"), OsString::from("1")),
            (OsString::from("GH_NO_UPDATE_NOTIFIER"), OsString::from("1")),
        ]);
        Ok(Self { repository, handed })
    }
}

/// Whether `name` is a variable `gh` reads for itself: every `GH_` one, and
/// the two tokens it falls back to. **In any case**, because Windows reads a
/// variable's name without its case.
///
/// **No child but `gh` is handed one** — the gate, the cargo that relocks,
/// and the git the history is read through (`crate::history`) — so the App's
/// token `--publish` holds is handed to `gh` alone. Handed is not reached: a
/// process of the same user may read this one's starting environment, which is
/// why CI's release job runs no gate ([`the_gate_ran_in_the_job_before`]).
pub(crate) fn ghs_own(name: &OsStr) -> bool {
    let name = name.to_string_lossy().to_ascii_uppercase();
    name.starts_with("GH_") || name == "GITHUB_TOKEN" || name == "GITHUB_ENTERPRISE_TOKEN"
}

/// Every variable of `inherited` but `gh`'s own ([`ghs_own`]), as a child
/// that is not `gh` is handed them.
fn without_ghs_own(
    inherited: &[(OsString, OsString)],
) -> impl Iterator<Item = (&OsString, &OsString)> {
    inherited
        .iter()
        .filter(|(name, _)| !ghs_own(name))
        .map(|(name, value)| (name, value))
}

/// Computes the channel's release, gates, writes and reads it back, and
/// publishes it under `--publish`: [`run_at`], with this tool as the gate and
/// this process's environment as the one the release was handed.
///
/// **The one place that asks where this tool is**, so a test can drive
/// [`run_at`] whole with a program of its own as the gate, and with an
/// environment of its own for [`the_gate_ran_in_the_job_before`] to read.
///
/// # Errors
///
/// Returns why this tool could not find itself, or what [`run_at`] refused.
pub(crate) fn run(
    root: &Path,
    channel: Channel,
    dry_run: bool,
    publish: bool,
    after_the_gate_job: bool,
) -> Result<(), String> {
    let itself = std::env::current_exe().map_err(|error| {
        format!("this tool could not find itself to run the gate: {error}. Nothing was written")
    })?;
    let inherited: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    run_at(
        &itself,
        root,
        channel,
        dry_run,
        publish,
        after_the_gate_job,
        &inherited,
    )
}

/// Refuses [`AFTER_THE_GATE_JOB`] unless this run publishes, as the release
/// job of [`THE_RELEASE_WORKFLOW`] on `channel`'s branch, in GitHub Actions.
///
/// **The gate stands aside where the job before ran it, and nowhere else.**
/// `release.yml` runs the gate in a job of its own, which enters no
/// environment and holds no secret and no token, and the release job waits
/// for it (`needs: gate`), so a gate run there again would run every build
/// script, procedural macro and test of the tree beside the App's key, for
/// nothing. Anywhere else the flag releases a tree no gate judged: a
/// developer's machine, another workflow, another job, another branch. So it
/// is refused without `--publish`, and refused unless `inherited` says this is
/// that job: `GITHUB_ACTIONS` is `true`, `GITHUB_JOB` is [`THE_RELEASE_JOB`],
/// and `GITHUB_WORKFLOW_REF` names [`THE_RELEASE_WORKFLOW`] of the repository
/// `GITHUB_REPOSITORY` names, at the channel's branch.
///
/// **What it is not** is a boundary against a person who sets those variables
/// by hand: on their own machine they decide whether their own gate runs, and
/// with no token of the App's `--publish` has nothing to publish with. It stops
/// the flag passed by mistake, or copied somewhere no gate runs first; the
/// gate refuses it in every file CI reaches but `release.yml`, and holds that
/// file to the two jobs.
///
/// # Errors
///
/// Returns why this run is not the release job of `release.yml` on the
/// channel's branch, under `--publish`.
fn the_gate_ran_in_the_job_before(
    channel: Channel,
    publish: bool,
    inherited: &[(OsString, OsString)],
) -> Result<(), String> {
    if !publish {
        return Err(format!(
            "{AFTER_THE_GATE_JOB} stands the gate aside because the job before ran it, and it \
             is passed with --publish alone, by the release job of {THE_RELEASE_WORKFLOW}. \
             Nothing was written"
        ));
    }
    let named = |name: &str| {
        inherited
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.to_string_lossy().into_owned())
    };
    let repository = named("GITHUB_REPOSITORY").unwrap_or_default();
    let workflow = named("GITHUB_WORKFLOW_REF").unwrap_or_default();
    let in_this_workflow = workflow
        .strip_suffix(&format!(
            "/{THE_RELEASE_WORKFLOW}@refs/heads/{}",
            channel.branch()
        ))
        .is_some_and(|named| !named.is_empty() && named.eq_ignore_ascii_case(&repository));
    let actions = named("GITHUB_ACTIONS");
    let job = named("GITHUB_JOB");
    if actions.as_deref() == Some("true")
        && job.as_deref() == Some(THE_RELEASE_JOB)
        && in_this_workflow
    {
        return Ok(());
    }
    let said = |value: Option<String>| {
        value.map_or_else(
            || "unset".to_owned(),
            |value| format!("`{}`", escaped(&value)),
        )
    };
    Err(format!(
        "{AFTER_THE_GATE_JOB} is passed by the release job of {THE_RELEASE_WORKFLOW} on {} \
         alone, where the gate ran in the job before it, and this run is not that job: \
         GITHUB_ACTIONS is {}, GITHUB_JOB is {}, and GITHUB_WORKFLOW_REF is {}. Nothing was \
         written",
        channel.branch(),
        said(actions),
        said(job),
        said((!workflow.is_empty()).then_some(workflow))
    ))
}

/// Runs the gate, this tool's `check`, in a child of its own started
/// from `itself` with every variable of `inherited` but `gh`'s, and refuses
/// unless it passed.
///
/// **A child cleared of `gh`'s variables, and that is all it is.** A run with
/// `--publish` that runs its own gate holds `GH_TOKEN` in this process's
/// environment, where every child it starts finds it unless told otherwise,
/// and nothing here can take a variable out of this process's own: that is
/// `unsafe` in this edition, which this workspace forbids. The gate starts
/// cargo, which runs every build script, procedural macro and test of the
/// workspace and of every dependency, and docker and git besides; none of them
/// is handed the token in its own environment. **But each can still read it**:
/// on Linux a process reads the starting environment of another of the same
/// user, this one's among them. So CI's release job runs no gate here: the
/// gate runs in a job of its own that holds no secret and no token, and the
/// release job passes [`AFTER_THE_GATE_JOB`]
/// ([`the_gate_ran_in_the_job_before`]).
///
/// **Its exit status is all the release reads of it**, so a status other
/// than success stops the release before anything is written: read as
/// passed, a gate that refused would let `--publish` commit, tag and release
/// a tree whose tests are red.
///
/// # Errors
///
/// Returns why the gate could not be started, or that it refused, as its
/// own lines above say.
fn the_gate(itself: &Path, root: &Path, inherited: &[(OsString, OsString)]) -> Result<(), String> {
    let ran = a_gate(itself, root, inherited)
        .status()
        .map_err(|error| format!("the gate could not be started: {error}. Nothing was written"))?;
    if ran.success() {
        Ok(())
    } else {
        Err("the gate refused, as its lines above say, so nothing was written".to_owned())
    }
}

/// The gate as [`the_gate`] starts it, from `itself`, over `root`.
///
/// **Cleared before it is handed anything**, so a variable this process holds
/// reaches it only through `inherited`, and `gh`'s do not reach it through
/// that. Its lines are this same tool's, each through its own `ui`, so they
/// are shown as it prints them; nothing reads its standard input.
fn a_gate(itself: &Path, root: &Path, inherited: &[(OsString, OsString)]) -> std::process::Command {
    #[expect(
        clippy::disallowed_methods,
        reason = "the gate is this same tool, and every line it prints goes through its own `ui`"
    )]
    let mut command = std::process::Command::new(itself);
    command
        .arg("check")
        .env_clear()
        .envs(without_ghs_own(inherited))
        .current_dir(root)
        .stdin(std::process::Stdio::null());
    command
}

/// Puts the lockfile's pristine bytes back after a release failed part way.
///
/// **It is the only thing that puts `Cargo.lock` back** after a failed relock,
/// and a count and a string literal cannot hold it: a restore writing
/// `&locked[..0]` would truncate the lockfile to nothing — after which
/// `--locked` refuses all five verbs.
///
/// A named function so the bytes can be asked about directly, which is
/// what `the_lockfile_comes_back_byte_for_byte` does.
fn put_the_lockfile_back(lockfile: &Path, locked: &[u8]) -> std::io::Result<()> {
    put_back_as(lockfile, locked)
}

/// Puts the changelog back as the release found it: the bytes it held, or
/// no file at all when the tree had none.
///
/// **Every failure of [`written`] after the version is written reaches it**,
/// through [`put_back`] in one arm, so no failure puts back less than another.
/// `std::fs::write` creates or truncates the file before it writes, so a write
/// that failed part way leaves it new, empty or partial, and a refusal that
/// puts the manifest back and not this says nothing was released over a
/// changelog the release changed.
///
/// A changelog the tree had none of, and the failed write never created, is
/// no file to remove, and is not a failure to put it back.
fn put_the_changelog_back(changelog: &Path, before: Option<&[u8]>) -> std::io::Result<()> {
    match before {
        Some(bytes) => put_back_as(changelog, bytes),
        None => match std::fs::remove_file(changelog) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            removed => removed,
        },
    }
}

/// Writes `bytes` into `path`, unless it already holds them.
///
/// **A file already holding what it held is left alone**, so one a refused
/// release never wrote — refused before its write, or by a write that never
/// opened it, as a read-only file's is — is not written, and is not named as
/// a file that did not go back.
fn put_back_as(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if std::fs::read(path).is_ok_and(|held| held == bytes) {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

/// Puts each file of `before` back as it held, and answers each that could
/// not be, with why.
fn put_back_listed(root: &Path, before: &[(&str, String)]) -> Vec<String> {
    before
        .iter()
        .filter_map(|(name, text)| {
            put_back_as(&root.join(name), text.as_bytes())
                .err()
                .map(|error| format!("{name} ({error})"))
        })
        .collect()
}

/// The release [`run`] makes, with the gate started from `itself` and the
/// environment the release was handed in `inherited`.
///
/// **A function of its own so a test can drive the verb whole**, with a
/// program that exits as the test says standing in for this tool: a gate
/// that refused stops the release before anything is written, and one that
/// passed lets it on to the write. **And with an environment of its own**, so
/// a test drives [`the_gate_ran_in_the_job_before`] as CI's release job and as
/// anything else, never through this process's.
///
/// # Errors
///
/// Returns why the gate may not stand aside, why the publishing setting is
/// incomplete, why the gate refused, or what [`released`] refused.
fn run_at(
    itself: &Path,
    root: &Path,
    channel: Channel,
    dry_run: bool,
    publish: bool,
    after_the_gate_job: bool,
    inherited: &[(OsString, OsString)],
) -> Result<(), String> {
    let mode = if dry_run {
        Mode::DryRun
    } else if publish {
        Mode::Publish
    } else {
        Mode::Write
    };
    // Asked before anything else, so a gate stood aside where none ran is
    // refused before a line of the release is printed.
    if after_the_gate_job {
        the_gate_ran_in_the_job_before(channel, publish, inherited)?;
    }
    // Asked before the gate, so a job that cannot publish says so in its first
    // second and not after the whole gate has run.
    let github = match mode {
        Mode::Publish => Some(GitHub::of(inherited)?),
        Mode::DryRun | Mode::Write => None,
    };
    ui::heading("release");

    // **The gate first, before anything is written.** A release that rewrote a
    // version and then discovered the tests were red would leave a tree, and a
    // release commit, nobody asked for. In CI's release job it ran first in a
    // job of its own, on this commit, which this one waits for.
    if dry_run {
        ui::warn("--dry-run: the gate is not run and nothing is written");
    } else if after_the_gate_job {
        ui::detail(
            "the gate ran in the job before this one, on this commit, and this job runs none \
             of its own",
        );
    } else {
        ui::detail("running the gate before anything is written");
        the_gate(itself, root, inherited)?;
    }
    released(&Releasing {
        root,
        channel,
        mode,
        inherited,
        github: github.as_ref(),
    })
}

/// The release `setting` asks for, once the gate has passed: computed, then
/// written and read back, then published.
///
/// # Errors
///
/// Returns why the history cannot be read, why the channel may not release,
/// why the version would not move forward, why what GitHub holds is not this
/// release's, or why a write, the read-back or a step on GitHub refused.
fn released(setting: &Releasing) -> Result<(), String> {
    let history = Own::of(setting.root, setting.inherited)?;
    let head = the_head(&history)?;
    let tags = the_tags(&history)?;
    let manifest = the_manifests_version(setting.root)?;

    // **A run on a release commit carries that release on**, and computes
    // nothing: the push the release commit causes, and a job run again after
    // one died part way, both land here.
    if let Some(version) = a_release_commit_of(&head) {
        return carried_on(setting, &history, &head, &tags, manifest, version);
    }

    // **A channel releases only once every release of the channel above is in
    // it**, or the commits since the last production release it reaches
    // include what that release already shipped.
    the_channel_above_is_merged(setting.channel, &tags)?;
    let last = the_last_production(&tags);
    let since = changes_between(
        &history,
        &head.hash,
        &last
            .iter()
            .map(|tag| tag.commit.as_str())
            .collect::<Vec<_>>(),
    )?;
    let Some(numbers) = the_next_numbers(last.map(|tag| tag.version), manifest, &since) else {
        ui::ok(&format!(
            "nothing to release: none of the {} commit(s) since {} is a feat, a fix, a perf or a \
             breaking change",
            since.len(),
            last.map_or_else(String::new, |tag| tag.version.tag())
        ));
        return Ok(());
    };
    let version = on_the_channel(setting.channel, numbers, &tags)?;
    goes_forward(version, &tags, manifest)?;
    ui::ok(&format!(
        "{} releases {}: {}",
        setting.channel.branch(),
        version.tag(),
        why_this_version(last.map(|tag| tag.version), &since)
    ));

    let sections = the_sections(
        &history,
        &tags,
        &Point {
            version,
            at: head.hash.clone(),
        },
    )?;
    let notes = the_notes_of(&sections, version)?;
    let changelog = the_changelog(&sections);
    said_what_it_carries(&notes)?;

    match (setting.mode, setting.github) {
        (Mode::DryRun, _) => {
            ui::warn(&format!(
                "nothing was written; {} is what it would be",
                version.tag()
            ));
            Ok(())
        }
        (Mode::Publish, Some(github)) => {
            the_tree_is_clean(&history)?;
            let holds = what_github_holds(github, setting.channel, version)?;
            what_github_holds_is_this_releases(&holds, setting.channel, version, None)?;
            let written = written(setting, version, &changelog)?;
            let files = &written.carried;
            // **A refusal from here on puts back what the run wrote**, as one
            // after the first write and before this does: the tree holds what
            // its `HEAD` holds again, whatever GitHub was left holding.
            the_release_commit(github, setting.channel, version, &head.hash, files)
                .and_then(|at| {
                    published(
                        github,
                        setting.channel,
                        version,
                        &at,
                        &notes,
                        &once_the_branch_moved(&holds),
                    )
                })
                .map_err(|problem| with_the_tree_put_back(setting.root, &written.found, &problem))
        }
        (Mode::Write | Mode::Publish, _) => {
            the_tree_is_clean(&history)?;
            written(setting, version, &changelog)?;
            ui::gap();
            ui::detail(
                "without --publish nothing leaves this machine. CI's release job publishes: the \
                 release commit, the tag, the GitHub release and the pull request that carries \
                 it back",
            );
            Ok(())
        }
    }
}

/// Carries on the release `HEAD` is the release commit of, making on GitHub
/// only what is missing.
///
/// **Held to what this verb would have written**: the manifest carries the
/// version, and its changelog is the one the history gives now, byte
/// for byte — so a commit titled as a release and holding anything else is
/// refused rather than published.
///
/// # Errors
///
/// Returns why `HEAD` is not the release commit it is titled as, why what
/// is on GitHub is not this release's, or what a step on GitHub refused.
fn carried_on(
    setting: &Releasing,
    history: &Own,
    head: &Head,
    tags: &[Tag],
    manifest: Version,
    version: Version,
) -> Result<(), String> {
    ui::ok(&format!(
        "HEAD is the release commit of {}, so the release is carried on and not made again",
        version.tag()
    ));
    if manifest != version {
        return Err(format!(
            "HEAD is titled as the release commit of {} and Cargo.toml carries {manifest}. A \
             release commit carries its own version, so this one was not written by this verb",
            version.tag()
        ));
    }
    let sections = the_sections(
        history,
        tags,
        &Point {
            version,
            at: head.hash.clone(),
        },
    )?;
    let notes = the_notes_of(&sections, version)?;
    let written = std::fs::read(setting.root.join(CHANGELOG)).map_err(|error| {
        format!("HEAD is a release commit and {CHANGELOG} could not be read: {error}")
    })?;
    if written != the_changelog(&sections).into_bytes() {
        return Err(format!(
            "HEAD is titled as the release commit of {} and its {CHANGELOG} is not the changelog \
             the history gives, so it was not written by this verb, or the history under it \
             changed. Nothing was published",
            version.tag()
        ));
    }
    said_what_it_carries(&notes)?;
    match (setting.mode, setting.github) {
        (Mode::DryRun, _) => {
            ui::warn(&format!(
                "nothing was written; {} is carried on from HEAD",
                version.tag()
            ));
            Ok(())
        }
        (Mode::Publish, Some(github)) => {
            the_tree_is_clean(history)?;
            let holds = what_github_holds(github, setting.channel, version)?;
            what_github_holds_is_this_releases(&holds, setting.channel, version, Some(&head.hash))?;
            // **Asked when no pull request carries it**, before anything is
            // made: an earlier release's, open when this one was made, may
            // have merged it below since, and GitHub refuses a pull request
            // between a branch and one already holding all of it.
            let below = holds.carried.is_none() && held_below(github, setting.channel, &head.hash)?;
            published(
                github,
                setting.channel,
                version,
                &head.hash,
                &notes,
                &OnGitHub { below, ..holds },
            )
        }
        (Mode::Write | Mode::Publish, _) => {
            ui::ok("nothing to write: HEAD already carries the release");
            Ok(())
        }
    }
}

/// Prints the release notes and the assets the release carries, and refuses
/// assets GitHub would refuse.
///
/// # Errors
///
/// Returns why the assets are outside [`GITHUBS_LIMITS`].
fn said_what_it_carries(notes: &str) -> Result<(), String> {
    ui::ok("its notes, which are its section of the changelog:");
    for line in notes.lines() {
        if line.is_empty() {
            ui::gap();
        } else {
            ui::detail(line);
        }
    }
    within_githubs_limits(&ASSETS)?;
    ui::ok(&format!(
        "{} asset(s), within GitHub's limits for one release: {MOST_ASSETS} at most, each under \
         {} GiB ({GITHUBS_LIMITS})",
        ASSETS.len(),
        LARGEST_ASSET >> 30
    ));
    for (name, size) in ASSETS {
        ui::detail(&format!("{name}: {size} byte(s)"));
    }
    Ok(())
}

/// Refuses a list of assets GitHub would refuse ([`GITHUBS_LIMITS`]): more
/// than [`MOST_ASSETS`], or one not under [`LARGEST_ASSET`].
///
/// # Errors
///
/// Returns which limit the list breaks.
fn within_githubs_limits(assets: &[(&str, u64)]) -> Result<(), String> {
    if assets.len() > MOST_ASSETS {
        return Err(format!(
            "the release carries {} asset(s), and GitHub takes {MOST_ASSETS} at most \
             ({GITHUBS_LIMITS}). Split them into fewer, larger chunk packs (`NOCTUA.md §17.8`)",
            assets.len()
        ));
    }
    if let Some((name, size)) = assets.iter().find(|(_, size)| *size >= LARGEST_ASSET) {
        return Err(format!(
            "the asset `{}` is {size} byte(s), and GitHub takes each under {LARGEST_ASSET} \
             ({GITHUBS_LIMITS}). Split it into chunk packs (`NOCTUA.md §17.8`)",
            escaped(name)
        ));
    }
    Ok(())
}

/// The version the manifest carries, read as one this release can order.
///
/// # Errors
///
/// Returns why the manifest names no version, or why the one it names
/// is not one.
fn the_manifests_version(root: &Path) -> Result<Version, String> {
    let current = read(root, CARRIES_THE_VERSION[0])?;
    let from = version_in(&current).ok_or_else(|| {
        format!(
            "{} names no version, and it is the file the others are checked against",
            CARRIES_THE_VERSION[0]
        )
    })?;
    Version::parse(&from).map_err(|why| {
        format!(
            "this tree is at `{}`, and {why}. A version is written by the release; set it back \
             by hand to the last one it wrote",
            escaped(&from)
        )
    })
}

/// The commit `HEAD` is.
///
/// # Errors
///
/// Returns why git could not name it, or why what it named is not
/// a commit.
fn the_head(history: &Own) -> Result<Head, String> {
    let named = history.asked(&["rev-parse", "--verify", "HEAD^{commit}"], None)?;
    let hash = String::from_utf8_lossy(&named).trim().to_owned();
    if !a_hash(&hash) {
        return Err(format!(
            "git named HEAD as `{}`, which is not a commit's hash",
            escaped(&hash)
        ));
    }
    let raw = history
        .commits(&[hash.as_str()])?
        .pop()
        .map(|(_, raw)| raw)
        .unwrap_or_default();
    let parents = headers_of(&raw)
        .split(|byte| *byte == b'\n')
        .filter(|line| line.starts_with(b"parent "))
        .count();
    let message = the_message_of(&hash, &raw)?;
    Ok(Head {
        hash,
        parents,
        message,
    })
}

/// Every release tag, with the commit it names and whether `HEAD` reaches it.
///
/// **A tag opening with `v` is a release tag, and one this release cannot
/// read is refused**, not skipped: only the release App may create such a tag
/// (`NOCTUA.md §22.1`), so one in another shape is something to look at before
/// anything is computed from the others.
///
/// # Errors
///
/// Returns why git could not list the tags, or which tag is not a release
/// this release can order.
fn the_tags(history: &Own) -> Result<Vec<Tag>, String> {
    let listed = history.asked(
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objecttype)%00%(objectname)%00%(*objecttype)%00%(*objectname)",
            "refs/tags/",
        ],
        None,
    )?;
    let merged = history.asked(
        &[
            "for-each-ref",
            "--merged=HEAD",
            "--format=%(refname)",
            "refs/tags/",
        ],
        None,
    )?;
    let merged = String::from_utf8_lossy(&merged);
    let merged: Vec<&str> = merged.lines().collect();
    let mut tags = Vec::new();
    for line in String::from_utf8_lossy(&listed).lines() {
        let fields: Vec<&str> = line.split('\0').collect();
        let [refname, kind, object, peeled_kind, peeled] = fields.as_slice() else {
            return Err(format!(
                "git described a tag as `{}`, which is not one tag",
                escaped(line)
            ));
        };
        let Some(name) = refname.strip_prefix("refs/tags/v") else {
            continue;
        };
        let version = Version::parse(name).map_err(|why| {
            format!(
                "the tag `{}` opens as a release tag does, and {why}",
                escaped(refname)
            )
        })?;
        let commit = match (*kind, *peeled_kind) {
            ("commit", "") => *object,
            ("tag", "commit") => *peeled,
            _ => {
                return Err(format!(
                    "the tag `{}` names a {} rather than a commit",
                    escaped(refname),
                    escaped(if peeled_kind.is_empty() {
                        kind
                    } else {
                        peeled_kind
                    })
                ));
            }
        };
        if !a_hash(commit) {
            return Err(format!(
                "git named the commit of `{}` as `{}`, which is not a hash",
                escaped(refname),
                escaped(commit)
            ));
        }
        tags.push(Tag {
            version,
            commit: commit.to_owned(),
            merged: merged.contains(refname),
        });
    }
    tags.sort_by_key(|tag| tag.version);
    Ok(tags)
}

/// The production release `HEAD` reaches last: the highest `vX.Y.Z` merged
/// into it.
fn the_last_production(tags: &[Tag]) -> Option<&Tag> {
    tags.iter()
        .filter(|tag| tag.merged && tag.version.rc.is_none())
        .max_by(|one, other| one.version.cmp(&other.version))
}

/// The version `HEAD` is the release commit of, if it is one: a commit with
/// one parent and a subject of [`RELEASE_COMMIT_OPENS`] and a version, and
/// nothing else.
///
/// **One parent, because a release commit is the App's**: on `staging` and
/// `main` every commit a person brings is a merge, and only the release App
/// may push to either directly (`NOCTUA.md §22.1`).
fn a_release_commit_of(head: &Head) -> Option<Version> {
    if head.parents != 1 {
        return None;
    }
    let mut lines = head.message.lines();
    let subject = lines.next()?;
    if lines.any(|line| !line.trim().is_empty()) {
        return None;
    }
    Version::parse(subject.strip_prefix(RELEASE_COMMIT_OPENS)?).ok()
}

/// Refuses a release on `channel` while a release of the channel above is not
/// merged into it: on `staging`, while the latest production tag is not one
/// `HEAD` reaches.
///
/// **Fail closed**: the back-merge from `main` has not landed, so the commits
/// since the last production release `HEAD` reaches include what a later one
/// already shipped, and the version computed from them would repeat it.
///
/// # Errors
///
/// Returns which release is not merged.
fn the_channel_above_is_merged(channel: Channel, tags: &[Tag]) -> Result<(), String> {
    if channel != Channel::Staging {
        return Ok(());
    }
    let latest = tags
        .iter()
        .filter(|tag| tag.version.rc.is_none())
        .max_by(|one, other| one.version.cmp(&other.version));
    match latest {
        Some(tag) if !tag.merged => Err(format!(
            "{} is the latest production release and staging does not reach it: the pull \
             request that carries it back from main has not landed. Staging releases once it \
             has, so its version is computed from everything main shipped",
            tag.version.tag()
        )),
        _ => Ok(()),
    }
}

/// How far one change moves the version (`NOCTUA.md §22.3`).
fn bump_of(change: &Change) -> Bump {
    if change.breaking {
        return Bump::Major;
    }
    match change.kind.as_str() {
        "feat" => Bump::Minor,
        "fix" | "perf" => Bump::Patch,
        _ => Bump::Nothing,
    }
}

/// `last` moved as far as `bump` asks, or `None` when it asks nothing.
///
/// **A major is a minor while the major is 0** (`NOCTUA.md §22.3`): before
/// 1.0.0 the minor is what says a release breaks something.
fn bumped(last: [u64; 3], bump: Bump) -> Option<[u64; 3]> {
    let [major, minor, patch] = last;
    match bump {
        Bump::Nothing => None,
        Bump::Patch => Some([major, minor, patch + 1]),
        Bump::Minor => Some([major, minor + 1, 0]),
        Bump::Major if major == 0 => Some([0, minor + 1, 0]),
        Bump::Major => Some([major + 1, 0, 0]),
    }
}

/// The numbers the next release carries, or `None` when there is nothing to
/// release.
///
/// With no production release, the first is the version the manifest carries,
/// unbumped: its numbers, a candidate's own number left off. After one, the
/// commits since it decide, the furthest-reaching of them winning.
fn the_next_numbers(
    last: Option<Version>,
    manifest: Version,
    since: &[Change],
) -> Option<[u64; 3]> {
    let Some(last) = last else {
        return Some(manifest.core);
    };
    let bump = since.iter().map(bump_of).max().unwrap_or(Bump::Nothing);
    bumped(last.core, bump)
}

/// The release `numbers` is on `channel`, given every release tag that
/// exists.
///
/// On `staging`, `X.Y.Z-rc.N`, with `N` one past the highest candidate of
/// those numbers that exists; on `main`, `X.Y.Z`.
///
/// # Errors
///
/// Returns why the release already exists: the production release of those
/// numbers is tagged.
fn on_the_channel(channel: Channel, numbers: [u64; 3], tags: &[Tag]) -> Result<Version, String> {
    let production = Version {
        core: numbers,
        rc: None,
    };
    if tags.iter().any(|tag| tag.version == production) {
        return Err(format!(
            "{} is already tagged, and a version is released once. The commits since the last \
             production release this branch reaches give it again, so that release is not \
             merged here",
            production.tag()
        ));
    }
    match channel {
        Channel::Main => Ok(production),
        Channel::Staging => {
            let highest = tags
                .iter()
                .filter(|tag| tag.version.core == numbers)
                .filter_map(|tag| tag.version.rc)
                .max()
                .unwrap_or(0);
            Ok(Version {
                core: numbers,
                rc: Some(highest + 1),
            })
        }
    }
}

/// Whether `to` is ahead of `from` in the order this release knows
/// ([`Version`]'s).
fn moves_forward(from: Version, to: Version) -> bool {
    to > from
}

/// Refuses `version` unless it is ahead of every release `HEAD` reaches, and
/// its numbers are not behind the ones the manifest carries.
///
/// **The manifest's numbers, not the manifest's version**: the first release
/// candidate of the numbers a manifest carries — `0.1.0-rc.1` over `0.1.0` —
/// is a release of numbers not yet released, and is ahead of every release
/// there is.
///
/// # Errors
///
/// Returns which release it would not be ahead of.
fn goes_forward(version: Version, tags: &[Tag], manifest: Version) -> Result<(), String> {
    if let Some(behind) = tags
        .iter()
        .filter(|tag| tag.merged && !moves_forward(tag.version, version))
        .max_by(|one, other| one.version.cmp(&other.version))
    {
        return Err(format!(
            "{} is not ahead of {}, which HEAD already reaches. A release goes forwards only: \
             a tag is not taken back once it is published",
            version.tag(),
            behind.version.tag()
        ));
    }
    if version.core < manifest.core {
        return Err(format!(
            "{} is behind the {manifest} Cargo.toml carries. A version is written by the \
             release and never by hand; set it back to the last one the release wrote",
            version.tag()
        ));
    }
    Ok(())
}

/// What a release is, in a sentence: why the history gives this version.
fn why_this_version(last: Option<Version>, since: &[Change]) -> String {
    let Some(last) = last else {
        return "no production release yet, so the first is the version Cargo.toml carries"
            .to_owned();
    };
    let reason = match since.iter().map(bump_of).max().unwrap_or(Bump::Nothing) {
        Bump::Major if last.core[0] == 0 => {
            "a breaking change, which is a minor while the version is 0.x"
        }
        Bump::Major => "a breaking change, which is a major",
        Bump::Minor => "a feat, which is a minor",
        Bump::Patch => "a fix or a perf, which is a patch",
        Bump::Nothing => "nothing that moves it",
    };
    format!(
        "{} commit(s) since {}, and the furthest-reaching of them is {reason}",
        since.len(),
        last.tag()
    )
}

/// The releases the changelog has a section for, in the order their sections
/// are read: the production releases `HEAD` reaches, oldest first, then the
/// candidates no production release has folded in yet, oldest first — `new`
/// among them.
///
/// **A candidate below the last production release is folded into it**: its
/// commits are in that release's section, read since the production release
/// before it, so a section per candidate exists only while its numbers are not
/// yet released.
fn the_points(tags: &[Tag], new: &Point) -> Vec<Point> {
    let mut production: Vec<Point> = Vec::new();
    let mut candidates: Vec<Point> = Vec::new();
    let reached = tags.iter().filter(|tag| tag.merged).map(|tag| Point {
        version: tag.version,
        at: tag.commit.clone(),
    });
    for point in reached.chain(std::iter::once(new.clone())) {
        let into = if point.version.rc.is_none() {
            &mut production
        } else {
            &mut candidates
        };
        if !into.iter().any(|held| held.version == point.version) {
            into.push(point);
        }
    }
    production.sort_by_key(|point| point.version);
    let latest = production.last().map(|point| point.version);
    candidates.retain(|point| latest.is_none_or(|latest| point.version > latest));
    candidates.sort_by_key(|point| point.version);
    production.extend(candidates);
    production
}

/// Every section of the changelog, newest first: each release with the
/// commits it is the first of [`the_points`] to reach.
///
/// # Errors
///
/// Returns why the history between two releases could not be read.
fn the_sections(history: &Own, tags: &[Tag], new: &Point) -> Result<Vec<Section>, String> {
    let points = the_points(tags, new);
    let mut sections = Vec::with_capacity(points.len());
    for (at, point) in points.iter().enumerate() {
        let before: Vec<&str> = points[..at]
            .iter()
            .map(|earlier| earlier.at.as_str())
            .collect();
        sections.push(Section {
            version: point.version,
            changes: changes_between(history, &point.at, &before)?,
        });
    }
    sections.reverse();
    Ok(sections)
}

/// The notes `version` is published with: its own section of the changelog,
/// without its heading.
///
/// # Errors
///
/// Returns why no section is the version's, which is a release the changelog
/// would not show.
fn the_notes_of(sections: &[Section], version: Version) -> Result<String, String> {
    sections
        .iter()
        .find(|section| section.version == version)
        .map(the_notes)
        .ok_or_else(|| {
            format!(
                "{} has no section of its own in the changelog, which is a release no reader of \
                 it would see",
                version.tag()
            )
        })
}

/// Every commit `tip` reaches and none of `before` does, newest first, as
/// the release reads each.
///
/// # Errors
///
/// Returns why git could not list them, or which commit's message the release
/// cannot read.
fn changes_between(history: &Own, tip: &str, before: &[&str]) -> Result<Vec<Change>, String> {
    let mut asked = vec![
        "rev-list".to_owned(),
        "--topo-order".to_owned(),
        tip.to_owned(),
    ];
    asked.extend(before.iter().map(|earlier| format!("^{earlier}")));
    if asked
        .iter()
        .skip(2)
        .any(|named| !a_hash(named.trim_start_matches('^')))
    {
        return Err(format!(
            "the history between releases is asked for by hashes, and was handed {asked:?}"
        ));
    }
    let asked: Vec<&str> = asked.iter().map(String::as_str).collect();
    let listed = String::from_utf8(history.asked(&asked, None)?)
        .map_err(|_| "git listed the commits in something that is not UTF-8".to_owned())?;
    let hashes: Vec<&str> = listed.lines().collect();
    history
        .commits(&hashes)?
        .iter()
        .map(|(hash, raw)| a_change(hash, raw))
        .collect()
}

/// A commit object's headers: everything above its first blank line.
fn headers_of(raw: &[u8]) -> &[u8] {
    raw.windows(2)
        .position(|pair| pair == b"\n\n")
        .map_or(raw, |at| &raw[..at])
}

/// A commit's message, as git stores it, read as UTF-8.
///
/// # Errors
///
/// Returns why the message is not UTF-8, which the history step
/// refuses too.
fn the_message_of(hash: &str, raw: &[u8]) -> Result<String, String> {
    let headers = headers_of(raw);
    let message = raw.get(headers.len() + 2..).unwrap_or_default();
    let declared = headers
        .split(|byte| *byte == b'\n')
        .find_map(|line| line.strip_prefix(b"encoding "));
    if declared.is_some_and(|declared| {
        !declared.eq_ignore_ascii_case(b"utf-8") && !declared.eq_ignore_ascii_case(b"utf8")
    }) {
        return Err(format!(
            "commit {} declares its message in another encoding than UTF-8, so the release \
             cannot read it",
            short(hash)
        ));
    }
    String::from_utf8(message.to_vec()).map_err(|_| {
        format!(
            "commit {}'s message is not UTF-8, so the release cannot read it",
            short(hash)
        )
    })
}

/// One commit, as the release reads it ([`Change`]).
///
/// **Its subject held to the history step's rule first**
/// (`crate::history::subject`), so a commit the gate would refuse is refused
/// here too, by its hash, rather than placed by a guess.
///
/// # Errors
///
/// Returns why its message cannot be read, or why its subject is not a
/// conventional one.
fn a_change(hash: &str, raw: &[u8]) -> Result<Change, String> {
    let message = the_message_of(hash, raw)?;
    let lines: Vec<&str> = message.split('\n').collect();
    let subject = lines.first().copied().unwrap_or_default();
    crate::history::subject(subject).map_err(|why| {
        format!(
            "commit {}'s subject is not a conventional one, so the release cannot say what it \
             is: {why}. The history step refuses it too; `cargo xtask check` names it",
            short(hash)
        )
    })?;
    let (head, description) = subject
        .split_once(':')
        .map(|(head, after)| (head, after.strip_prefix(' ').unwrap_or(after)))
        .unwrap_or_default();
    let (head, bang) = match head.strip_suffix('!') {
        Some(head) => (head, true),
        None => (head, false),
    };
    let (kind, scope) = match head.split_once('(') {
        Some((kind, scope)) => (kind, scope.strip_suffix(')').map(str::to_owned)),
        None => (head, None),
    };
    let (footer, note) = the_breaking_note(lines.get(1..).unwrap_or_default());
    Ok(Change {
        hash: hash.to_owned(),
        kind: kind.to_owned(),
        scope,
        breaking: bang || footer,
        description: description.to_owned(),
        note,
    })
}

/// Whether a commit's body has a `BREAKING CHANGE:` footer, and what it says
/// a person or a developer must do.
///
/// **The footer as the convention spells it**, `BREAKING CHANGE:` or
/// `BREAKING-CHANGE:`, in capitals, at the start of a line; its text runs on
/// over the lines below it until a blank one or another footer, joined into
/// one line, with every character a reader cannot see written as its escape.
fn the_breaking_note(body: &[&str]) -> (bool, Option<String>) {
    let Some(at) = body.iter().position(|line| {
        line.starts_with("BREAKING CHANGE:") || line.starts_with("BREAKING-CHANGE:")
    }) else {
        return (false, None);
    };
    let first = body[at].split_once(':').map_or("", |(_, rest)| rest.trim());
    let mut words: Vec<&str> = first.split_whitespace().collect();
    for line in &body[at + 1..] {
        if line.trim().is_empty() || a_footer(line) {
            break;
        }
        words.extend(line.split_whitespace());
    }
    let note = escaped(&words.join(" "));
    (true, (!note.is_empty()).then_some(note))
}

/// Whether a line opens a footer of its own: a token of letters and hyphens,
/// then `: ` or ` #`.
fn a_footer(line: &str) -> bool {
    let token: String = line
        .chars()
        .take_while(|character| character.is_ascii_alphabetic() || *character == '-')
        .collect();
    !token.is_empty() && {
        let rest = &line[token.len()..];
        rest.starts_with(": ") || rest.starts_with(" #")
    }
}

/// The part of its release's section a change is listed in, or `None` when the
/// changelog leaves it out (`NOCTUA.md §22.4`).
///
/// A breaking change is **Changed**, whatever its type, because it is what a
/// person or a developer must act on; then a `security` scope is
/// **Security**, whatever its type; then the type: `feat` **Added**, `fix`
/// **Fixed**, `perf` **Performance**, `revert` **Removed**. `docs`, `test`,
/// `chore`, `ci`, `build` and `refactor` are left out.
fn the_part_of(change: &Change) -> Option<Part> {
    if change.breaking {
        return Some(Part::Changed);
    }
    if change.scope.as_deref() == Some("security") {
        return Some(Part::Security);
    }
    match change.kind.as_str() {
        "feat" => Some(Part::Added),
        "fix" => Some(Part::Fixed),
        "perf" => Some(Part::Performance),
        "revert" => Some(Part::Removed),
        _ => None,
    }
}

/// One change as the changelog lists it: its scope, what it does and its
/// short hash, and under it what a breaking change asks.
fn the_entry(change: &Change) -> String {
    let scope = change
        .scope
        .as_ref()
        .map_or_else(String::new, |scope| format!("**{scope}:** "));
    let note = change
        .note
        .as_ref()
        .map_or_else(String::new, |note| format!("  What to do: {note}\n"));
    format!(
        "- {scope}{} ({})\n{note}",
        change.description,
        short(&change.hash)
    )
}

/// A release's notes: its section of the changelog without the heading, each
/// part that lists something under its own heading, in [`PARTS`]' order.
fn the_notes(section: &Section) -> String {
    let parts: Vec<String> = PARTS
        .iter()
        .filter_map(|part| {
            let listed: String = section
                .changes
                .iter()
                .filter(|change| the_part_of(change) == Some(*part))
                .map(the_entry)
                .collect();
            (!listed.is_empty()).then(|| format!("### {}\n\n{listed}", part.heading()))
        })
        .collect();
    if parts.is_empty() {
        format!("{NOTHING_TO_LIST}\n")
    } else {
        parts.join("\n")
    }
}

/// The whole changelog: [`OPENING`], then every section, each under its
/// release's heading, in the order given.
///
/// **A function of its sections and nothing else** — no date, no clock and no
/// machine — so the same history gives the same bytes.
fn the_changelog(sections: &[Section]) -> String {
    let below: Vec<String> = sections
        .iter()
        .map(|section| format!("\n## {}\n\n{}", section.version.tag(), the_notes(section)))
        .collect();
    format!("{OPENING}{}", below.concat())
}

/// Refuses a tree that holds what no commit does.
///
/// **The release commit is made from `HEAD` and the files this writes**, and
/// the lockfile from whatever manifests the tree holds, so a change sitting in
/// the tree — a member's manifest edited, a crate's directory added and not
/// committed — would be relocked into a commit that does not carry it.
///
/// # Errors
///
/// Returns what the tree holds, or why git could not say.
fn the_tree_is_clean(history: &Own) -> Result<(), String> {
    let status = history.asked(
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        None,
    )?;
    if status.is_empty() {
        return Ok(());
    }
    let named: Vec<String> = status
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .take(5)
        .map(|entry| escaped(&String::from_utf8_lossy(entry)))
        .collect();
    Err(format!(
        "the tree holds changes no commit has ({}), and a release is made from HEAD and the \
         files it writes. Commit them, or put them aside, and run the release again",
        named.join("; ")
    ))
}

/// What each file a release writes held before it wrote any of them, so a
/// release that fails part way can put each one back.
#[derive(Debug)]
struct Found {
    /// Each manifest [`CARRIES_THE_VERSION`] names, by name, as it was.
    manifests: Vec<(&'static str, String)>,
    /// The lockfile's bytes.
    locked: Vec<u8>,
    /// The changelog's bytes, or `None` when the tree had none.
    changelog: Option<Vec<u8>>,
}

/// What [`written`] answers: each file a release commit carries, and what
/// every file it wrote held before the first write.
#[derive(Debug)]
struct Written {
    /// Each file the release commit carries, by name, with its bytes as read
    /// back.
    carried: Vec<(String, Vec<u8>)>,
    /// What each file it wrote held before it wrote any of them.
    found: Found,
}

/// Writes the release into the tree — the version, then the changelog, then
/// the lockfile — reads every one back, and answers each file a release
/// commit carries, with its bytes as read back.
///
/// **Every file it writes is read before the first write**, so a failure has
/// something to put back: each listed manifest, the lockfile, and the
/// changelog or its absence.
///
/// **And every failure after the first write puts back what it wrote.** A
/// listed file that cannot be read, written or read back is put back by
/// [`rewrite_listed`]; whatever [`written_after_the_version`] refuses — the
/// changelog's write, the relock, the read-back, a file the commit carries
/// that cannot be read — is put back by one arm of this function, through
/// [`put_back`]. Each refusal names any file that did not go back.
///
/// **And every one is a file of this tree's**, checked before any is read
/// ([`each_is_a_file_of_this_trees`]).
///
/// It answers what it found too, so a refusal after it returns can put the
/// files back ([`with_the_tree_put_back`]).
///
/// # Errors
///
/// Returns why a file is not one of this tree's, why one could not be read,
/// written or rebuilt, or why one read back is not what was written, with what
/// did not go back.
fn written(setting: &Releasing, version: Version, changelog: &str) -> Result<Written, String> {
    let root = setting.root;
    let target = version.to_string();
    each_is_a_file_of_this_trees(root)?;
    let manifests: Vec<(&'static str, String)> = CARRIES_THE_VERSION
        .into_iter()
        .map(|name| read(root, name).map(|text| (name, text)))
        .collect::<Result<_, _>>()?;
    let from = version_in(&manifests[0].1).ok_or_else(|| {
        format!(
            "{} names no version, and it is the file the others are checked against",
            CARRIES_THE_VERSION[0]
        )
    })?;
    let locked = std::fs::read(root.join("Cargo.lock")).map_err(|error| {
        format!(
            "Cargo.lock could not be read, and a release rewrites it: {error}. Nothing was \
             written"
        )
    })?;
    // **Absent, or read: never taken as absent.** A changelog that is there
    // and cannot be read, read as none, is one a failure below removes, and
    // the release would delete a file it never read.
    let before = match std::fs::read(root.join(CHANGELOG)) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "{CHANGELOG} could not be read, and a release rewrites it: {error}. Nothing \
                 was written"
            ));
        }
    };
    let found = Found {
        manifests,
        locked,
        changelog: before,
    };

    let rewritten = rewrite_listed(root, &CARRIES_THE_VERSION, &target)?;
    match written_after_the_version(setting, version, changelog, &rewritten) {
        Ok(carried) => Ok(Written { carried, found }),
        // **Everything this wrote goes back, and the refusal says what did
        // not.** The manifest alone put back leaves `Cargo.lock` at the new
        // version — `cargo update` has already written it before failing — and
        // a message saying nothing was released over a lockfile that says
        // otherwise is a lie the next `clippy --locked` finds. One arm for every
        // refusal after the version is written, so what each puts back cannot
        // drift apart.
        Err(problem) => {
            let not_back = put_back(root, &found);
            let put_back = if not_back.is_empty() {
                format!(". Nothing was released and the version is back at {from}")
            } else {
                format!(
                    ". Nothing was released, and these could not be put back as they were: \
                     {}. The release began on a clean tree, so `git status` names what it left",
                    not_back.join("; ")
                )
            };
            Err(format!("{problem}{put_back}"))
        }
    }
}

/// Writes the changelog and the lockfile once the version is written, reads
/// every file back, and answers each file a release commit carries.
///
/// # Errors
///
/// Returns why the changelog could not be written, why the lockfile could not
/// be rebuilt, or why a file read back is not what was written.
fn written_after_the_version(
    setting: &Releasing,
    version: Version,
    changelog: &str,
    rewritten: &[&str],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let root = setting.root;
    let target = version.to_string();
    std::fs::write(root.join(CHANGELOG), changelog)
        .map_err(|error| format!("{CHANGELOG} could not be written: {error}"))?;
    ui::ok(&format!(
        // **What was written, not what was listed.** This printed
        // count is `rewritten.len()`, so a write that does nothing cannot
        // report a file.
        "{} file(s) carry {target}, and {CHANGELOG} is written whole",
        rewritten.len()
    ));
    for name in rewritten {
        ui::detail(name);
    }

    // **A line printed before the window it warns about.** Between the
    // manifest's write and `cargo update`'s return the two disagree, and
    // `.cargo/config.toml` puts `--locked` in the alias, so an interruption
    // there leaves every `cargo xtask` verb refusing with a message that names
    // neither the release nor the version. Nothing here handles a signal, and
    // a check afterwards is unreachable — in that state cargo will not start
    // this tool — so the recovery is this sentence and `AGENTS.md`'s.
    ui::detail(
        "rewriting Cargo.lock. If this is interrupted, the manifest and the lockfile \
         disagree and every `cargo xtask` verb refuses — the alias passes `--locked`. \
         `cargo update --workspace` puts the lockfile back",
    );
    relock(root, setting.inherited)?;
    read_back(root, version, changelog)?;
    ui::ok("Cargo.lock rebuilt, and every file read back as written");

    // **Every file the release wrote, each listed one among them**: a file
    // `CARRIES_THE_VERSION` names and the commit left out would be rewritten
    // here and published at the version before, which is the one file left
    // behind that list is written down to prevent.
    let mut carried: Vec<(String, Vec<u8>)> = Vec::new();
    for name in [CHANGELOG, "Cargo.lock"]
        .into_iter()
        .chain(CARRIES_THE_VERSION)
    {
        let bytes = std::fs::read(root.join(name))
            .map_err(|error| format!("{name} could not be read back: {error}"))?;
        carried.push((name.to_owned(), bytes));
    }
    carried.sort_by(|one, other| one.0.cmp(&other.0));
    Ok(carried)
}

/// Refuses, before the first write, a file the release writes that is not a
/// regular file of this tree's: each manifest [`CARRIES_THE_VERSION`] names,
/// the lockfile, and the changelog, which may be missing and is then created.
///
/// **A link is followed by every write and removed by a put-back.**
/// `std::fs::write` writes through a link to wherever it points, and creates
/// the file a link to nothing names; the changelog's put-back, when the tree
/// had none, removes the link itself; and under `--publish` the release commit
/// carries the bytes read through it as a file of its own. A directory, a pipe
/// or a device is no file a release can read and put back either. So each is
/// judged by what it is, not by what it points to (`symlink_metadata`), and
/// anything but a regular file stops the release with nothing written.
///
/// # Errors
///
/// Returns which file is not a regular file, and what it is; or why one could
/// not be judged.
fn each_is_a_file_of_this_trees(root: &Path) -> Result<(), String> {
    for name in CARRIES_THE_VERSION
        .into_iter()
        .chain(["Cargo.lock", CHANGELOG])
    {
        let found = match std::fs::symlink_metadata(root.join(name)) {
            Ok(found) => found.file_type(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && name == CHANGELOG => {
                continue;
            }
            Err(error) => {
                return Err(format!(
                    "{name} could not be read, and a release rewrites it: {error}. Nothing was \
                     written"
                ));
            }
        };
        if found.is_file() {
            continue;
        }
        // A link is told by the link it reads as, which no other kind of file
        // answers.
        let what = if std::fs::read_link(root.join(name)).is_ok() {
            "a link"
        } else if found.is_dir() {
            "a directory"
        } else {
            "neither a file nor a directory"
        };
        return Err(format!(
            "{name} is {what}, and a release writes it only as a regular file of this tree's: \
             a write follows a link to wherever it points, and creates the file a link to \
             nothing names. Nothing was written"
        ));
    }
    Ok(())
}

/// `problem`, a refusal at the release commit or after it, once every file
/// [`written`] wrote is put back as it `found` them: and what could not be.
///
/// **The same put-back as a refusal before the release commit**, so every
/// refusal of the verb after its first write leaves the tree as `HEAD` holds
/// it, whatever GitHub was left holding: a release commit made is carried on
/// by the run its push starts, and one refused leaves nothing on GitHub.
fn with_the_tree_put_back(root: &Path, found: &Found, problem: &str) -> String {
    let not_back = put_back(root, found);
    if not_back.is_empty() {
        format!("{problem}. Every file this run wrote is put back as HEAD holds it")
    } else {
        format!(
            "{problem}. And these could not be put back as HEAD holds them, so each holds what \
             this run wrote: {}. The release began on a clean tree, so `git status` names what \
             it left",
            not_back.join("; ")
        )
    }
}

/// Puts every file [`written`] writes back as it found them — each listed
/// manifest, the lockfile and the changelog — and answers each that could
/// not be, with why.
///
/// **What it found, whatever the failure was.** A failure before the relock
/// finds the lockfile as it was and leaves it alone; one after it finds the
/// lockfile `cargo update` wrote, and puts back the bytes read before the
/// first write.
fn put_back(root: &Path, found: &Found) -> Vec<String> {
    let lockfile = put_the_lockfile_back(&root.join("Cargo.lock"), &found.locked)
        .err()
        .map(|error| format!("Cargo.lock ({error})"));
    let changelog = put_the_changelog_back(&root.join(CHANGELOG), found.changelog.as_deref())
        .err()
        .map(|error| format!("{CHANGELOG} ({error})"));
    put_back_listed(root, &found.manifests)
        .into_iter()
        .chain(lockfile)
        .chain(changelog)
        .collect()
}

/// Reads the release back through readers that share nothing with the
/// writers: every manifest [`CARRIES_THE_VERSION`] names and the lockfile as
/// TOML, and the changelog as bytes.
///
/// **A parser, not the line the writer found.** The writer rewrites the line
/// `version_of` finds, and a read-back through `version_of` agrees with
/// itself whatever it found; `toml` reads the table cargo reads. The lockfile
/// is held to the version for every package of the workspace — every package
/// with no `source` — and to having at least one.
///
/// # Errors
///
/// Returns which file does not read back as written.
fn read_back(root: &Path, version: Version, changelog: &str) -> Result<(), String> {
    let target = version.to_string();
    for name in CARRIES_THE_VERSION {
        let manifest: toml::Table = toml::from_str(&read(root, name)?)
            .map_err(|error| format!("{name} does not read back as TOML: {error}"))?;
        let carries = manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("package"))
            .or_else(|| manifest.get("package"))
            .and_then(|package| package.get("version"))
            .and_then(toml::Value::as_str);
        if carries != Some(target.as_str()) {
            return Err(format!(
                "{name} reads back as carrying {carries:?}, and the release wrote {target}"
            ));
        }
    }
    let lockfile: toml::Table = toml::from_str(&read(root, "Cargo.lock")?)
        .map_err(|error| format!("Cargo.lock does not read back as TOML: {error}"))?;
    let members: Vec<&toml::Value> = lockfile
        .get("package")
        .and_then(toml::Value::as_array)
        .map(|packages| {
            packages
                .iter()
                .filter(|package| package.get("source").is_none())
                .collect()
        })
        .unwrap_or_default();
    let behind: Vec<&str> = members
        .iter()
        .filter(|package| {
            package.get("version").and_then(toml::Value::as_str) != Some(target.as_str())
        })
        .filter_map(|package| package.get("name").and_then(toml::Value::as_str))
        .collect();
    if members.is_empty() || !behind.is_empty() {
        return Err(format!(
            "Cargo.lock reads back with {} package(s) of this workspace, and these not at \
             {target}: {behind:?}",
            members.len()
        ));
    }
    let on_disk = std::fs::read(root.join(CHANGELOG))
        .map_err(|error| format!("{CHANGELOG} could not be read back: {error}"))?;
    if on_disk != changelog.as_bytes() {
        return Err(format!(
            "{CHANGELOG} reads back as other bytes than were written"
        ));
    }
    Ok(())
}

/// Refuses what GitHub holds of `version` that this run cannot build on,
/// before anything is written or made: `made` is the release commit `HEAD` is,
/// or `None` when this run is to make it.
///
/// **Judged whole, right after GitHub is asked**, on a run that makes the
/// release commit as on one carried on from it, so a refusal never leaves half
/// of a release behind — not even a release commit with no tag, which every
/// run after it on that branch would carry on into the same refusal. Before
/// the release commit is made, GitHub holds no tag and no release of it: one
/// there is a release this clone's tags are behind on. After it, the tag
/// names that commit, because a tag is never moved. And a pull request that
/// carried this release and was closed without merging is a refusal on either
/// side: the release opens it once, and a person closed it.
///
/// # Errors
///
/// Returns which of the three GitHub holds.
fn what_github_holds_is_this_releases(
    holds: &OnGitHub,
    channel: Channel,
    version: Version,
    made: Option<&str>,
) -> Result<(), String> {
    let tag = version.tag();
    match (made, holds.tag.as_deref()) {
        (None, tagged) if tagged.is_some() || holds.release => {
            return Err(format!(
                "{} is already tagged or released on GitHub, and HEAD is not its release \
                 commit. A version is released once: the tags this clone holds are behind \
                 GitHub's, so fetch them and run the release again",
                version.tag()
            ));
        }
        (Some(at), Some(tagged)) if tagged != at => {
            return Err(format!(
                "{tag} is tagged at {} on GitHub, and this release is at {}. A tag is never \
                 moved: look at both before releasing again",
                short(tagged),
                short(at)
            ));
        }
        _ => {}
    }
    if let Some(closed) = holds
        .carried
        .as_ref()
        .filter(|carried| carried.state == Carried::Closed)
    {
        return Err(format!(
            "the pull request that carried {tag} back to {}, #{}, was closed without merging. \
             Reopen it, or carry the release back by hand: the release makes it once",
            channel.below(),
            closed.number
        ));
    }
    Ok(())
}

/// What GitHub `holds`, once the release commit this run made has moved the
/// released branch, which is the head of the pull request back.
///
/// **An open pull request is queued, never merged now**: the merge state
/// GitHub gave was of the head before, and the checks of the commit just made
/// have not run. Into `staging`, whose rules the App alone may pass, a merge
/// asked now would land the release before any check of it ran, and whether it
/// conflicts is GitHub's to say again: auto-merge refused is what says it then.
/// **A merged one carried what the branch held before**, never the commit just
/// made, so the release opens its own. And no branch below holds a commit just
/// made.
fn once_the_branch_moved(holds: &OnGitHub) -> OnGitHub {
    let carried = holds
        .carried
        .as_ref()
        .filter(|carried| carried.state != Carried::Merged)
        .map(|carried| CarriedBack {
            mergeable: false,
            conflicting: false,
            ..carried.clone()
        });
    OnGitHub {
        tag: holds.tag.clone(),
        release: holds.release,
        carried,
        below: false,
    }
}

/// Makes on GitHub whatever of the release `holds` does not already hold, in
/// order — the tag at `at`, the GitHub release, the pull request that carries
/// it back, and auto-merge on that — and creates nothing twice.
///
/// **What it holds was judged before** by
/// [`what_github_holds_is_this_releases`]: a tag it holds names `at`, and the
/// pull request back is open or merged.
///
/// **A pull request whose checks have passed is merged, not queued**, on a run
/// that did not move its head: GitHub refuses auto-merge on one it would merge
/// now, and a run carried on after the checks of the one it opened had
/// finished would be refused at that step on every run after it, the release
/// never carried back. Its merge state is GitHub's when it was asked: checks
/// that finish between the question and the request are a refusal of this
/// run, and the next run merges. A run that made the release commit hands in
/// [`once_the_branch_moved`], whose pull request is never merged now.
///
/// **A release the branch below holds already is not carried back again**:
/// the pull request that merged it there was an earlier release's, and a
/// second is refused between a branch and another that holds all of it, so
/// that run, and every run after it, would fail over a release carried back
/// whole.
///
/// # Errors
///
/// Returns what a step on GitHub refused, or why its answer is not the one
/// asked for.
fn published(
    github: &GitHub,
    channel: Channel,
    version: Version,
    at: &str,
    notes: &str,
    holds: &OnGitHub,
) -> Result<(), String> {
    let tag = version.tag();
    let mut made = 0_usize;
    if holds.tag.is_none() {
        the_tag(github, version, at)?;
        made += 1;
    }
    if !holds.release {
        the_github_release(github, channel, version, notes)?;
        made += 1;
    }
    let carried = match &holds.carried {
        Some(carried) => Some(carried.clone()),
        None if holds.below => None,
        None => {
            made += 1;
            Some(the_pull_request_back(github, channel, version)?)
        }
    };
    if let Some(carried) = carried
        .as_ref()
        .filter(|carried| carried.state == Carried::Open)
    {
        if carried.conflicting {
            return Err(carried_back_by_hand(
                channel,
                version,
                carried.number,
                "GitHub says it conflicts with that branch",
            ));
        }
        if !carried.auto_merge {
            if carried.mergeable {
                merged_now(github, carried)?;
            } else {
                auto_merged(github, carried).map_err(|problem| {
                    carried_back_by_hand(channel, version, carried.number, &problem)
                })?;
            }
            made += 1;
        }
    }
    if made == 0 {
        ui::ok(&format!(
            "already released: {tag} is tagged, released, and carried back to {}",
            channel.below()
        ));
    } else if let Some(carried) = &carried {
        ui::ok(&format!(
            "{tag} is released, and pull request #{} carries it back to {}",
            carried.number,
            channel.below()
        ));
    } else {
        ui::ok(&format!(
            "{tag} is released, and {} holds it already",
            channel.below()
        ));
    }
    Ok(())
}

/// What GitHub holds of `version` on `channel`: its tag, its release, and the
/// pull request that carries it back.
///
/// **Only a pull request whose head is this repository's own branch is
/// one.** `headRefName` matches the branch of that name in every fork too:
/// a stranger's pull request from a fork's `staging`, taken as the one
/// carrying the release back, would have auto-merge turned on with the App's
/// token for code nobody reviewed, and one closed and titled as the release
/// would stop it. **An open pull request from this repository's branch to the
/// one below carries this release, whatever its title**: GitHub keeps one open
/// per pair of a repository's branches, and its head is the branch itself, so
/// a release made while an earlier one's is still open is carried by that
/// one. Otherwise the pull request is the one titled as this release's.
///
/// # Errors
///
/// Returns what `gh` refused, or why GitHub's answer is not the shape asked.
fn what_github_holds(
    github: &GitHub,
    channel: Channel,
    version: Version,
) -> Result<OnGitHub, String> {
    let (owner, name) = github.repository.split_once('/').unwrap_or_default();
    let answer = gh(
        github,
        "graphql",
        &serde_json::json!({
            "query": WHAT_GITHUB_HOLDS,
            "variables": {
                "owner": owner,
                "name": name,
                "tag": version.tag(),
                "ref": format!("refs/tags/{}", version.tag()),
                "head": channel.branch(),
                "base": channel.below(),
            },
        }),
    )?;
    let repository = answer
        .pointer("/data/repository")
        .filter(|repository| repository.is_object())
        .ok_or_else(|| {
            format!(
                "GitHub answered no repository {} for the token the release holds",
                github.repository
            )
        })?;
    let tag = match repository.get("ref") {
        None | Some(serde_json::Value::Null) => None,
        Some(named) => Some(
            named
                .pointer("/target/oid")
                .and_then(serde_json::Value::as_str)
                .filter(|oid| a_hash(oid))
                .map(str::to_owned)
                .ok_or_else(|| format!("GitHub named the tag {} with no commit", version.tag()))?,
        ),
    };
    let release = repository
        .get("release")
        .is_some_and(|release| !release.is_null());
    let nodes = repository
        .pointer("/pullRequests/nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "GitHub answered no list of pull requests".to_owned())?;
    let title = carried_back_as(channel, version);
    let mut carried = None;
    for node in nodes {
        let read = a_carried_back(node)?;
        if !from_this_repository(node, read.number, &github.repository)? {
            continue;
        }
        if node
            .get("mergeStateStatus")
            .and_then(serde_json::Value::as_str)
            .is_none()
        {
            return Err(format!(
                "GitHub answered pull request #{} with no merge state, and whether it is merged \
                 now or queued rests on it",
                read.number
            ));
        }
        let titled = node.get("title").and_then(serde_json::Value::as_str) == Some(&title);
        if read.state == Carried::Open {
            carried = Some(read);
            break;
        }
        if titled && carried.is_none() {
            carried = Some(read);
        }
    }
    Ok(OnGitHub {
        tag,
        release,
        carried,
        below: false,
    })
}

/// A pull request as GitHub's answer describes it.
///
/// **Its node is `node_id` where GitHub answers through REST**, as it does the
/// pull request the release opens, whose `id` there is a number; and `id`
/// where it answers through GraphQL, as it does the question, which has no
/// `node_id`. Read the other way round, the number stands first, is no
/// node, and every release made fresh would be refused after its tag, its
/// release and its pull request were made.
///
/// # Errors
///
/// Returns which of its parts is not the shape GitHub
/// was asked for.
fn a_carried_back(node: &serde_json::Value) -> Result<CarriedBack, String> {
    let number = node
        .get("number")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "GitHub answered a pull request with no number".to_owned())?;
    let id = node
        .get("node_id")
        .and_then(serde_json::Value::as_str)
        .or_else(|| node.get("id").and_then(serde_json::Value::as_str))
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("GitHub answered pull request #{number} with no node"))?
        .to_owned();
    let state = match node
        .get("state")
        .and_then(serde_json::Value::as_str)
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("OPEN") => Carried::Open,
        Some("MERGED") => Carried::Merged,
        Some("CLOSED") => Carried::Closed,
        other => {
            return Err(format!(
                "GitHub answered pull request #{number} as {}, which is not open, merged or \
                 closed",
                escaped(other.unwrap_or("nothing"))
            ));
        }
    };
    let auto_merge = node
        .get("autoMergeRequest")
        .is_some_and(|request| !request.is_null());
    // **Ready to merge as GitHub's merge box reads it**, in the states `gh`
    // itself merges at once rather than queue: clean, or passing with hooks or
    // with checks no rule requires. An answer that names none — the one a pull
    // request just opened gives — is not.
    let mergeable = matches!(
        node.get("mergeStateStatus")
            .and_then(serde_json::Value::as_str),
        Some("CLEAN" | "HAS_HOOKS" | "UNSTABLE")
    );
    // **A conflict, in either shape GitHub says one**: `mergeable` false where
    // it answers through REST, as it does the pull request the release opens,
    // and the merge state `DIRTY` where it answers through GraphQL.
    let conflicting = node.get("mergeable") == Some(&serde_json::Value::Bool(false))
        || node
            .get("mergeStateStatus")
            .and_then(serde_json::Value::as_str)
            == Some("DIRTY");
    Ok(CarriedBack {
        number,
        id,
        state,
        auto_merge,
        mergeable,
        conflicting,
    })
}

/// Whether the pull request numbered `number`, as `node` describes it, has its
/// head in `repository`, this one, rather than in a fork.
///
/// **A fork's is passed over, whatever else it says**: its head repository is
/// gone once the fork is deleted, and a refusal there would let anyone stop
/// every release by opening one and deleting the fork. One of this
/// repository's own names it as its head, or the answer is refused.
///
/// # Errors
///
/// Returns why GitHub's answer does not say where the head is.
fn from_this_repository(
    node: &serde_json::Value,
    number: u64,
    repository: &str,
) -> Result<bool, String> {
    match node
        .get("isCrossRepository")
        .and_then(serde_json::Value::as_bool)
    {
        Some(true) => Ok(false),
        Some(false) => {
            let head = node
                .pointer("/headRepository/nameWithOwner")
                .and_then(serde_json::Value::as_str);
            // GitHub reads an owner and a name without their case.
            if head.is_some_and(|head| head.eq_ignore_ascii_case(repository)) {
                Ok(true)
            } else {
                Err(format!(
                    "GitHub answered pull request #{number} as one of this repository's own \
                     with its head in {}, and this repository is {repository}",
                    escaped(head.unwrap_or("no repository"))
                ))
            }
        }
        None => Err(format!(
            "GitHub answered pull request #{number} without saying whether its head is in a \
             fork, and a fork's never carries a release"
        )),
    }
}

/// Whether the branch below `channel` holds the commit `at` already: GitHub
/// compares the two, and `at` is that branch's head or behind it.
///
/// **Ahead or diverged, it does not**, and the release carries it back:
/// diverged is the branch below holding a commit of its own beside it, as it
/// does after any merge into it the released branch has not taken. Any other
/// answer is refused, since one read as held would leave a release nobody
/// carried back, ended as if it were.
///
/// # Errors
///
/// Returns what `gh` refused, or why GitHub's answer is not one of the four
/// a comparison is.
fn held_below(github: &GitHub, channel: Channel, at: &str) -> Result<bool, String> {
    let (owner, name) = github.repository.split_once('/').unwrap_or_default();
    let answer = gh(
        github,
        "graphql",
        &serde_json::json!({
            "query": WHAT_THE_BRANCH_BELOW_HOLDS,
            "variables": {
                "owner": owner,
                "name": name,
                "below": format!("refs/heads/{}", channel.below()),
                "at": at,
            },
        }),
    )?;
    match answer
        .pointer("/data/repository/ref/compare/status")
        .and_then(serde_json::Value::as_str)
    {
        Some("BEHIND" | "IDENTICAL") => Ok(true),
        Some("AHEAD" | "DIVERGED") => Ok(false),
        other => Err(format!(
            "GitHub did not say whether {} holds {}: it answered {}, and a comparison is \
             ahead, behind, diverged or identical",
            channel.below(),
            short(at),
            escaped(other.unwrap_or("nothing"))
        )),
    }
}

/// Makes the release commit on `channel`'s branch: `files` over `head`, titled
/// as the release, and answers its hash.
///
/// **`expectedHeadOid` is the commit the job checked out**, so a branch that
/// moved since fails the release rather than receiving a commit made from a
/// tree it no longer holds. GitHub signs the commit and marks it verified
/// (`NOCTUA.md §22.5`).
///
/// # Errors
///
/// Returns what `gh` refused — a branch that moved among it — or why GitHub's
/// answer names no commit.
fn the_release_commit(
    github: &GitHub,
    channel: Channel,
    version: Version,
    head: &str,
    files: &[(String, Vec<u8>)],
) -> Result<String, String> {
    let additions: Vec<serde_json::Value> = files
        .iter()
        .map(|(path, bytes)| serde_json::json!({ "path": path, "contents": base64(bytes) }))
        .collect();
    let answer = gh(
        github,
        "graphql",
        &serde_json::json!({
            "query": THE_RELEASE_COMMIT,
            "variables": {
                "input": {
                    "branch": {
                        "repositoryNameWithOwner": github.repository,
                        "branchName": channel.branch(),
                    },
                    "expectedHeadOid": head,
                    "message": { "headline": format!("{RELEASE_COMMIT_OPENS}{version}") },
                    "fileChanges": { "additions": additions },
                },
            },
        }),
    )?;
    let made = answer
        .pointer("/data/createCommitOnBranch/commit/oid")
        .and_then(serde_json::Value::as_str)
        .filter(|oid| a_hash(oid))
        .ok_or_else(|| "GitHub answered the release commit with no commit".to_owned())?
        .to_owned();
    ui::ok(&format!(
        "the release commit {} is on {}, over {}",
        short(&made),
        channel.branch(),
        short(head)
    ));
    Ok(made)
}

/// Tags `at` as `version`.
///
/// # Errors
///
/// Returns what `gh` refused, or why GitHub's answer is not that
/// tag.
fn the_tag(github: &GitHub, version: Version, at: &str) -> Result<(), String> {
    let named = format!("refs/tags/{}", version.tag());
    let answer = gh(
        github,
        &format!("repos/{}/git/refs", github.repository),
        &serde_json::json!({ "ref": named, "sha": at }),
    )?;
    if answer.get("ref").and_then(serde_json::Value::as_str) != Some(named.as_str())
        || answer
            .pointer("/object/sha")
            .and_then(serde_json::Value::as_str)
            != Some(at)
    {
        return Err(format!(
            "GitHub answered the tag {} with another tag, or another commit",
            version.tag()
        ));
    }
    ui::ok(&format!("tagged {} at {}", version.tag(), short(at)));
    Ok(())
}

/// The GitHub release for `version`'s tag, with `notes`: a pre-release
/// on `staging`, and the latest release on `main`.
///
/// # Errors
///
/// Returns what `gh` refused, or why GitHub's answer is another
/// release.
fn the_github_release(
    github: &GitHub,
    channel: Channel,
    version: Version,
    notes: &str,
) -> Result<(), String> {
    let tag = version.tag();
    let answer = gh(
        github,
        &format!("repos/{}/releases", github.repository),
        &serde_json::json!({
            "tag_name": tag,
            "name": tag,
            "body": notes,
            "draft": false,
            "prerelease": channel == Channel::Staging,
            "make_latest": if channel == Channel::Main { "true" } else { "false" },
        }),
    )?;
    if answer.get("tag_name").and_then(serde_json::Value::as_str) != Some(tag.as_str()) {
        return Err(format!(
            "GitHub answered the release of {tag} with another release"
        ));
    }
    ui::ok(&format!("released {tag} on GitHub"));
    Ok(())
}

/// What the pull request that carries `version` back from `channel` is titled.
fn carried_back_as(channel: Channel, version: Version) -> String {
    format!(
        "chore(release): carry {} back to {}",
        version.tag(),
        channel.below()
    )
}

/// What a release says when the pull request `number`, which carries
/// `version` back from `channel`'s branch, cannot be merged on GitHub, and
/// why: how a person carries it back.
///
/// **The released branch stays the pull request's head**, and it takes
/// nothing but pull requests and the release App, so a conflict cannot be
/// resolved on it, and GitHub's conflict editor cannot commit there. A branch
/// made from the one below can: it takes the released branch by a merge, keeps
/// the released version and changelog where the two disagree, and has its
/// lockfile rebuilt for that version; its own pull request then lands the
/// release, and GitHub marks this one merged once its head is reachable from
/// the branch below. The tag and the GitHub release are made already, because
/// they come before the pull request back.
fn carried_back_by_hand(channel: Channel, version: Version, number: u64, why: &str) -> String {
    let (branch, below) = (channel.branch(), channel.below());
    format!(
        "pull request #{number}, which carries {} back to {below}, cannot be merged on GitHub: \
         {why}. {} is tagged and released. Carry it back by hand: make a branch from {below}, \
         merge {branch} into it, keep {branch}'s version and {CHANGELOG} wherever the two \
         disagree, run `cargo update --workspace`, commit it signed, and open a pull request \
         into {below}. Once that merges, GitHub marks #{number} merged, because its head is \
         reachable from {below}",
        version.tag(),
        version.tag()
    )
}

/// Opens the pull request that carries `version` back from `channel`'s branch
/// to the one below.
///
/// **Opened with the App's token, so its checks run**: a pull request opened
/// with the workflow's own token starts none, and auto-merge would wait on
/// them for ever.
///
/// # Errors
///
/// Returns what `gh` refused, or why GitHub's answer is another pull request.
fn the_pull_request_back(
    github: &GitHub,
    channel: Channel,
    version: Version,
) -> Result<CarriedBack, String> {
    let title = carried_back_as(channel, version);
    let answer = gh(
        github,
        &format!("repos/{}/pulls", github.repository),
        &serde_json::json!({
            "title": title,
            "head": channel.branch(),
            "base": channel.below(),
            "body": format!(
                "The release commit of {} writes its version and {CHANGELOG}. This carries it \
                 back to {}, so the branch below holds what was released.",
                version.tag(),
                channel.below()
            ),
        }),
    )?;
    if answer.get("title").and_then(serde_json::Value::as_str) != Some(title.as_str()) {
        return Err(format!(
            "GitHub answered the pull request that carries {} back with another one",
            version.tag()
        ));
    }
    let opened = a_carried_back(&answer)?;
    ui::ok(&format!(
        "pull request #{} carries {} back to {}",
        opened.number,
        version.tag(),
        channel.below()
    ));
    Ok(opened)
}

/// Turns auto-merge on, by merge commit, for the pull request `carried`.
///
/// # Errors
///
/// Returns what `gh` refused, or why GitHub's answer is not
/// auto-merge on.
fn auto_merged(github: &GitHub, carried: &CarriedBack) -> Result<(), String> {
    let answer = gh(
        github,
        "graphql",
        &serde_json::json!({ "query": AUTO_MERGE, "variables": { "id": carried.id } }),
    )?;
    if answer
        .pointer("/data/enablePullRequestAutoMerge/pullRequest/autoMergeRequest/mergeMethod")
        .and_then(serde_json::Value::as_str)
        != Some("MERGE")
    {
        return Err(format!(
            "GitHub did not answer pull request #{} with auto-merge on, by merge commit",
            carried.number
        ));
    }
    ui::ok(&format!(
        "auto-merge is on for #{}, by merge commit",
        carried.number
    ));
    Ok(())
}

/// Merges the pull request `carried`, by merge commit, now.
///
/// **For one whose checks have passed already**, which GitHub merges at once
/// and will not queue: auto-merge on it is refused.
///
/// # Errors
///
/// Returns what `gh` refused, or why GitHub's answer is not the pull request
/// merged.
fn merged_now(github: &GitHub, carried: &CarriedBack) -> Result<(), String> {
    let answer = gh(
        github,
        "graphql",
        &serde_json::json!({ "query": MERGED_NOW, "variables": { "id": carried.id } }),
    )?;
    if answer
        .pointer("/data/mergePullRequest/pullRequest/merged")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(format!(
            "GitHub did not answer pull request #{} as merged",
            carried.number
        ));
    }
    ui::ok(&format!(
        "#{} is merged, by merge commit: its checks had passed already",
        carried.number
    ));
    Ok(())
}

/// Asks GitHub, through `gh api`, for `endpoint` with `body` as the request,
/// and answers GitHub's answer.
///
/// **Its output is read, never printed**, and what `gh` writes to standard
/// error is returned with every character a reader cannot see written as its
/// escape, a newline among them, so no line of GitHub's can open a line of
/// CI's log, where a runner obeys one that opens with `::`. The body goes on
/// standard input from a thread of its own, so a long answer filling the pipe
/// cannot wait on a body still being written.
///
/// # Errors
///
/// Returns why `gh` could not be run, what it refused, or why its answer is
/// not JSON or carries GitHub's errors.
fn gh(
    github: &GitHub,
    endpoint: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let input = serde_json::to_vec(body)
        .map_err(|error| format!("the request for {endpoint} is not JSON: {error}"))?;
    #[expect(
        clippy::disallowed_methods,
        reason = "the CLI the release publishes through; its output is read, not printed"
    )]
    let mut command = std::process::Command::new("gh");
    command
        .env_clear()
        .envs(github.handed.iter().map(|(name, value)| (name, value)))
        .args([
            "api",
            "--hostname",
            "github.com",
            "--method",
            "POST",
            endpoint,
            "--input",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        format!("gh could not be started, and a release is published through it: {error}")
    })?;
    let writer = child
        .stdin
        .take()
        .map(|mut stdin| std::thread::spawn(move || std::io::Write::write_all(&mut stdin, &input)));
    let output = child
        .wait_with_output()
        .map_err(|error| format!("gh stopped before it answered: {error}"))?;
    if let Some(writer) = writer {
        writer
            .join()
            .map_err(|_| "the thread writing gh's input panicked".to_owned())?
            .map_err(|error| format!("gh's input could not be written: {error}"))?;
    }
    if !output.status.success() {
        return Err(format!(
            "`gh api {endpoint}` refused: {}",
            escaped(String::from_utf8_lossy(&output.stderr).trim())
        ));
    }
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!("`gh api {endpoint}` answered something that is not JSON: {error}")
    })?;
    if let Some(errors) = answer.get("errors").filter(|errors| !errors.is_null()) {
        return Err(format!(
            "`gh api {endpoint}` answered with GitHub's errors: {}",
            escaped(&errors.to_string())
        ));
    }
    Ok(answer)
}

/// `bytes` in base64, the standard alphabet with padding, which is
/// what `createCommitOnBranch` reads a file's contents as.
///
/// Written here rather than taken: it is twenty lines, and the dependency it
/// would be is a commitment (`NOCTUA.md §2.3`).
fn base64(bytes: &[u8]) -> String {
    /// The sixty-four characters, in the order their values give them.
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let [first, second, third] = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let sextets = [
            first >> 2,
            ((first & 0b11) << 4) | (second >> 4),
            ((second & 0b1111) << 2) | (third >> 6),
            third & 0b11_1111,
        ];
        for (at, sextet) in sextets.into_iter().enumerate() {
            if at <= chunk.len() {
                encoded.push(char::from(ALPHABET[usize::from(sextet)]));
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

/// Whether `text` is a commit's hash: forty or sixty-four lower-case hex
/// digits.
fn a_hash(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
}

/// The part of a hash a changelog entry and a line of this verb show.
fn short(hash: &str) -> &str {
    hash.get(..SHORT).unwrap_or(hash)
}

/// `text` with every character a reader cannot see written as its
/// escape, `\u{…}`.
///
/// **Escaped, and not dropped**, so a line that quotes what a commit, a tag or
/// GitHub said reads as what it is, and cannot move the terminal or open a
/// line of CI's log.
fn escaped(text: &str) -> String {
    text.chars()
        .map(|character| {
            if nc_schema::unseen(character) {
                character.escape_unicode().to_string()
            } else {
                character.to_string()
            }
        })
        .collect()
}

fn read(root: &Path, name: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(name))
        .map_err(|error| format!("could not read {name}: {error}"))
}

/// Rebuilds the lockfile without touching the network beyond what resolution
/// needs.
///
/// **Cargo as `$CARGO` names it**, from the environment the release was
/// handed, so the cargo that built this tool is the cargo that relocks, and
/// with none of `gh`'s variables, which no child but `gh` is handed.
fn relock(root: &Path, inherited: &[(OsString, OsString)]) -> Result<(), String> {
    let program = inherited
        .iter()
        .find(|(name, _)| name == "CARGO")
        .map_or_else(|| OsString::from("cargo"), |(_, value)| value.clone());
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let output = std::process::Command::new(program)
        .args(["update", "--workspace"])
        .env_clear()
        .envs(without_ghs_own(inherited))
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run cargo update: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cargo update refused: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// The line in a manifest that holds it.
///
/// **Kept for the tests and the prose, not for the writer.** A writer that is
/// `replacen` over this one spelling, beside a reader — `version_of` — that
/// accepts arbitrary whitespace, makes a manifest cargo accepts readable and
/// unwritable. `written_as_the_reader_reads_it` asks the reader where the line
/// is; this is the canonical form the scanner and the fixtures are written in.
#[cfg(test)]
const KEY: &str = "version = \"";

/// The version a manifest declares, if it declares one.
fn version_in(text: &str) -> Option<String> {
    version_of(text).map(|(_, version)| version)
}

/// The workspace's version, and which line of the file it is on.
///
/// **Under `[workspace.package]`, not the first `version = "` in the file.**
/// Taking the first trimmed line with the key, whatever table it is in, lets a
/// dependency pinned above the workspace's own table win, and a release then
/// rewrites *that*:
///
/// ```text
/// [workspace.dependencies.house-crate]
/// version = "0.2.0"        <- rewritten
/// path = "crates/house"
///
/// [workspace.package]
/// version = "0.1.0"        <- untouched
/// ```
///
/// The release would then start from the dependency's requirement as the
/// version the tree is at, and corrupt it, while the workspace's own stays
/// where it was: a read-back through this same function would agree with
/// itself, which is why [`read_back`] reads the manifest through `toml`.
/// Today's manifest happens to put `[workspace.package]` first; the second
/// file added to `CARRIES_THE_VERSION` is the one that bites.
///
/// **And the value is read as TOML writes it.** `version="0.1.0"` with no
/// spaces and `version = "0.1.0" # keep` are both manifests cargo parses, and
/// a reader of one spelling answers `None` for both — so a manifest that
/// names a version is reported as `Cargo.toml names no version`.
fn version_of(text: &str) -> Option<(usize, String)> {
    let mut table = String::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            name.clone_into(&mut table);
            continue;
        }
        // The workspace's table, a single-crate manifest's own, or the top
        // level of a file that is nothing but a version. What is skipped is a
        // *named* table that is neither — `[workspace.dependencies.x]` is where
        // such a decoy lives.
        if !matches!(table.as_str(), "workspace.package" | "package" | "") {
            continue;
        }
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        // **TOML has two string forms, and this reads both.** `version =
        // '0.1.0'` is a *literal* string, which cargo parses like any other —
        // measured, `cargo metadata` on a scratch manifest answers
        // `"version":"0.1.0"` — and a reader of double quotes alone answers
        // `None` for it. The whole gate then goes red on a tree cargo builds,
        // with a message saying the manifest *holds no version*, and
        // `cargo xtask release` refuses a manifest naming the version it holds.
        // That is the refusal-of-something-legitimate direction, in the reader
        // the writer follows.
        //
        // This file records the same class for `version="0.1.0"` and
        // `version  = "0.1.0"`, each *a manifest cargo accepts*, and widening
        // only the spellings somebody has tried leaves the next one out. TOML has
        // exactly two string forms, so this is the closed set rather than a
        // third list.
        let (version, _) = ['"', '\''].into_iter().find_map(|quote| {
            value
                .strip_prefix(quote)
                .and_then(|rest| rest.split_once(quote))
        })?;
        return Some((index, version.to_owned()));
    }
    None
}

/// Rewrites the version in every listed file, and reads each one back.
///
/// Separated from [`written`] so the writing half can be driven against a
/// scratch tree without a history or a lockfile around it, which is where a
/// test reaches its refusals: the refusal on a listed file with no version and
/// the refusal when the files already disagree are unreachable while
/// [`CARRIES_THE_VERSION`] has one entry.
///
/// **Every refusal of [`rewrite_each`] puts back each file it had read**, in
/// one arm of this function, and ends saying so or naming the file that did
/// not go back.
///
/// # Errors
///
/// Returns why a listed file could not be read or names no version, why the
/// files already disagree, why one could not be written, or why a file does
/// not name the new version after being rewritten, with what did not go back.
fn rewrite_listed<'a>(
    root: &Path,
    files: &[&'a str],
    version: &str,
) -> Result<Vec<&'a str>, String> {
    let current = read(root, files[0])?;
    let from = version_in(&current).ok_or_else(|| {
        format!(
            "{} names no version, and it is the file the others are checked against",
            files[0]
        )
    })?;

    // **What each file held before this touched it**, so a refusal can put it
    // back. Writing and *then* validating, with `Err` and no restore, lets a
    // comment naming the current version above `[workspace.package]` take the
    // rewrite while the read-back sees the real version unchanged, and the
    // release stops after the gate passed with the comment already edited,
    // under a message that says to fix the file and never says the file has
    // already been changed. So whatever `rewrite_each` refuses — a file that
    // cannot be read, holds no version, disagrees, cannot be written or does
    // not read back — puts back each file it had read. The file whose write
    // refused is put back with the rest, because `std::fs::write` truncates
    // before it writes.
    let mut before: Vec<(&str, String)> = Vec::new();
    match rewrite_each(root, files, &from, version, &mut before) {
        Ok(rewritten) => Ok(rewritten),
        Err(problem) => {
            let not_back = put_back_listed(root, &before);
            let put_back = if not_back.is_empty() {
                ". Every file this run touched has been put back as it was".to_owned()
            } else {
                format!(
                    ". And these could not be put back as they were, so each holds what this \
                     run left in it: {}",
                    not_back.join("; ")
                )
            };
            Err(format!("{problem}{put_back}"))
        }
    }
}

/// Rewrites the version in each of `files`, from `from` to `version`, reads
/// each one back, and records in `before` what each held before it was
/// touched.
///
/// # Errors
///
/// Returns why a listed file could not be read or names no version, why the
/// files already disagree, why one could not be written, or why a file does
/// not name the new version after being rewritten.
fn rewrite_each<'a>(
    root: &Path,
    files: &[&'a str],
    from: &str,
    version: &str,
    before: &mut Vec<(&'a str, String)>,
) -> Result<Vec<&'a str>, String> {
    let mut rewritten = Vec::new();
    for name in files {
        let text = read(root, name)?;
        before.push((name, text.clone()));
        // Not "skip a file that does not have it". A file listed here and not
        // holding the version is either a list that has gone stale or a file
        // that lost its version, and both are things to look at now.
        if version_in(&text).is_none() {
            return Err(format!(
                "{name} is listed as carrying the version and holds none. Either it \
                 stopped carrying one — take it out of CARRIES_THE_VERSION — or the \
                 line was lost, and a release now would ship a tree that disagrees \
                 with itself"
            ));
        }
        // **Spelled as the reader spells it.** `version_of` accepts a version
        // line written with no spaces, which cargo accepts, and `KEY` does not —
        // so a `replacen` over `KEY` would never match, and the release would
        // refuse a manifest it had just read, naming the version it holds. The reader and
        // the writer have to agree about what a version line looks like.
        let updated = written_as_the_reader_reads_it(&text, from, version);
        if updated == text && from != version {
            return Err(format!(
                "{name} holds a version and not `{from}`, so the files already disagree"
            ));
        }
        // **And the file then says so, read back the way it was read before.**
        // `version_in` scans *trimmed lines* for the key, so it skips a comment;
        // `replacen` searches the *raw text* and rewrites the first occurrence
        // anywhere, comments included. One comment line above
        // `[workspace.package]` mentioning the current version is enough for a
        // `replacen` writer to report `1 file(s) carry` the release over a
        // manifest whose own version did not move — the file left behind that
        // this module's own header says it exists to prevent, reached through
        // the one file it lists. So the file is read back here, the way it was
        // read before, and one still naming another version is a refusal.
        if updated != text {
            std::fs::write(root.join(name), &updated)
                .map_err(|error| format!("could not write {name}: {error}"))?;
            rewritten.push(*name);
        }
        let after = read(root, name)?;
        if version_in(&after).as_deref() != Some(version) {
            return Err(format!(
                "{name} was rewritten and still names {}, not {version}. The reader \
                 and the writer are one rule — the version is rewritten on the line \
                 `version_of` found it on — so this is a file whose version sits \
                 somewhere that reader does not look: a table other than \
                 `[workspace.package]` or `[package]`, or a spelling it cannot parse",
                version_in(&after).unwrap_or_else(|| "no version".to_owned())
            ));
        }
    }
    Ok(rewritten)
}

/// The ways TOML writes a version key, for the scanner that looks for files
/// carrying one.
///
/// **The summary above is this list's own, and only reading can tell.** A
/// writer's verb phrase standing here is rendered by rustdoc as the description
/// of a `#[cfg(test)]` array of literals that rewrites nothing, and neither
/// doc-block census can see a summary moved *inside* one block: the block count
/// and the bare count are both conserved, and the summary paragraph is one
/// line, so the fused-summary rule never looks past it. That rule's own doc
/// says reading is the only way.
///
/// **And the scanner reads every one of them.** A scan of the tree for
/// `format!("{KEY}{at}\"")` alone — a single spelling — leaves a probe file
/// holding `version="0.1.0"` invisible, though a writer of four spellings would
/// rewrite it, which is the pairing "the file somebody adds tomorrow" is about:
///
/// ```text
/// version="0.1.0"    -> every_file_listed_as_carrying_the_version… ok
/// version = "0.1.0"  -> …are not in CARRIES_THE_VERSION: ["docs/probe.md"]
/// ```
///
/// **The writer does not read this.** Matching one of these eight literals is
/// narrower than `version_of` — the reader — so a writer doing that leaves
/// `version  = "0.1.0"` readable and unwritable, and the release would refuse a
/// manifest naming the version it had just read.
/// `written_as_the_reader_reads_it` asks `version_of` where the line is. This
/// is the *scanner*'s list, which looks for files that carry a version and are
/// not on the list, and a scanner may be broad.
///
/// **And both quote characters.** A scanner looking for a double quote alone
/// leaves a *new* file naming its version `version = '0.1.0'` invisible to the
/// pairing this list exists for — the blindness a double-quote reader has, in
/// the same rule seen from the other end.
#[cfg(test)]
const SPELLINGS: [&str; 8] = [
    "version = \"",
    "version=\"",
    "version =\"",
    "version= \"",
    "version = '",
    "version='",
    "version ='",
    "version= '",
];

/// The version rewritten on the line the *reader* found it on.
///
/// **The reader and the writer speak one language.** `version_of` trims
/// whitespace without bound, so `version  = "0.1.0"` — two spaces, which cargo
/// accepts and `cargo fmt` never touches, because it does not format TOML — is
/// an ordinary version to it; `SPELLINGS` is eight literals, none of them that
/// one. A writer matching those runs the whole gate, and then:
///
/// ```text
/// check passed, with 4 warn line(s) above
///   error Cargo.toml holds a version and not `0.1.0`, so the files already disagree
/// ```
///
/// `Cargo.toml` holds exactly `0.1.0` and is the only file on the list, so there
/// is nothing for it to disagree with. Asking `version_of` where the version is
/// and rewriting *that line* makes one rule out of two: whatever the reader can
/// read, the writer can write.
fn written_as_the_reader_reads_it(text: &str, from: &str, version: &str) -> String {
    let Some((line_number, found)) = version_of(text) else {
        return text.to_owned();
    };
    if found != from {
        return text.to_owned();
    }
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let Some(line) = lines.get_mut(line_number) else {
        return text.to_owned();
    };
    // **In the quotes the file uses.** `version_of` reads both TOML string
    // forms, and a rewrite looking for a double-quoted one only reads, matches
    // and then leaves untouched a manifest cargo accepts — `version = '0.1.0'`
    // — and `cargo xtask release` answers *the files already disagree* about a
    // list with one file in it. The rewrite keeps the
    // spelling it found, which is what
    // `the_rewrite_does_not_change_the_spelling` asks of it.
    let Some((at, quote)) = ['"', '\'']
        .into_iter()
        .find_map(|quote| Some((line.find(&format!("{quote}{from}{quote}"))?, quote)))
    else {
        return text.to_owned();
    };
    let needle = format!("{quote}{from}{quote}");
    line.replace_range(at..at + needle.len(), &format!("{quote}{version}{quote}"));
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        CARRIES_THE_VERSION, Change, Channel, GitHub, KEY, Mode, Own, Releasing, SPELLINGS, Tag,
        Version, version_in,
    };
    use crate::check::LeavesNothingBehind;
    use crate::history::ARepository;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// Every file under `directory`, with its bytes.
    ///
    /// **A comparison of named files cannot see a file appear.** Reading back
    /// the one entry in `CARRIES_THE_VERSION` and asserting it unchanged is true
    /// of every tree a stray write lands in — the write is somewhere else. A
    /// listing is what has the other shape: it holds what the run was not
    /// supposed to touch as well as what it was.
    ///
    /// The bytes and not the length, because a rewrite that keeps the size is
    /// the same defect and a length cannot see it. `crate::gates::walk` is the
    /// walk every gate here uses, so build output is already out of the set.
    fn every_file_under(directory: &Path) -> Vec<(String, Vec<u8>)> {
        let mut seen = Vec::new();
        crate::gates::walk(directory, &mut |path| {
            seen.push((
                crate::relative_to(directory, path),
                std::fs::read(path).unwrap_or_default(),
            ));
        })
        .expect("the tree is readable");
        seen.sort();
        seen
    }

    /// What two listings disagree about, named one file per line.
    ///
    /// Empty is the assertion; the sentences exist so a failure says which file
    /// and what happened to it rather than printing five megabytes of bytes.
    fn changed_between(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) -> Vec<String> {
        let mut said = Vec::new();
        for (name, bytes) in after {
            match before.iter().find(|(was, _)| was == name) {
                // Bound rather than pushed inline, each of them: the verdict
                // census in `check.rs` reads `.push(` and these are a test's own
                // labels, not sentences the gate can say.
                None => {
                    let appeared = format!("{name} appeared");
                    said.push(appeared);
                }
                Some((_, was)) if was != bytes => {
                    let rewritten = format!("{name} was rewritten");
                    said.push(rewritten);
                }
                Some(_) => {}
            }
        }
        for (name, _) in before {
            if !after.iter().any(|(now, _)| now == name) {
                let vanished = format!("{name} vanished");
                said.push(vanished);
            }
        }
        said.sort();
        said
    }

    /// `files` written into `directory`, and then the version in each
    /// rewritten to `version` through `rewrite_listed`.
    ///
    /// **`rewrite_listed` alone, and not the verb**: the refusals it holds — a
    /// listed file with no version, and files that already disagree — are
    /// out of the verb's reach while `CARRIES_THE_VERSION` has one entry. They
    /// are held here, against a scratch tree, which is what makes those
    /// branches real rather than documentation.
    fn rewrite(directory: &Path, files: &[(&str, &str)], version: &str) -> Result<(), String> {
        for (name, text) in files {
            let path = directory.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("writable");
            }
            std::fs::write(path, text).expect("writable");
        }
        // **The names this call was given, not `CARRIES_THE_VERSION`.** The
        // shipped list has one entry, so through it every case here that names
        // two scratch files writes both and then rewrites a third, and every
        // assertion about the first going back is true because nothing has
        // touched it. A refusal part-way through a list is exactly the case the
        // shipped list does not reach today.
        let named: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
        super::rewrite_listed(directory, &named, version).map(|_| ())
    }

    /// `rewrite_listed` rewrites every file it is handed, and reports each.
    ///
    /// **Held with the arguments `written` passes**, the list
    /// `CARRIES_THE_VERSION` and the version as it writes it, with each listed
    /// file required to carry the new version afterwards. The count `written`
    /// prints as `N file(s) carry <version>` is `rewritten.len()`, so it is
    /// held to the length of the list: a write that does nothing reports no
    /// file, and one that skips a file reports one fewer.
    #[test]
    fn the_write_half_writes() {
        let scratch = std::env::temp_dir().join(format!("nc-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        for name in CARRIES_THE_VERSION {
            std::fs::write(
                scratch.join(name),
                "[workspace.package]\nversion = \"0.1.0\"\n",
            )
            .expect("writable");
        }

        let rewritten = super::rewrite_listed(&scratch, &CARRIES_THE_VERSION, "0.2.0")
            .expect("every listed file carries the version it is moving from");

        assert_eq!(
            rewritten.len(),
            CARRIES_THE_VERSION.len(),
            "the write half reported {} file(s) for a list of {}, and `written` prints that \
             number as `N file(s) carry <version>`",
            rewritten.len(),
            CARRIES_THE_VERSION.len()
        );
        for name in CARRIES_THE_VERSION {
            assert_eq!(
                std::fs::read_to_string(scratch.join(name)).expect("readable"),
                "[workspace.package]\nversion = \"0.2.0\"\n",
                "{name} was listed as carrying the version and does not carry the new one"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_version_written_into_a_comment_is_not_a_release() {
        // **The reader and the writer are one rule.** `version_in` reads the
        // first *trimmed line* beginning `version = "`, so it skips a comment;
        // a `replacen` rewrites the first occurrence in the *raw text*, which a
        // comment can be. Such a writer reports a file rewritten while
        // `[workspace.package]` still says 0.1.0.
        let scratch = std::env::temp_dir().join(format!("nc-release-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");

        let decoy = "# the line below is the one this rewrites: version = \"0.1.0\"\n\
                     [workspace.package]\nversion = \"0.1.0\"\n";
        // **A release, not a refusal.** The writer rewrites the line
        // `version_of` found rather than the first occurrence in the raw text,
        // so the comment is not a candidate at all — a writer that rewrites the
        // comment leaves the real version behind, with only a read-back that
        // refuses *after* the write between that and a bad release. One rule
        // cannot disagree with itself.
        rewrite(&scratch, &[("Cargo.toml", decoy)], "0.2.0")
            .expect("the real version is on a line the reader found");
        let after = std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable");
        assert_eq!(
            after,
            "# the line below is the one this rewrites: version = \"0.1.0\"\n\
             [workspace.package]\nversion = \"0.2.0\"\n",
            "the comment is prose and the version is the version"
        );

        let good = "[workspace.package]\nversion = \"0.1.0\"\n";

        // **All three refusal paths, not one.** The repair went into the
        // read-back path; the two above it — a listed file holding no version,
        // and a file whose version is not the one being moved from — still
        // returned with the first file rewritten at the *new* version under a
        // message saying nothing was released.
        let holds_none = "[package]\nname = \"x\"\n";
        let refused = rewrite(
            &scratch,
            &[("first.toml", good), ("second.toml", holds_none)],
            "0.2.0",
        )
        .expect_err("a listed file that carries no version stops the release");
        assert!(refused.contains("holds none"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(scratch.join("first.toml")).expect("readable"),
            good,
            "a file was left rewritten when a later one carried no version"
        );

        let disagrees = "[workspace.package]\nversion = \"9.9.9\"\n";
        let refused = rewrite(
            &scratch,
            &[("first.toml", good), ("second.toml", disagrees)],
            "0.2.0",
        )
        .expect_err("files that already disagree stop the release");
        assert!(refused.contains("already disagree"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(scratch.join("first.toml")).expect("readable"),
            good,
            "a file was left rewritten when a later one already disagreed"
        );

        // **And the spelling the reader accepts, the writer writes.**
        // `version_of` reads a version line with no spaces around the `=` —
        // which cargo accepts — and `KEY` does not, so a rewrite matching `KEY`
        // would match nothing and the release would refuse a manifest it had
        // just read, naming the version it holds — with one entry listed.
        // Two spaces: what cargo accepts, what `cargo fmt` never touches
        // because it does not format TOML, and what `SPELLINGS`' eight literals
        // do not hold — so the whole gate runs and such a release then refuses
        // a manifest holding exactly the version it has just read.
        let spaced = "[workspace.package]\nversion  = \"0.1.0\"\n";
        rewrite(&scratch, &[("Cargo.toml", spaced)], "0.2.0")
            .expect("whatever the reader can read, the writer can write");
        assert_eq!(
            std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable"),
            "[workspace.package]\nversion  = \"0.2.0\"\n",
            "the rewrite changed the spacing as well as the number"
        );

        let tight = "[workspace.package]\nversion=\"0.1.0\"\n";
        rewrite(&scratch, &[("Cargo.toml", tight)], "0.2.0")
            .expect("a manifest cargo accepts is one this can release");
        assert_eq!(
            std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable"),
            "[workspace.package]\nversion=\"0.2.0\"\n",
            "the rewrite changed the spelling as well as the number"
        );

        // **And the other of TOML's two string forms**, which `version_of`
        // reads and the writer has to write: with a double-quoted needle
        // whatever the file uses, a manifest cargo accepts is read, matched and
        // left untouched — and `release` then answers *the files already
        // disagree* about a list with one file in it.
        let quoted = "[workspace.package]\nversion = '0.1.0'\n";
        rewrite(&scratch, &[("Cargo.toml", quoted)], "0.2.0")
            .expect("a manifest cargo accepts is one this can release");
        assert_eq!(
            std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable"),
            "[workspace.package]\nversion = '0.2.0'\n",
            "the rewrite changed the quotes as well as the number"
        );

        // The same, for a list of more than one: the first file is written, the
        // second refuses, and the first must go back.
        //
        // The decoy above is rewritable, so the file that refuses is one whose
        // only `version` sits in a table the reader skips — a dependency's pin,
        // which is the shape `version_of`'s own comment names as the decoy it
        // walks past.
        let unwritable = "[workspace.dependencies.serde]\nversion = \"0.1.0\"\n";
        let refused = rewrite(
            &scratch,
            &[("first.toml", good), ("second.toml", unwritable)],
            "0.2.0",
        )
        .expect_err("the second file has no version this rewrites");
        assert!(refused.contains("holds none"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(scratch.join("first.toml")).expect("readable"),
            good,
            "the first file was left rewritten when the second refused"
        );

        // And the ordinary file, so the refusal above is about the comment.
        let plain = "[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n";
        rewrite(&scratch, &[("Cargo.toml", plain)], "0.2.0").expect("an ordinary manifest");
        let after = std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable");
        assert_eq!(version_in(&after).as_deref(), Some("0.2.0"));

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A listed file that lost its version, or holds another, stops the
    /// release, and the files before it go back.
    ///
    /// **Driven with a second file**, because the branch is unreachable while
    /// `CARRIES_THE_VERSION` has one entry — `from` is read from the same file
    /// the loop re-reads — and undriven it is documentation rather than a gate.
    /// A list that has gone stale, or a file that lost its line, is a tree that
    /// would ship disagreeing with itself. **The manifest is not written back
    /// between the two calls**: each refusal puts it back itself, and each is
    /// asked to.
    #[test]
    fn a_listed_file_that_lost_its_version_stops_the_release() {
        let scratch = LeavesNothingBehind::new("release-two");
        let manifest = scratch.root.join("Cargo.toml");
        let held = "[workspace.package]\nversion = \"0.1.0\"\n";
        let put_back = "Every file this run touched has been put back as it was";
        std::fs::write(&manifest, held).expect("writable");
        std::fs::write(scratch.root.join("second.toml"), "edition = \"2024\"\n").expect("writable");

        let refused = super::rewrite_listed(&scratch.root, &["Cargo.toml", "second.toml"], "0.2.0")
            .expect_err("a listed file with no version stops the release");
        assert!(
            refused.contains("second.toml") && refused.contains("holds none"),
            "the refusal does not name the file that lost its version: {refused}"
        );
        assert!(refused.ends_with(put_back), "{refused}");
        assert_eq!(
            std::fs::read_to_string(&manifest).expect("readable"),
            held,
            "the manifest was left at the new version when the file after it held none"
        );

        // And a second file that holds a *different* version is the other half:
        // the tree already disagrees, and a release would paper over it.
        std::fs::write(scratch.root.join("second.toml"), "version = \"9.9.9\"\n")
            .expect("writable");
        let refused = super::rewrite_listed(&scratch.root, &["Cargo.toml", "second.toml"], "0.2.0")
            .expect_err("a tree that already disagrees stops the release");
        assert!(
            refused.contains("already disagree"),
            "the refusal does not say the files disagree: {refused}"
        );
        assert!(refused.ends_with(put_back), "{refused}");
        assert_eq!(
            std::fs::read_to_string(&manifest).expect("readable"),
            held,
            "the manifest was left at the new version when the file after it disagreed"
        );
    }

    /// A listed file that cannot be read, or cannot be written, stops the
    /// rewrite with every file before it put back.
    ///
    /// **Each refusal reached through the file's own state.** The second
    /// file a directory first, which no system reads as a file; then a file
    /// whose read-only bit refuses the write — on every system, to anyone
    /// but root, so the test asks first, and fails by name rather than
    /// passing when that bit refuses nothing.
    #[test]
    fn a_file_that_cannot_be_read_or_written_puts_back_the_files_before_it() {
        let scratch = LeavesNothingBehind::new("release-unwritable");
        let first = scratch.root.join("first.toml");
        let second = scratch.root.join("second.toml");
        let held = "[workspace.package]\nversion = \"0.1.0\"\n";
        let listed = ["first.toml", "second.toml"];
        std::fs::write(&first, held).expect("a writable scratch");

        std::fs::create_dir(&second).expect("a writable scratch");
        let refused = super::rewrite_listed(&scratch.root, &listed, "0.2.0")
            .expect_err("a listed file that cannot be read stops the rewrite");
        assert!(
            refused.contains("could not read second.toml")
                && refused.contains("Every file this run touched has been put back as it was"),
            "{refused}"
        );
        assert_eq!(
            std::fs::read_to_string(&first).expect("readable"),
            held,
            "the first file was left rewritten when the second could not be read"
        );
        std::fs::remove_dir(&second).expect("a writable scratch");

        std::fs::write(&second, held).expect("a writable scratch");
        let was = std::fs::metadata(&second)
            .expect("the file is there")
            .permissions();
        let mut read_only = was.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&second, read_only).expect("the bit is set");
        // The bytes it holds, so a write the bit lets through changes nothing.
        let the_bit_holds = std::fs::write(&second, held).is_err();
        let ran = the_bit_holds.then(|| super::rewrite_listed(&scratch.root, &listed, "0.2.0"));
        // Off again before anything can panic, so the scratch can be removed.
        std::fs::set_permissions(&second, was).expect("the bit comes off");
        assert!(
            the_bit_holds,
            "a read-only file took a write, so this run is root's, which writes past the bit: \
             the write half is held only when the tests run as an ordinary user"
        );
        let refused = ran
            .expect("asked above")
            .expect_err("a listed file that cannot be written stops the rewrite");
        assert!(
            refused.contains("could not write second.toml")
                && refused.contains("Every file this run touched has been put back as it was"),
            "{refused}"
        );
        assert_eq!(
            std::fs::read_to_string(&first).expect("readable"),
            held,
            "the first file was left rewritten when the second could not be written"
        );
        assert_eq!(
            std::fs::read_to_string(&second).expect("readable"),
            held,
            "the file whose write was refused does not hold what it held"
        );
    }

    /// A line with any leading visibility taken off it.
    ///
    /// **A re-export is an import.** Matched by `starts_with("use ")` alone,
    /// `pub(crate) use std::fs as disk;` is not an import at all — an ordinary
    /// line, `cargo fmt` and clippy clean — and every rule built on
    /// [`the_imports_of`]'s census would hold against nothing: `disk::write(…)`
    /// is `std::fs::write` to rustc and matches none of the spellings below,
    /// which is exactly the alias the paragraph above says the census exists to
    /// refuse.
    ///
    /// `pub`, `pub(crate)`, `pub(super)` and `pub(in path)` are all of them.
    /// Whitespace is required after the visibility, so an identifier that merely
    /// begins with those three letters is left alone.
    fn without_a_visibility(line: &str) -> &str {
        let Some(rest) = line.strip_prefix("pub") else {
            return line;
        };
        let rest = if let Some(inside) = rest.strip_prefix('(') {
            match inside.find(')') {
                Some(at) => &inside[at + 1..],
                None => return line,
            }
        } else {
            rest
        };
        if rest.starts_with(char::is_whitespace) {
            rest.trim_start()
        } else {
            line
        }
    }

    /// Every `use` statement in `shipped`, one per line and joined.
    ///
    /// Split out of [`the_gate_runs_before_the_first_write`], which crossed
    /// clippy's line limit with it inline. The whole statement, because
    /// `use std::fs\n    as disk;` is the same import and rustfmt is free to
    /// wrap a long one; a bound on how far it will look, because an unterminated
    /// `use` is a file that does not compile and not this rule's business.
    fn the_imports_of(shipped: &str) -> String {
        let lines: Vec<&str> = shipped.lines().collect();
        let mut imports = String::new();
        for (at, line) in lines.iter().enumerate() {
            let opens = without_a_visibility(line.trim());
            if !opens.starts_with("use ") {
                continue;
            }
            let mut statement = opens.to_owned();
            let mut next = at + 1;
            while !statement.ends_with(';') && next < lines.len() && next - at < 20 {
                statement.push(' ');
                statement.push_str(lines[next].trim());
                next += 1;
            }
            imports.push_str(&statement.split_whitespace().collect::<Vec<_>>().join(" "));
            imports.push('\n');
        }
        imports
    }

    /// Every function of this workspace the shipped half of this module calls.
    ///
    /// **A write can be a call into this workspace, and every rule beside this
    /// reads `std::fs`.** One line inside `run_at`, before the gate, rewrites
    /// every stale page under `docs/` *above* the line announcing that the
    /// gate is about to run:
    ///
    /// ```text
    /// if !dry_run { crate::docs::run(root)?; }
    /// ```
    ///
    /// Without this list, all five other guards are silent about it, and each
    /// for the same reason: the imports rule reads `use … as`,
    /// `nothing_binds_a_write` reads a `fs::` path that is named and not called,
    /// `puts_bytes_on_disk` is twenty literal spellings, and the `Command`
    /// census counts child processes. `crate::docs::run(root)` is none of
    /// those. The `!dry_run` guard keeps it off the path
    /// `a_dry_run_writes_nothing` drives, and without that guard that test
    /// does fire.
    ///
    /// So this is a name list, compared both directions the way
    /// `xtask/src/policy.rs`'s `CORE_POLICIES` is: a call into this workspace
    /// that is not written here fails, and a name written here that the module
    /// stopped calling fails too. The two rules this module reads with rather
    /// than writes again: a conventional subject, and a character nobody can
    /// see. The gate is not a call: it runs in a child of its own, this same
    /// tool's `check`, which the `Command` census counts.
    const MAY_REACH: [&str; 2] = ["crate::history::subject", "nc_schema::unseen"];

    /// Every `use` statement the shipped half is allowed to carry.
    ///
    /// The other half of [`MAY_REACH`]: the call census reads paths beginning
    /// `crate::`, and `use crate::docs;` followed by `docs::run(root)` is the
    /// same call spelled through an import. The alias rule beside this refuses
    /// `use … as`; this refuses the import itself, so both spellings of a new
    /// reach into this workspace are a line added here.
    const MAY_IMPORT: [&str; 4] = [
        "use std::ffi::{OsStr, OsString};",
        "use std::path::Path;",
        "use crate::history::Own;",
        "use crate::ui;",
    ];

    /// The three spellings of a path into this workspace, and what each one is.
    ///
    /// **`super::` is a reach a census of `crate::` cannot spell, and it is the
    /// same defect respelled.** This file is a child of the crate root — so
    /// `super::docs::run(root)?` on the `!dry_run` path *is* `crate::docs::run`
    /// to rustc, and it is the write the paragraph on [`MAY_REACH`] records
    /// verbatim. Measured against a census of the one needle `crate::`, that
    /// line inside `run` spelled that way leaves every test passing and
    /// `cargo xtask check` byte-identical to a clean run. The import half holds
    /// — `use super::docs;` is an import [`MAY_IMPORT`] does not name — and the
    /// call half sees nothing at all.
    ///
    /// `check.rs` already carries this list for its own census, as
    /// `THIS_MODULE`, which is one rule written twice with the second copy a
    /// spelling short.
    ///
    /// Each needle is paired with what it means in front of a path here: this
    /// module is `crate::release`, so `super::` resolves to the crate root and
    /// `self::` to this module. Both are rewritten to the spelling `MAY_REACH`
    /// is written in, or the list would have to name every way of saying one
    /// function.
    ///
    /// **And a sibling member is a reach with no prefix at all.** The three
    /// above are the ways to say "somewhere in *this crate*", and `MAY_REACH`'s
    /// own doc says "every function of this workspace". `xtask` depends on
    /// `nc-catalog`, `nc-catalog-compiler` and `nc-policy`, and a call into one
    /// of those needs no import and wears none of these prefixes. Measured, one
    /// line inside `run`:
    ///
    /// ```text
    /// let _ = nc_catalog_compiler::camel("probe");
    ///   ->  the suite passes, `cargo xtask check` byte-identical to a clean run
    /// ```
    ///
    /// No live write follows from it — those three crates hand back values and
    /// the writes are in `build.rs` — so without the member names the census
    /// would have a hole, though not a hole with a write behind it: the list
    /// says what this module reaches, and a whole class of reach could not be
    /// spelled.
    ///
    /// The names come from `cargo metadata`, through the one reader of the
    /// resolved graph this crate has, so a member added to the workspace is in
    /// this census the moment cargo builds it — a written-out list here would be
    /// the fourth hand copy of the member list in this repository. Cargo spells
    /// a package name with hyphens and rustc spells the crate with underscores,
    /// which is the one translation between them.
    ///
    /// Assembled, or the needles are the thing they look for.
    fn the_spellings_of_a_reach() -> Vec<(String, String)> {
        let mut spellings = vec![
            (["cra", "te::"].join(""), ["cra", "te::"].join("")),
            (["sup", "er::"].join(""), ["cra", "te::"].join("")),
            (["se", "lf::"].join(""), ["cra", "te::release::"].join("")),
        ];
        for member in crate::members::members(root()).expect("cargo lists this workspace") {
            let crate_name = format!("{}::", member.name.replace('-', "_"));
            spellings.push((crate_name.clone(), crate_name));
        }
        spellings
    }

    /// Every path into this workspace that the shipped half *calls*, in the one
    /// spelling [`MAY_REACH`] is written in.
    ///
    /// Read out of the blanked text, so a `crate::…` named in this module's
    /// prose — and there are several — is prose and not a call. A call is told
    /// from a citation by what follows the path, as
    /// [`the_fs_bindings_of`] tells a call from a binding: a call opens a
    /// parenthesis.
    fn the_workspace_calls_of(shipped: &str) -> Vec<String> {
        let spent = crate::gates::blanked(shipped);
        let mut called: Vec<String> = Vec::new();
        for (needle, instead) in the_spellings_of_a_reach() {
            let mut rest = spent.as_str();
            while let Some(at) = rest.find(&needle) {
                rest = &rest[at..];
                let path: String = rest
                    .chars()
                    .take_while(|character| {
                        character.is_alphanumeric() || *character == '_' || *character == ':'
                    })
                    .collect();
                rest = &rest[path.len()..];
                // What rustc resolves it to, so one function has one name here
                // however it was written.
                let resolved = match path.strip_prefix(&needle) {
                    Some(tail) => format!("{instead}{tail}"),
                    None => path,
                };
                if rest.starts_with('(') && !called.contains(&resolved) {
                    called.push(resolved);
                }
            }
        }
        called.sort();
        called
    }

    /// `shipped` with its `use` statements blanked, line for line.
    ///
    /// An import is a path into this workspace that opens no parenthesis, and
    /// so is a binding — which is the whole distinction
    /// [`the_workspace_bindings_of`] reads. The two are told apart the only way
    /// they can be: an import is a statement of its own, and this is the same
    /// line walk [`the_imports_of`] uses to collect them, spending the lines
    /// instead of keeping them.
    fn outside_the_imports(shipped: &str) -> String {
        let lines: Vec<&str> = shipped.lines().collect();
        let mut out = String::with_capacity(shipped.len());
        let mut at = 0_usize;
        while at < lines.len() {
            let opens = without_a_visibility(lines[at].trim());
            if !opens.starts_with("use ") {
                out.push_str(lines[at]);
                out.push('\n');
                at += 1;
                continue;
            }
            // A long import is wrapped by rustfmt, and every line of it is the
            // one statement.
            let began = at;
            let mut statement = opens.to_owned();
            while !statement.ends_with(';') && at + 1 < lines.len() && at - began < 20 {
                at += 1;
                statement.push(' ');
                statement.push_str(lines[at].trim());
            }
            for _ in began..=at {
                out.push('\n');
            }
            at += 1;
        }
        out
    }

    /// Every path into this workspace the shipped half *binds* rather than
    /// calls.
    ///
    /// **The other half of [`the_workspace_calls_of`].** That function tells a
    /// call from a binding "as [`the_fs_bindings_of`] does", and that sibling
    /// *reports* the bindings it finds, while a call census alone drops them at
    /// `if rest.starts_with('(')`. A function is a value in Rust, so the two
    /// lines
    ///
    /// ```text
    /// let go = crate::docs::run;
    /// go(root)?;
    /// ```
    ///
    /// are `crate::docs::run(root)` to rustc and match nothing `MAY_REACH` can
    /// refuse. Measured with the call census alone, that pair placed on the
    /// non-dry-run path leaves every test passing and `check passed`; the
    /// control — the same call written out — fails with
    /// the assertion naming `crate::docs::run`. `nothing_binds_a_write` records
    /// the identical defeat for `std::fs`, and `xtask/src/main.rs` a third for
    /// the print scan.
    ///
    /// Read outside the `use` statements, because an import is a path that
    /// opens no parenthesis too — and this module imports `crate::ui`, which
    /// [`MAY_IMPORT`] already decides.
    fn the_workspace_bindings_of(shipped: &str) -> Vec<String> {
        let spent = crate::gates::blanked(&outside_the_imports(shipped));
        let mut bound: Vec<String> = Vec::new();
        for (needle, instead) in the_spellings_of_a_reach() {
            let mut rest = spent.as_str();
            while let Some(at) = rest.find(&needle) {
                rest = &rest[at..];
                let path: String = rest
                    .chars()
                    .take_while(|character| {
                        character.is_alphanumeric() || *character == '_' || *character == ':'
                    })
                    .collect();
                rest = &rest[path.len()..];
                // A prefix with nothing after it names no function: `crate::` on
                // its own is how a path is written in a `mod` declaration, not a
                // value anybody can call later.
                if path.len() == needle.len() {
                    continue;
                }
                let resolved = match path.strip_prefix(&needle) {
                    Some(tail) => format!("{instead}{tail}"),
                    None => path,
                };
                if !rest.starts_with('(') && !bound.contains(&resolved) {
                    bound.push(resolved);
                }
            }
        }
        bound.sort();
        bound
    }

    /// The shipped half binds no function of this workspace to a name.
    ///
    /// The shape [`nothing_binds_a_write`] has, for the census beside it: a
    /// call is refused unless `MAY_REACH` names it, and a binding is refused
    /// outright, because the name it is bound to is a word the author chooses
    /// and no list of spellings can hold it.
    #[track_caller]
    fn nothing_binds_a_workspace_call(shipped: &str) {
        let bound = the_workspace_bindings_of(shipped);
        assert!(
            bound.is_empty(),
            "this module binds a function of this workspace to a name instead of calling \
             it, and the census beside this reads a call: `let go = crate::docs::run;` \
             makes `go(root)` a reach `MAY_REACH` cannot refuse. The bindings are: \
             {bound:?}"
        );
    }

    /// The shipped half reaches only the functions of this workspace it says it
    /// does, and imports only what it says it imports.
    ///
    /// Its own function because [`the_gate_runs_before_the_first_write`] is at
    /// clippy's line limit, which is why its neighbours are functions too.
    #[track_caller]
    fn nothing_else_in_this_workspace_is_reached(shipped: &str, imports: &str) {
        let called = the_workspace_calls_of(shipped);
        let mut written: Vec<String> = MAY_REACH.iter().map(|one| (*one).to_owned()).collect();
        written.sort();
        assert_eq!(
            called, written,
            "the shipped half of this module calls {called:?} inside this workspace and \
             `MAY_REACH` names {written:?}. A write can be a call — `crate::docs::run(root)` \
             rewrote six pages of this repository above the line announcing the gate had \
             not run, and every `std::fs` rule beside this was silent"
        );
        let carried: Vec<&str> = imports.lines().collect();
        assert_eq!(
            carried, MAY_IMPORT,
            "the shipped half of this module imports {carried:?} and `MAY_IMPORT` names \
             {MAY_IMPORT:?}. `use crate::docs;` then `docs::run(root)` is a reach into \
             this workspace the call census above cannot see, because the path it writes \
             does not begin with the crate"
        );
    }

    /// The two rules a list of `std::fs` spellings cannot state, together.
    ///
    /// One function because [`the_gate_runs_before_the_first_write`] is at
    /// clippy's line limit, which is why its neighbours are functions too.
    #[track_caller]
    fn nothing_renames_or_reaches_past_the_gate(shipped: &str, imports: &str) {
        // The rename written with `let`, which no import carries.
        nothing_binds_a_write(shipped);
        // And a write that is neither: a call into this workspace.
        nothing_else_in_this_workspace_is_reached(shipped, imports);
        // And the same rename on that census, which a call half alone misses.
        nothing_binds_a_workspace_call(shipped);
    }

    /// Every place the shipped half *binds* a `std::fs` function rather than
    /// calling it.
    ///
    /// **The rename the import rule cannot see.** `use std::fs as disk;` is
    /// refused above, and `let put = std::fs::write;` is the same rename written
    /// in a body: `put(root.join("probe.txt"), b"planted")` is `std::fs::write`
    /// to rustc and matches none of the spellings the census below holds.
    /// Measured without this rule, that pair placed after the `--dry-run` early
    /// return writes a file into the root of this repository with every test
    /// passing and `check passed`; the control — the same call spelled
    /// `std::fs::write(` — fails the count at `left: 4 right: 3`.
    /// `xtask/src/main.rs` records the identical defeat for the print scan, in
    /// its own header:
    /// `let handle = std::io::stdout;` and then `handle()`.
    ///
    /// A call is told from a binding by what follows the path: a call opens a
    /// parenthesis and a binding does not. That distinction is what keeps the
    /// eleven ordinary `std::fs::read_to_string(…)` calls in this module out of
    /// it.
    ///
    /// **And `let` is only one of the ways to hand a function on**, which is all
    /// the two needles `= std::fs::` and `= fs::` can see. A function is a value
    /// in Rust, and a value can be put in an array, in an `Option`, or passed
    /// straight to something that calls it, and none of those is an `=`.
    /// Measured against those two needles, one line on the `!dry_run` path:
    ///
    /// ```text
    /// let _ = [std::fs::write].map(|put| put(root.join("probe.txt"), b"planted"));
    ///   ->  cargo fmt clean, clippy exit 0, the suite passes, `cargo xtask check`
    ///       byte-identical to a clean run — and a 7-byte `probe.txt` in the
    ///       root of this repository
    /// ```
    ///
    /// `Some(std::fs::write).map(…)` and `f(std::fs::write)` are the same shape.
    /// So there is no `= ` prefix, and what is left is the test the sibling
    /// [`the_workspace_calls_of`] uses: the path is found wherever it is
    /// written, and what follows it says which of the two it is. One needle
    /// rather than two, because `std::fs::write` and `fs::write` differ only in
    /// a qualifier that is read backwards off the same match — two needles would
    /// report the qualified spelling twice.
    ///
    /// Read out of the blanked text, for the reason [`the_workspace_calls_of`]
    /// gives: this module's own prose names `std::fs::write` a dozen times, and
    /// a rule that could not tell a paragraph from a program would be
    /// unsatisfiable here.
    fn the_fs_bindings_of(shipped: &str) -> Vec<String> {
        let spent = crate::gates::blanked(shipped);
        let is_path = |character: char| character.is_alphanumeric() || character == '_';
        let mut bound = Vec::new();
        for (at, _) in spent.match_indices("fs::") {
            // The qualifier in front of it, so `std::fs::write` is one match and
            // not two, and so a field or a variable called `…fs` is not one.
            let head_at = spent[..at]
                .rfind(|character: char| !(is_path(character) || character == ':'))
                .map_or(0, |before| before + 1);
            let head = &spent[head_at..at];
            if !(head.is_empty() || head.ends_with("::")) {
                continue;
            }
            let rest = &spent[at + "fs::".len()..];
            let path: String = rest
                .chars()
                .take_while(|character| is_path(*character) || *character == ':')
                .collect();
            if path.is_empty() || rest[path.len()..].starts_with('(') {
                continue;
            }
            // Bound rather than pushed inline, for the reason its neighbours
            // give: the verdict census reads `.push(`.
            let spelled = format!("{head}fs::{path}");
            bound.push(spelled);
        }
        bound
    }

    /// The shipped half binds no `std::fs` function to a name.
    ///
    /// **A function is a value in Rust, so the import rule cannot be the whole
    /// of it.** `let put = std::fs::write;` binds the name in a body, where no
    /// `use` statement carries it — measured without this, that binding plus
    /// `let _ = put(root.join("probe"), b"planted");` after the `--dry-run`
    /// early return leaves every test passing and `check passed`, with a file
    /// written into the root of this repository. The control, the same call
    /// written out, fails the write count at `left: 4 right: 3`.
    ///
    /// Its own function because [`the_gate_runs_before_the_first_write`] is at
    /// clippy's line limit, which is why the import rule beside it is a
    /// function too.
    #[track_caller]
    fn nothing_binds_a_write(shipped: &str) {
        let bound = the_fs_bindings_of(shipped);
        assert!(
            bound.is_empty(),
            "this module binds a `std::fs` function to a name instead of calling it, and \
             every rule beside this is a list of spellings: `let put = std::fs::write;` \
             makes `put(…)` a write none of them can see. The bindings are: {bound:?}"
        );
    }

    /// A `std::fs` function bound to a name is read as a binding, and a call as
    /// a call.
    ///
    /// The floor under the refusal in [`the_gate_runs_before_the_first_write`]:
    /// a reader that saw a binding in every `std::fs::read_to_string(…)` would
    /// be unsatisfiable, and one that saw none would refuse nothing.
    #[test]
    fn a_std_fs_function_bound_to_a_name_is_read_as_a_binding() {
        for bound in [
            "let put = std::fs::write;\n",
            "let put = fs::write;\n",
            "let put: fn(&Path, &[u8]) = std::fs::write;\n",
            "let gone = std::fs::remove_dir_all;\n",
            "let make = std::fs::DirBuilder::new;\n",
            // **And the four spellings that are not an assignment.** A function
            // is a value, and a value goes into an array, into an `Option`, into
            // an argument and into a return position without an `=` anywhere
            // near it. The first of these, unread, writes a file into the root
            // of this repository with every rule in this module green.
            "let _ = [std::fs::write].map(|put| put(root.join(\"probe.txt\"), b\"x\"));\n",
            "let _ = Some(std::fs::write).map(|put| put(root, b\"x\"));\n",
            "hand_it_on(std::fs::write);\n",
            "    std::fs::write\n",
        ] {
            assert_eq!(
                the_fs_bindings_of(bound).len(),
                1,
                "`{bound}` binds a `std::fs` function to a name and was read as a call"
            );
        }
        for called in [
            "let text = std::fs::read_to_string(&path).expect(\"readable\");\n",
            "let _ = std::fs::write(root.join(name), text);\n",
            "let file = std::fs::File::create(&path);\n",
            "let text = fs::read_to_string(&path);\n",
            // A mention in this module's own prose, which the widened needle
            // would otherwise report on every paragraph that names the rule.
            "// `let put = std::fs::write;` binds it rather than calling it\n",
            "let hint = \"std::fs::write\";\n",
            // And a name that merely ends in those letters.
            "let path = self.prefs::default();\n",
        ] {
            assert!(
                the_fs_bindings_of(called).is_empty(),
                "`{called}` calls a `std::fs` function and was read as a binding"
            );
        }
    }

    /// A call into this workspace is read as one however it is spelled.
    ///
    /// The floor under [`nothing_else_in_this_workspace_is_reached`]: a census
    /// of `crate::` alone reads the same call written `super::` as no call at
    /// all — and this file is a child of the crate root, which makes those two
    /// spellings the same function. Both directions, because a census that
    /// answered every path would refuse this module's own prose.
    #[test]
    fn a_reach_into_this_workspace_is_read_however_it_is_spelled() {
        // Assembled, or this test's own text is a call site.
        let root = ["cra", "te::"].join("");
        let up = ["sup", "er::"].join("");
        let here = ["se", "lf::"].join("");
        let sibling = ["nc_catalog", "_compiler::"].join("");
        for (why, code, resolved) in [
            (
                "the crate root, written out",
                format!("{root}docs::run(root)?;\n"),
                format!("{root}docs::run"),
            ),
            (
                "the parent module, which is the crate root",
                format!("{up}docs::run(root)?;\n"),
                format!("{root}docs::run"),
            ),
            (
                "this module, the long way",
                format!("{here}validate(version)?;\n"),
                format!("{root}release::validate"),
            ),
            (
                "guarded, on the path no test enters",
                format!("if !dry_run {{ {up}docs::run(root)?; }}\n"),
                format!("{root}docs::run"),
            ),
            // **A sibling member, which wears none of the three prefixes.**
            // `xtask` depends on three crates of this workspace and a call into
            // one needs no import at all, so without this `MAY_REACH` — whose own
            // doc says "every function of this workspace" — cannot spell a whole
            // class of reach. Assembled, like its neighbours.
            (
                "a sibling member of this workspace",
                format!("let _ = {sibling}camel(\"probe\");\n"),
                format!("{sibling}camel"),
            ),
        ] {
            assert_eq!(
                the_workspace_calls_of(&code),
                vec![resolved.clone()],
                "`{code}` reaches `{resolved}` inside this workspace ({why}) and the \
                 census walks past it, so `MAY_REACH` names nothing it could refuse"
            );
        }
        for (why, code) in [
            (
                "a citation rather than a call",
                format!("// see {root}check::run\n"),
            ),
            (
                "an import, which the other half reads",
                format!("use {up}docs;\n"),
            ),
            (
                "a path that opens no parenthesis",
                format!("let it = {root}check::run;\n"),
            ),
            (
                "ordinary code",
                "let root = repository_root();\n".to_owned(),
            ),
            (
                "a sibling member named in prose rather than called",
                format!("// {sibling}camel is the name it writes\n"),
            ),
            (
                "an import of a sibling member, which opens no parenthesis",
                format!("use {sibling}model::Catalog;\n"),
            ),
        ] {
            assert!(
                the_workspace_calls_of(&code).is_empty(),
                "`{code}` is not a call into this workspace ({why}) and the census \
                 reports one, so the rule fires on this module's own prose"
            );
        }
    }

    /// A binding of a function of this workspace is read as one, and a call is
    /// not.
    ///
    /// The floor under [`nothing_binds_a_workspace_call`], in the shape
    /// [`a_reach_into_this_workspace_is_read_however_it_is_spelled`] has: a
    /// census that reported nothing would be satisfied by any binding, and one
    /// that reported every path would fire on this module's own imports. The
    /// third case of the first list is the one the negative case
    /// `a path that opens no parenthesis` is about — it is correctly no *call*,
    /// and it is exactly what this reads.
    #[test]
    fn a_binding_of_a_workspace_function_is_read_as_one() {
        // Assembled, or this test's own text is a binding site.
        let root = ["cra", "te::"].join("");
        let up = ["sup", "er::"].join("");
        let sibling = ["nc_catalog", "_compiler::"].join("");
        for (why, code, resolved) in [
            (
                "the `let` that hands a function on, which the call census drops",
                format!("let go = {root}docs::run;\n"),
                format!("{root}docs::run"),
            ),
            (
                "the same binding under the parent spelling",
                format!("let go = {up}docs::run;\n"),
                format!("{root}docs::run"),
            ),
            (
                "a function put in an array rather than assigned",
                format!("let _ = [{root}docs::run].map(|go| go(root));\n"),
                format!("{root}docs::run"),
            ),
            (
                "a sibling member handed to something that calls it",
                format!("let _ = Some({sibling}camel).map(|go| go(\"probe\"));\n"),
                format!("{sibling}camel"),
            ),
        ] {
            assert_eq!(
                the_workspace_bindings_of(&code),
                vec![resolved.clone()],
                "`{code}` binds `{resolved}` ({why}) and the census walks past it, so the \
                 name it is bound to is a reach `MAY_REACH` can never refuse"
            );
        }
        for (why, code) in [
            (
                "a call, which the other half reads",
                format!("{root}docs::run(root)?;\n"),
            ),
            (
                "an import, which `MAY_IMPORT` decides",
                format!("use {root}ui;\n"),
            ),
            (
                "an import the formatter wrapped across lines",
                format!("use {root}ui::{{\n    ok,\n}};\n"),
            ),
            (
                "a citation rather than code",
                format!("// see {root}check::run\n"),
            ),
            (
                "ordinary code",
                "let root = repository_root();\n".to_owned(),
            ),
        ] {
            assert!(
                the_workspace_bindings_of(&code).is_empty(),
                "`{code}` binds nothing ({why}) and the census reports one, so the rule \
                 fires on this module's own imports"
            );
        }
    }

    /// A visibility in front of an import does not hide it from the census.
    #[test]
    fn a_re_export_is_read_as_the_import_it_is() {
        for spelled in [
            "use std::fs as disk;",
            "pub use std::fs as disk;",
            "pub(crate) use std::fs as disk;",
            "pub(super) use std::fs as disk;",
            "pub(in crate::release) use std::fs as disk;",
            "    pub(crate) use std::fs as disk;",
        ] {
            let read = the_imports_of(&format!("{spelled}\n"));
            assert_eq!(
                read, "use std::fs as disk;\n",
                "`{spelled}` was not read as the import it is, so the alias rule in \
                 `the_gate_runs_before_the_first_write` holds against nothing"
            );
        }
        // And a name that merely opens with those three letters is not one.
        assert_eq!(the_imports_of("published = 1;\n"), "");
        assert_eq!(the_imports_of("pubuse std::fs;\n"), "");
    }

    /// The restore puts the lockfile's own bytes back, and not some of them.
    ///
    /// **A count and a string literal do not hold it.** A count of the sites
    /// that call `fs::write(` in what this module ships, and a failure message
    /// holding a sentence, are both true of a restore that writes anything:
    /// measured in wave 16 as `&locked[..0]`, which left all 627 tests green
    /// and truncated the lockfile to nothing after a failed `cargo update`. So
    /// this asks the function for the bytes, and the test of a failed relock
    /// asks `written` for them after the read-back refused what it wrote.
    #[test]
    fn the_lockfile_comes_back_byte_for_byte() {
        // **A file, not a directory.** A scratch *tree* is a thing this
        // repository counts and guards — `BUILDS_A_SCRATCH` and its two
        // censuses — and this test needs one path, so it builds none.
        let lockfile =
            std::env::temp_dir().join(format!("nc-release-restore-{}.lock", std::process::id()));
        // Bytes a truncation, a trim and a re-encoding each change.
        let locked: Vec<u8> = b"# pristine\n[[package]]\nname = \"nc-log\"\n\n\t \r\n".to_vec();
        std::fs::write(&lockfile, b"whatever a failed update left").expect("writable");
        super::put_the_lockfile_back(&lockfile, &locked).expect("the restore writes");
        let back = std::fs::read(&lockfile).expect("readable");
        let _ = std::fs::remove_file(&lockfile);
        assert_eq!(
            back, locked,
            "the restore is the only thing that puts `Cargo.lock` back, and it put \
             back something else"
        );
    }

    /// The changelog comes back as the release found it: its bytes, or no
    /// file at all.
    ///
    /// **Both halves, because `std::fs::write` creates as well as truncates.**
    /// A changelog the tree had is put back byte for byte over what a failed
    /// write left in it; one the tree had none of is removed, whatever the
    /// write left there; and one never created is no file to remove and no
    /// failure to put it back.
    #[test]
    fn the_changelog_comes_back_byte_for_byte_or_not_at_all() {
        let scratch = LeavesNothingBehind::new("release-changelog-back");
        let changelog = scratch.root.join("CHANGELOG.md");
        // Bytes a truncation, a trim and a re-encoding each change.
        let held: &[u8] = b"# Changelog\n\n## v0.1.0\n\n\t \r\n";
        std::fs::write(&changelog, b"# Chan").expect("a writable scratch");
        super::put_the_changelog_back(&changelog, Some(held)).expect("the restore writes");
        assert_eq!(
            std::fs::read(&changelog).expect("readable"),
            held,
            "a changelog the release found came back as something else"
        );

        std::fs::write(&changelog, b"").expect("a writable scratch");
        super::put_the_changelog_back(&changelog, None).expect("the restore removes");
        assert!(
            !changelog.exists(),
            "a changelog the tree had none of is still there after the release put it back"
        );
        super::put_the_changelog_back(&changelog, None)
            .expect("a changelog never created is no failure to put it back");
    }

    #[test]
    fn a_manifest_yields_its_version_and_a_file_without_one_yields_nothing() {
        assert_eq!(
            version_in("[workspace.package]\nversion = \"0.1.0\"\n").as_deref(),
            Some("0.1.0")
        );
        // The first one wins, which is the workspace's — a member's
        // `version.workspace = true` carries no literal to be confused with it.
        assert_eq!(
            version_in("version = \"0.1.0\"\nversion = \"9.9.9\"\n").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(version_in("edition = \"2024\"\n"), None);
        assert_eq!(version_in("version.workspace = true\n"), None);

        // **A dependency pinned above the workspace's own table does not win.**
        // Taking the first trimmed line carrying the key, whatever table it is
        // in, makes a release rewrite *that* — corrupting a requirement, and
        // reporting `1 file(s) carry 0.2.0` for a release that did not happen.
        // A read-back would not see it, because it re-reads through this same
        // function and would agree with itself.
        let decoy = "[workspace.dependencies.house-crate]\nversion = \"0.0.1\"\n\
                     path = \"crates/house\"\n\n[workspace.package]\nversion = \"0.1.0\"\n";
        assert_eq!(version_in(decoy).as_deref(), Some("0.1.0"));

        // **And the value is read as TOML writes it.** Both of these are
        // manifests cargo parses, and a reader of one spelling answers `None`
        // for both — which is `Cargo.toml names no version` about a file that
        // names one.
        assert_eq!(
            version_in("[workspace.package]\nversion=\"0.1.0\"\n").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(
            version_in("[workspace.package]\nversion = \"0.1.0\" # keep this\n").as_deref(),
            Some("0.1.0")
        );

        // **And TOML's other string form.** `version = '0.1.0'` is a literal
        // string; cargo parses it like any other — `cargo metadata` on a scratch
        // manifest answers `"version":"0.1.0"` — and a reader of double quotes
        // answers `None`, so the whole gate goes red on a tree cargo builds and
        // `release` refuses a manifest naming the version it holds. TOML has
        // exactly two string forms, so these are all
        // of them rather than the next list somebody guesses.
        for said in [
            "[workspace.package]\nversion = '0.1.0'\n",
            "[workspace.package]\nversion='0.1.0'\n",
            "[workspace.package]\nversion = '0.1.0' # keep this\n",
        ] {
            assert_eq!(
                version_in(said).as_deref(),
                Some("0.1.0"),
                "a manifest cargo parses answered no version: {said}"
            );
        }
        // And a quote of one kind does not close a string opened with the other.
        assert_eq!(
            version_in("[workspace.package]\nversion = '0.1.0\"\n"),
            None
        );
    }

    #[test]
    fn every_file_listed_as_carrying_the_version_actually_carries_one() {
        // The list is what makes a release refuse rather than silently leave a
        // file behind, and a list naming a file that holds no version would make
        // every release fail for a reason nobody put there.
        assert!(!CARRIES_THE_VERSION.is_empty(), "nothing carries a version");
        // **The needle itself, written out here.** Every assertion below asks
        // `version_in`, which builds its needle from `KEY` — so `KEY` broken to
        // anything a manifest never holds makes `version_in` answer `None`
        // everywhere, and the loop below finds nothing to be wrong about because
        // its own filter has emptied. Measured without this: `cargo xtask check`
        // green with the release verb unable to find a version at all.
        //
        // TOML writes it exactly this way, and `version_of` is documented as
        // reading it the way TOML writes it.
        assert_eq!(
            KEY, "version = \"",
            "the key a manifest holds its version under is spelled another way here, and \
             everything below builds its needle from it"
        );
        for name in CARRIES_THE_VERSION {
            let text = std::fs::read_to_string(root().join(name))
                .unwrap_or_else(|error| panic!("{name} is listed and unreadable: {error}"));
            assert!(
                version_in(&text).is_some(),
                "{name} is listed as carrying the version and holds none"
            );
        }

        // **And nothing unlisted carries one**, which is the direction this
        // module's header is actually worried about: "the shape that leaves one
        // file behind on the day somebody adds it". The test above cannot see
        // that day — it only ever reads the files already named.
        //
        // The version this tree is at, looked for everywhere the walk goes.
        // `crates/nc-log/src/lib.rs` already has an `Origin { version }` field,
        // and the first service that fills it from a literal is the file that
        // would be left behind.
        let at = version_in(
            &std::fs::read_to_string(root().join(CARRIES_THE_VERSION[0])).expect("readable"),
        )
        .expect("the manifest names a version");
        // **And the spellings a *writer* would not use.** `SPELLINGS` is the
        // writer's list, and the writer rewrites TOML — but this half asks a
        // different question: which files *name* the version and are not listed
        // as carrying it. `crates/nc-log/src/lib.rs` already has an
        // `Origin { version: Literal }`, and the first service to fill it from a
        // literal writes `version: "0.1.0"`, which no TOML spelling matches.
        // That is the file the module header names as "the one left behind on
        // the day somebody adds it".
        //
        // Reported rather than rewritten: a Rust file naming the version is a
        // decision for a person — either it joins `CARRIES_THE_VERSION` and the
        // writer learns its spelling, or it stops naming the version.
        // **A version closes with the quote it opened with.** Appending a double
        // quote to every spelling, when half of `SPELLINGS` opens with a single
        // one, makes four of the eight needles `version = '0.1.0"`, a string no
        // TOML file can hold. Measured: that impossible text written into a file
        // is found and the real `version = '0.1.0'` is not, so a second copy of
        // the version in single quotes is invisible to such a rule while
        // `version_of` reads both forms.
        let mut needles: Vec<String> = SPELLINGS
            .iter()
            .map(|spelling| {
                let closes = if spelling.ends_with('\'') { '\'' } else { '"' };
                format!("{spelling}{at}{closes}")
            })
            .collect();
        // Every spelling a *file* can name the version in, TOML, YAML, JSON and
        // Rust. `crates/nc-log/src/lib.rs` already carries an `Origin { version
        // }`, and the first service to fill it writes one of the last three —
        // measured, `const VERSION: &str = "0.1.0";` in `dev.rs` is invisible
        // to a list of TOML spellings, and `literal!("0.1.0")` is what the type
        // wall forces a service to write.
        for spelling in [
            "version: ",
            "version = ",
            "\"version\": ",
            "VERSION: &str = ",
            "VERSION: &'static str = ",
            "literal!(",
        ] {
            let named = format!("{spelling}\"{at}\"");
            needles.push(named);
        }
        assert!(
            needles.contains(&format!("{KEY}{at}\"")),
            "`KEY` is not one of the spellings the writer knows, so the two halves of \
             this rule are looking for different things"
        );
        // **And every needle is a string a file could hold.** Built from
        // `SPELLINGS` with one closing quote, four of the eight are
        // `version = '0.1.0"` — opened with one quote and closed with the other
        // — so half such a scan looks for text no TOML file can contain, and a
        // second copy of the version in single quotes is invisible to it while
        // `version_of` reads that form.
        for spelling in SPELLINGS {
            let opens = spelling
                .chars()
                .next_back()
                .expect("a spelling ends in a quote");
            let built = needles
                .iter()
                .find(|needle| needle.starts_with(spelling))
                .unwrap_or_else(|| panic!("`{spelling}` built no needle"));
            assert!(
                built.ends_with(opens),
                "`{built}` opens with `{opens}` and closes with something else, so it is \
                 a string no file can hold and this half of the scan reads nothing"
            );
        }
        let mut carriers = Vec::new();
        // **A floor on the walk.** `carriers.is_empty()` is an assertion over an
        // empty collection, and `if true { return; }` as the first line of this
        // callback leaves it green — measured. The number read is the number that
        // says the walk happened at all.
        let mut read = 0_usize;
        crate::gates::walk(root(), &mut |path| {
            let relative = crate::relative_to(root(), path);
            if CARRIES_THE_VERSION.contains(&relative.as_str()) || relative == "Cargo.lock" {
                return;
            }
            // This file, which writes the needles out in order to look for them.
            if relative == "xtask/src/release.rs" {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            // **After the judgement, not before it.** `read` is the floor that
            // makes `carriers.is_empty()` mean something, and standing above the
            // decision it lets one `if !relative.starts_with("crates/") { return; }`
            // under it leave a real unlisted carrier unreported with the floor
            // satisfied — measured, a probe file under `docs/` holding the
            // workspace version, with every test passing.
            let names_it = needles.iter().any(|needle| text.contains(needle.as_str()));
            read += 1;
            if names_it {
                carriers.push(relative);
            }
        })
        .expect("the tree is walkable");
        assert!(
            carriers.is_empty(),
            "these name the workspace version and are not in CARRIES_THE_VERSION, so a \
             release would leave them behind: {carriers:?}"
        );
        assert!(
            read > 100,
            "the walk read {read} file(s), so `carriers.is_empty()` is true of a \
             collection nothing could have been put in"
        );
    }

    /// The body of the function `name` in `source`, from its signature to the
    /// line that closes it at the top level.
    ///
    /// The needle is assembled from `name` at run time, so a test naming a
    /// function never finds itself.
    fn the_body_of<'a>(source: &'a str, name: &str) -> &'a str {
        let opened = ["\nfn ", "\npub(crate) fn "]
            .iter()
            .filter_map(|opens| {
                let needle = format!("{opens}{name}");
                source.match_indices(&needle).find_map(|(at, _)| {
                    source[at + needle.len()..]
                        .starts_with(['(', '<'])
                        .then_some(at + 1)
                })
            })
            .min()
            .unwrap_or_else(|| panic!("`{name}` is a function of this file"));
        let rest = &source[opened..];
        &rest[..rest.find("\n}\n").unwrap_or(rest.len())]
    }

    /// The warning prints before the window it warns about opens.
    ///
    /// The whole of that line's value is its position: moved below the relock,
    /// it reads as a release that happened and recovers nobody.
    fn the_warning_prints_before_the_window(body: &str) {
        let said = body
            .find(&["ui::detail(", "\n        \"rewriting Cargo.lock"].join(""))
            .expect("the release warns before it rewrites the lockfile");
        let window = body
            .find(&["relock(", "root, setting"].join(""))
            .expect("the release relocks");
        assert!(
            said < window,
            "the line warning that `Cargo.lock` is being rewritten prints after the \
             rewrite has already begun, so an interrupted release leaves a tree \
             nobody was told how to recover"
        );
    }

    /// The gate runs before anything is written, the lockfile is read before
    /// the first write, and nothing reaches GitHub before the read-back.
    ///
    /// **A source assertion, and it says so.** Running the real thing means
    /// running the whole gate and then rewriting the manifest of the tree the
    /// test is running in. What can be checked is the order of the calls, which
    /// is the whole of each rule — and deleting the gate call fails this test.
    ///
    /// What this does not prove: that the gate checks anything. That is
    /// `check`'s own suite.
    #[test]
    fn the_gate_runs_before_the_first_write() {
        let source = std::fs::read_to_string(root().join("xtask/src/release.rs"))
            .expect("this file is readable");
        let run = the_body_of(&source, "run_at");
        let gate = run
            .find(&["the_gate(", "itself, root, inherited)?;"].join(""))
            .expect("the gate is run from the release verb, and its refusal handed back");
        let onwards = run
            .find(&["rele", "ased(&Releasing"].join(""))
            .expect("the release verb hands on to `released`");
        assert!(
            gate < onwards,
            "the gate is called after the release is computed and written, so a red tree \
             would be rewritten before anyone found out"
        );
        let written = the_body_of(&source, "written");
        let read = written
            .find(&["std::fs::read(root.join(\"Cargo", ".lock\"))"].join(""))
            .expect("the release reads the lockfile it rewrites");
        let first_write = written
            .find(&["rewrite_listed(root, &CARRIES_THE", "_VERSION, &target)"].join(""))
            .expect("the release writes through rewrite_listed");
        assert!(
            read < first_write,
            "`Cargo.lock` is read after the first write, so a release that cannot read it \
             prints `Nothing was written` over a manifest already at the new version"
        );
        the_warning_prints_before_the_window(the_body_of(&source, "written_after_the_version"));
        let released = the_body_of(&source, "released");
        let the_files = released
            .find(&["writ", "ten(setting, version, &changelog)"].join(""))
            .expect("the release writes before it publishes");
        let committed = released
            .find(&["the_release_", "commit(github"].join(""))
            .expect("the release makes its commit");
        assert!(
            the_files < committed,
            "the release commit is made before the files it carries are written and read \
             back, so GitHub would receive bytes nobody read"
        );

        // **What this module *ships*, which is the code above the tests and
        // the code below them**, cut by `crate::gates::the_test_modules`, which
        // matches braces in a text where every comment and every literal is
        // spent character for character.
        let tests = crate::gates::the_test_modules(&source)
            .expect("every module of tests in this file closes its braces");
        let mut ran = String::new();
        let mut shipped_from = 0;
        for region in &tests {
            ran.push_str(&source[shipped_from..region.start]);
            shipped_from = region.end;
        }
        ran.push_str(&source[shipped_from..]);
        assert!(
            !ran.contains(&["#[te", "st]"].join("")),
            "a module of tests was not cut out, so the counts below are about the tests as \
             well as about what this module ships"
        );
        assert!(
            tests
                .iter()
                .all(|region| source[region.clone()].contains(&["mod", " "].join(""))),
            "something that is not a module was cut out of the shipped code"
        );
        // **A rename is what makes any list of spellings blind**, so the
        // shipped half imports nothing under another name, and reaches into
        // this workspace through the calls and imports written down.
        let imports = the_imports_of(&ran);
        assert!(
            !imports.contains(" as "),
            "this module imports something under another name, and every rule below is a \
             list of spellings: `use std::fs as disk;` makes `disk::write(…)` a write none of \
             them can see. The imports are:\n{imports}"
        );
        nothing_renames_or_reaches_past_the_gate(&ran, &imports);
        assert!(
            imports.contains("use std::path::Path"),
            "the shipped half of this module reads as having no imports at all, so the rule \
             above holds against nothing: {imports}"
        );
        the_writes_are_the_ones_written_down(&ran);
        the_children_are_cargo_and_gh(&ran);
        // **Only `run` asks where this tool is**, so what `run_at` runs is what
        // a test drives with a gate of its own.
        let asks = ["current", "_exe("].join("");
        assert!(
            ran.matches(&asks).count() == 1 && the_body_of(&source, "run").contains(&asks),
            "something below `run` asks where this tool is, so the verb a test drives through \
             `run_at` is not the verb CI runs"
        );
    }

    /// Every way this module puts bytes on disk is one of the writes written
    /// down here, in the function written down beside it.
    ///
    /// **The spellings this list names, which is not every spelling** — a file
    /// arrives through `copy`, `rename`, `OpenOptions` and `write!` too, so each
    /// is named; what the sentence claims is what the list holds. Written
    /// without a `std::` prefix, so a `use std::fs;` spelling is the same match.
    #[track_caller]
    fn the_writes_are_the_ones_written_down(ran: &str) {
        let puts_bytes_on_disk = [
            "fs::write(",
            "fs::copy(",
            "fs::rename(",
            "fs::remove_file(",
            "fs::remove_dir(",
            "fs::remove_dir_all(",
            "fs::create_dir(",
            "fs::create_dir_all(",
            "fs::hard_link(",
            "fs::soft_link(",
            "fs::set_permissions(",
            "fs::DirBuilder",
            "File::create(",
            "File::create_new(",
            "File::options(",
            "OpenOptions",
            "write_all(",
            "write!(",
            "writeln!(",
            "symlink(",
        ];
        // Each write, and where it is: the version in `rewrite_each`; the
        // changelog in `written_after_the_version`; every put-back of what a
        // file held in `put_back_as`, which the lockfile's, the changelog's and
        // each listed file's all go through; the changelog's removal when the
        // tree held none in `put_the_changelog_back`; and the request handed to
        // `gh` on its standard input, which is a pipe and not a file.
        let written_down: [(&str, &str, usize); 9] = [
            ("rewrite_each", "fs::write(", 1),
            ("rewrite_listed", "fs::write(", 0),
            ("written_after_the_version", "fs::write(", 1),
            ("written", "fs::write(", 0),
            ("put_back_as", "fs::write(", 1),
            ("put_the_lockfile_back", "fs::write(", 0),
            ("put_the_changelog_back", "fs::remove_file(", 1),
            ("gh", "write_all(", 1),
            ("gh", "fs::write(", 0),
        ];
        for needle in puts_bytes_on_disk {
            let found = ran.matches(needle).count();
            let expected: usize = written_down
                .iter()
                .filter(|(_, spelled, _)| *spelled == needle)
                .map(|(_, _, count)| count)
                .sum();
            assert_eq!(
                found, expected,
                "`{needle}` puts bytes on disk {found} time(s) in what this module ships, and \
                 {expected} are written down: a write outside the ones this test names is a \
                 write nothing orders after the gate"
            );
        }
        for (function, needle, count) in written_down {
            assert_eq!(
                the_body_of(ran, function).matches(needle).count(),
                count,
                "`{function}` holds another number of `{needle}` than is written down"
            );
        }
    }

    /// This module starts three programs: this same tool for the gate, cargo
    /// for the relock, and `gh` to publish. Git is asked through the history
    /// step's reader, never here.
    #[track_caller]
    fn the_children_are_cargo_and_gh(ran: &str) {
        let spent = crate::gates::blanked(ran);
        assert_eq!(
            spent.matches(&["Command", "::new("].join("")).count(),
            3,
            "this module starts another number of programs than three — this tool for the \
             gate, cargo for the relock and `gh` to publish; git is asked through \
             `crate::history`, which holds what it inherits"
        );
        for started in [
            ["std::process::Command::new(", "itself)"].join(""),
            [".arg(", "\"check\")"].join(""),
            ["std::process::Command::new(", "program)"].join(""),
            ["std::process::Command::new(", "\"gh\")"].join(""),
            [".args([\"update\", ", "\"--workspace\"])"].join(""),
        ] {
            assert!(
                ran.contains(&started),
                "`{started}` is not in what this module ships, so the program it starts is \
                 not the one written down"
            );
        }
    }

    /// A change, as a test writes one.
    fn a_change_of(kind: &str, scope: Option<&str>, breaking: bool, description: &str) -> Change {
        Change {
            hash: format!("{:0<40}", description.len()),
            kind: kind.to_owned(),
            scope: scope.map(str::to_owned),
            breaking,
            description: description.to_owned(),
            note: None,
        }
    }

    /// A release tag, as a test writes one.
    fn a_tag(version: &str, merged: bool) -> Tag {
        Tag {
            version: Version::parse(version).expect("a version a test writes"),
            commit: format!("{:a>40}", version.len()),
            merged,
        }
    }

    /// A version a test writes.
    fn v(text: &str) -> Version {
        Version::parse(text).expect("a version a test writes")
    }

    /// The two shapes this release orders are read, and written back as read.
    #[test]
    fn three_numbers_and_a_candidate_of_them_are_versions() {
        for written in [
            "0.1.0",
            "12.0.345",
            "0.10.0",
            "10.0.0",
            "0.1.0-rc.1",
            "1.2.3-rc.10",
        ] {
            assert_eq!(
                Version::parse(written).map(|version| version.to_string()),
                Ok(written.to_owned()),
                "`{written}` is not read and written back as itself"
            );
        }
        assert_eq!(v("0.1.0-rc.2").tag(), "v0.1.0-rc.2");
        assert_eq!(v("1.2.3").tag(), "v1.2.3");
    }

    /// A version cargo would refuse is refused here first, and so is every
    /// shape this release cannot order.
    ///
    /// `"01".parse::<u64>()` is `Ok(1)`, so `01.02.03` would be read — and the
    /// write would then leave a manifest nothing in the workspace parses,
    /// this tool included.
    #[test]
    fn anything_else_is_not_a_version_and_the_message_says_what_is() {
        for attempt in [
            "01.02.03",
            "0.01.0",
            "1.0.00",
            "+1.0.0",
            "1.0.0 ",
            "0.1",
            "0.1.0.0",
            "v0.1.0",
            "0.1.0-rc1",
            "0.1.0-rc.0",
            "0.1.0-rc.01",
            "0.1.0-beta.1",
            "0.1.0-rc.1-rc.2",
            "0.1.0-",
            "",
            "a.b.c",
        ] {
            let error = Version::parse(attempt).expect_err("not a version this orders");
            assert!(
                error.contains("`X.Y.Z` and `X.Y.Z-rc.N`"),
                "the message has to say what a version looks like: {error}"
            );
        }
    }

    /// A release goes forwards only, in the order this release knows.
    #[test]
    fn a_release_goes_forwards_only() {
        for (from, to) in [
            ("0.1.0", "0.2.0"),
            ("0.1.0", "0.1.1"),
            ("0.9.9", "1.0.0"),
            // Numbers, not text: `0.10.0` is ahead of `0.9.0`.
            ("0.9.0", "0.10.0"),
            // A candidate is below the release it is a candidate for, and
            // below the next candidate.
            ("0.1.0-rc.1", "0.1.0"),
            ("0.1.0-rc.1", "0.1.0-rc.2"),
            ("0.1.0-rc.9", "0.1.0-rc.10"),
            ("0.1.0", "0.1.1-rc.1"),
            ("0.9.0", "0.10.0-rc.1"),
        ] {
            assert!(super::moves_forward(v(from), v(to)), "{from} → {to}");
            assert!(!super::moves_forward(v(to), v(from)), "{to} → {from}");
        }
        // A repeat is not forwards: it would publish a tag that already names
        // something else.
        for same in ["0.1.0", "0.1.0-rc.1"] {
            assert!(!super::moves_forward(v(same), v(same)));
        }
    }

    /// Each type moves the version as far as `NOCTUA.md §22.3` says, and no
    /// further.
    #[test]
    fn each_type_moves_the_version_as_far_as_it_should() {
        for kind in crate::history::TYPES {
            let moved = match kind {
                "feat" => Some([1, 3, 0]),
                "fix" | "perf" => Some([1, 2, 4]),
                _ => None,
            };
            assert_eq!(
                super::the_next_numbers(
                    Some(v("1.2.3")),
                    v("1.2.3"),
                    &[a_change_of(kind, None, false, "say what it does")]
                ),
                moved,
                "a `{kind}` moved 1.2.3 to the wrong place"
            );
        }
        // **The furthest-reaching wins**, wherever it sits in the history.
        let mixed = [
            a_change_of("fix", None, false, "a fix"),
            a_change_of("feat", Some("xtask"), false, "a feat"),
            a_change_of("docs", None, false, "a page"),
        ];
        assert_eq!(
            super::the_next_numbers(Some(v("1.2.3")), v("1.2.3"), &mixed),
            Some([1, 3, 0])
        );
        // A breaking change, whatever its type, is a major past 0.x.
        let broken = [a_change_of("refactor", None, true, "rename the thing")];
        assert_eq!(
            super::the_next_numbers(Some(v("1.2.3")), v("1.2.3"), &broken),
            Some([2, 0, 0])
        );
    }

    /// While the major is 0, a breaking change is a minor.
    #[test]
    fn a_major_is_a_minor_while_the_major_is_0() {
        let broken = [a_change_of("feat", None, true, "rename the thing")];
        assert_eq!(
            super::the_next_numbers(Some(v("0.3.1")), v("0.3.1"), &broken),
            Some([0, 4, 0])
        );
        assert_eq!(
            super::bumped([0, 3, 1], super::Bump::Major),
            Some([0, 4, 0])
        );
        assert_eq!(
            super::bumped([1, 3, 1], super::Bump::Major),
            Some([2, 0, 0])
        );
        assert_eq!(
            super::bumped([0, 3, 1], super::Bump::Minor),
            Some([0, 4, 0])
        );
        assert_eq!(
            super::bumped([0, 3, 1], super::Bump::Patch),
            Some([0, 3, 2])
        );
        assert_eq!(super::bumped([0, 3, 1], super::Bump::Nothing), None);
    }

    /// With no production release, the first is the version the manifest
    /// carries, unbumped, whatever the commits are.
    #[test]
    fn the_first_release_is_the_version_the_manifest_carries() {
        let only_docs = [a_change_of("docs", None, false, "a page")];
        let features = [a_change_of("feat", None, true, "a breaking one")];
        for since in [&only_docs[..], &features[..], &[]] {
            assert_eq!(
                super::the_next_numbers(None, v("0.1.0"), since),
                Some([0, 1, 0])
            );
        }
        // A manifest a release candidate wrote carries the numbers it is a
        // candidate of.
        assert_eq!(
            super::the_next_numbers(None, v("0.1.0-rc.3"), &only_docs),
            Some([0, 1, 0])
        );
    }

    /// Commits that move nothing are nothing to release.
    #[test]
    fn nothing_to_release_is_when_no_commit_since_the_last_one_moves_it() {
        let internal: Vec<Change> = ["docs", "test", "chore", "ci", "build", "refactor", "revert"]
            .iter()
            .map(|kind| a_change_of(kind, Some("xtask"), false, "something internal"))
            .collect();
        assert_eq!(
            super::the_next_numbers(Some(v("0.1.0")), v("0.1.0"), &internal),
            None
        );
        assert_eq!(
            super::the_next_numbers(Some(v("0.1.0")), v("0.1.0"), &[]),
            None
        );
    }

    /// A candidate is numbered one past the highest of its numbers.
    ///
    /// On `staging` that counts every candidate that exists, merged or
    /// not; on `main` a release is refused when its tag exists.
    #[test]
    fn the_candidate_counter_is_one_past_the_highest_that_exists() {
        let tags = [
            a_tag("0.1.0-rc.9", true),
            a_tag("0.1.0", true),
            a_tag("0.2.0-rc.1", true),
            a_tag("0.2.0-rc.3", false),
        ];
        assert_eq!(
            super::on_the_channel(Channel::Staging, [0, 2, 0], &tags),
            Ok(v("0.2.0-rc.4"))
        );
        assert_eq!(
            super::on_the_channel(Channel::Staging, [0, 3, 0], &tags),
            Ok(v("0.3.0-rc.1"))
        );
        assert_eq!(
            super::on_the_channel(Channel::Main, [0, 2, 0], &tags),
            Ok(v("0.2.0"))
        );
        for channel in [Channel::Main, Channel::Staging] {
            let refused =
                super::on_the_channel(channel, [0, 1, 0], &tags).expect_err("v0.1.0 is tagged");
            assert!(refused.contains("v0.1.0 is already tagged"), "{refused}");
        }
    }

    /// `staging` releases only once `main`'s latest release is merged into
    /// it; `main` has no channel above it.
    #[test]
    fn staging_releases_only_once_the_latest_production_release_is_merged() {
        let behind = [
            a_tag("0.1.0", true),
            a_tag("0.2.0", false),
            a_tag("0.3.0-rc.1", false),
        ];
        let refused = super::the_channel_above_is_merged(Channel::Staging, &behind)
            .expect_err("the back-merge has not landed");
        assert!(
            refused.contains("v0.2.0 is the latest production release"),
            "{refused}"
        );
        assert_eq!(
            super::the_channel_above_is_merged(Channel::Main, &behind),
            Ok(())
        );
        let merged = [
            a_tag("0.1.0", true),
            a_tag("0.2.0", true),
            a_tag("0.3.0-rc.1", false),
        ];
        assert_eq!(
            super::the_channel_above_is_merged(Channel::Staging, &merged),
            Ok(())
        );
        assert_eq!(
            super::the_channel_above_is_merged(Channel::Staging, &[]),
            Ok(())
        );
    }

    /// A release behind one `HEAD` reaches, or behind the numbers the
    /// manifest carries, is refused; the first candidate of the manifest's
    /// numbers is not behind them.
    #[test]
    fn a_release_behind_what_head_reaches_is_refused() {
        let reached = [a_tag("0.2.0", true), a_tag("0.9.0", false)];
        assert_eq!(
            super::goes_forward(v("0.3.0-rc.1"), &reached, v("0.2.0")),
            Ok(())
        );
        let refused =
            super::goes_forward(v("0.2.0-rc.1"), &reached, v("0.2.0")).expect_err("behind v0.2.0");
        assert!(refused.contains("not ahead of v0.2.0"), "{refused}");
        assert_eq!(
            super::goes_forward(v("0.1.0-rc.1"), &[], v("0.1.0")),
            Ok(())
        );
        let refused =
            super::goes_forward(v("0.1.0-rc.1"), &[], v("0.2.0")).expect_err("behind the manifest");
        assert!(
            refused.contains("behind the 0.2.0 Cargo.toml carries"),
            "{refused}"
        );
    }

    /// A release commit is a commit of one parent whose message is its subject
    /// alone, and nothing that only looks like one is read as one.
    #[test]
    fn a_release_commit_is_one_parent_and_its_subject_alone() {
        let head = |parents: usize, message: &str| super::Head {
            hash: "a".repeat(40),
            parents,
            message: message.to_owned(),
        };
        for message in [
            "chore(release): v0.1.0-rc.1",
            "chore(release): v0.1.0-rc.1\n",
            "chore(release): v2.0.0\n\n",
        ] {
            assert!(
                super::a_release_commit_of(&head(1, message)).is_some(),
                "`{}` with one parent is a release commit",
                message.escape_debug()
            );
        }
        for (parents, message) in [
            // A merge titled as one: a pull request carrying a release commit's
            // title is not the App's commit.
            (2, "chore(release): v0.1.0-rc.1"),
            (0, "chore(release): v0.1.0-rc.1"),
            (1, "chore(release): v0.1.0-rc.1 (#4)"),
            (1, "chore(release): carry v0.1.0-rc.1 back to dev"),
            (1, "chore(release): v0.1"),
            (
                1,
                "chore(release): v0.1.0-rc.1\n\nAnd a body nobody wrote here.\n",
            ),
            (1, "chore: v0.1.0"),
        ] {
            assert!(
                super::a_release_commit_of(&head(parents, message)).is_none(),
                "`{}` with {parents} parent(s) was read as a release commit",
                message.escape_debug()
            );
        }
        assert_eq!(
            super::a_release_commit_of(&head(1, "chore(release): v0.2.0-rc.3")),
            Some(v("0.2.0-rc.3"))
        );
    }

    /// A commit object, as git stores one, with `message` as its message.
    fn a_commit_object(message: &str) -> Vec<u8> {
        format!(
            "tree {:0>40}\nparent {:1>40}\nauthor A Person <a.person@example.invalid> 0 +0000\n\
             committer A Person <a.person@example.invalid> 0 +0000\n\n{message}",
            0, 1
        )
        .into_bytes()
    }

    /// A breaking change is read from its `!` and from its footer, and the
    /// footer's text is what the changelog says to do.
    #[test]
    fn a_breaking_change_is_read_from_its_bang_and_from_its_footer() {
        let hash = "a".repeat(40);
        let read = |message: &str| {
            super::a_change(&hash, &a_commit_object(message)).expect("a conventional commit")
        };
        let bang = read("feat(nc-kernel)!: rename the world\n");
        assert_eq!(
            (
                bang.kind.as_str(),
                bang.scope.as_deref(),
                bang.breaking,
                bang.note
            ),
            ("feat", Some("nc-kernel"), true, None)
        );
        assert_eq!(bang.description, "rename the world");
        let unscoped = read("fix!: rename the world (#4)\n");
        assert!(unscoped.breaking && unscoped.scope.is_none());
        assert_eq!(unscoped.description, "rename the world (#4)");
        let footer = read(
            "fix(xtask): read the flag as cargo does\n\nWhy it changed.\n\nBREAKING CHANGE: pass \
             `--channel` where a version\n  was passed before\nRefs: #12\n",
        );
        assert!(
            footer.breaking,
            "a footer is a breaking change without a `!`"
        );
        assert_eq!(
            footer.note.as_deref(),
            Some("pass `--channel` where a version was passed before")
        );
        let hyphen = read("perf: a faster walk\n\nBREAKING-CHANGE: the cache moved\n");
        assert_eq!(hyphen.note.as_deref(), Some("the cache moved"));
        // **The convention's spelling, and nothing that merely mentions it.**
        let mentioned =
            read("docs: say when a breaking change: is one\n\nA breaking change: note.\n");
        assert!(
            !mentioned.breaking,
            "prose that names a breaking change is not one"
        );
        let unseen = read("feat!: x\n\nBREAKING CHANGE: a\u{202E}b\n");
        assert_eq!(unseen.note.as_deref(), Some("a\\u{202e}b"));
        // And a subject the history step refuses is refused here, by its hash.
        let refused = super::a_change(&hash, &a_commit_object("Not a subject\n"))
            .expect_err("not conventional");
        assert!(
            refused.contains("commit aaaaaaa's subject is not a conventional one"),
            "{refused}"
        );
    }

    /// Each change is listed in the part its type and its breaking say, and
    /// the internal types are left out.
    #[test]
    fn a_change_is_listed_in_the_part_its_type_says() {
        for (kind, scope, breaking, part) in [
            ("feat", None, false, Some("Added")),
            ("fix", Some("xtask"), false, Some("Fixed")),
            ("perf", None, false, Some("Performance")),
            ("revert", None, false, Some("Removed")),
            ("fix", Some("security"), false, Some("Security")),
            ("chore", Some("security"), false, Some("Security")),
            ("feat", Some("security"), true, Some("Changed")),
            ("refactor", None, true, Some("Changed")),
            ("docs", None, false, None),
            ("test", None, false, None),
            ("chore", None, false, None),
            ("ci", None, false, None),
            ("build", None, false, None),
            ("refactor", Some("xtask"), false, None),
        ] {
            assert_eq!(
                super::the_part_of(&a_change_of(kind, scope, breaking, "x"))
                    .map(super::Part::heading),
                part,
                "`{kind}` ({scope:?}, breaking: {breaking}) is listed in the wrong part"
            );
        }
    }

    /// A release's section: each part that lists something, in order, each
    /// change with its scope and its short hash, and a breaking change's note
    /// under it.
    #[test]
    fn a_section_lists_each_part_in_order_and_leaves_internal_types_out() {
        let mut broken = a_change_of(
            "feat",
            Some("xtask"),
            true,
            "take the channel, not a version",
        );
        broken.note = Some("pass `--channel staging` or `--channel main`".to_owned());
        broken.hash = "b".repeat(40);
        let mut changes = vec![
            a_change_of("fix", None, false, "read the tag as git names it (#3)"),
            a_change_of("docs", None, false, "say it"),
            a_change_of("feat", Some("nc-kernel"), false, "run a plugin"),
            broken,
            a_change_of("perf", None, false, "walk once"),
            a_change_of("chore", None, false, "tidy"),
            a_change_of("fix", Some("security"), false, "refuse a link"),
            a_change_of("revert", None, false, "feat: run a plugin"),
        ];
        changes[0].hash = "c".repeat(40);
        let section = super::Section {
            version: v("0.2.0"),
            changes,
        };
        let short = |length: usize| format!("{length:0<7}");
        assert_eq!(
            super::the_notes(&section),
            format!(
                "### Changed\n\n\
                 - **xtask:** take the channel, not a version (bbbbbbb)\n  What to do: pass \
                 `--channel staging` or `--channel main`\n\n\
                 ### Security\n\n- **security:** refuse a link ({})\n\n\
                 ### Added\n\n- **nc-kernel:** run a plugin ({})\n\n\
                 ### Fixed\n\n- read the tag as git names it (#3) (ccccccc)\n\n\
                 ### Performance\n\n- walk once ({})\n\n\
                 ### Removed\n\n- feat: run a plugin ({})\n",
                short("refuse a link".len()),
                short("run a plugin".len()),
                short("walk once".len()),
                short("feat: run a plugin".len()),
            )
        );
        let internal = super::Section {
            version: v("0.2.1"),
            changes: vec![a_change_of("docs", None, false, "say it")],
        };
        assert_eq!(
            super::the_notes(&internal),
            format!("{}\n", super::NOTHING_TO_LIST)
        );
    }

    /// The changelog is its opening, then each section under its heading.
    ///
    /// The sections are in the order handed in, and the same sections give
    /// the same bytes.
    #[test]
    fn the_changelog_is_its_sections_under_their_headings_and_nothing_else() {
        let sections = [
            super::Section {
                version: v("0.2.0-rc.1"),
                changes: vec![a_change_of("feat", None, false, "a feat")],
            },
            super::Section {
                version: v("0.1.0"),
                changes: vec![a_change_of("fix", Some("xtask"), false, "a fix")],
            },
        ];
        let changelog = super::the_changelog(&sections);
        assert_eq!(
            changelog,
            format!(
                "{}\n## v0.2.0-rc.1\n\n### Added\n\n- a feat ({:0<7})\n\n## v0.1.0\n\n### \
                 Fixed\n\n- **xtask:** a fix ({:0<7})\n",
                super::OPENING,
                "a feat".len(),
                "a fix".len()
            )
        );
        assert_eq!(changelog, super::the_changelog(&sections.clone()));
        assert!(
            changelog.starts_with("# Changelog\n\n") && changelog.ends_with(")\n"),
            "{changelog}"
        );
    }

    /// A candidate below the last production release folds into it; the
    /// candidates above it are sections of their own, oldest first, after
    /// every production release.
    #[test]
    fn a_candidate_folds_into_the_release_it_was_a_candidate_for() {
        let tags = [
            a_tag("0.1.0-rc.1", true),
            a_tag("0.1.0-rc.2", true),
            a_tag("0.1.0", true),
            a_tag("0.1.1-rc.1", true),
            a_tag("0.2.0-rc.1", true),
            a_tag("0.2.0-rc.2", false),
        ];
        let new = super::Point {
            version: v("0.2.0-rc.3"),
            at: "f".repeat(40),
        };
        let read: Vec<String> = super::the_points(&tags, &new)
            .iter()
            .map(|point| point.version.to_string())
            .collect();
        assert_eq!(read, ["0.1.0", "0.1.1-rc.1", "0.2.0-rc.1", "0.2.0-rc.3"]);
        // And when 0.2.0 ships, every candidate below it folds in.
        let shipped = super::Point {
            version: v("0.2.0"),
            at: "f".repeat(40),
        };
        let read: Vec<String> = super::the_points(&tags, &shipped)
            .iter()
            .map(|point| point.version.to_string())
            .collect();
        assert_eq!(read, ["0.1.0", "0.2.0"]);
        // A release already tagged at the commit it is made at is one point.
        let tagged = [a_tag("0.1.0-rc.1", true)];
        let again = super::Point {
            version: v("0.1.0-rc.1"),
            at: tagged[0].commit.clone(),
        };
        assert_eq!(super::the_points(&tagged, &again), vec![again]);
    }

    /// GitHub's limits on a release's assets are asked of every list, however
    /// short.
    #[test]
    fn a_list_of_assets_github_would_refuse_is_refused() {
        assert_eq!(super::within_githubs_limits(&super::ASSETS), Ok(()));
        let many: Vec<(&str, u64)> = (0..=super::MOST_ASSETS).map(|_| ("a", 1)).collect();
        let refused = super::within_githubs_limits(&many).expect_err("one too many");
        assert!(refused.contains("1001 asset(s)"), "{refused}");
        assert_eq!(super::within_githubs_limits(&many[1..]), Ok(()));
        let large = [("noctua.tar", super::LARGEST_ASSET)];
        let refused = super::within_githubs_limits(&large).expect_err("not under 2 GiB");
        assert!(refused.contains("`noctua.tar`"), "{refused}");
        assert_eq!(
            super::within_githubs_limits(&[("noctua.tar", super::LARGEST_ASSET - 1)]),
            Ok(())
        );
        assert_eq!(super::LARGEST_ASSET, 2_147_483_648, "2 GiB");
    }

    /// The encoder `createCommitOnBranch` is handed a file's contents through
    /// gives RFC 4648's own vectors.
    #[test]
    fn base64_is_rfc_4648_s() {
        for (bytes, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::base64(bytes.as_bytes()), encoded, "`{bytes}`");
        }
        assert_eq!(super::base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    /// No fixed text a release publishes, and no changelog it writes, holds
    /// a line the history step reads as a credit.
    #[test]
    fn what_a_release_publishes_credits_no_agent() {
        let sections = [super::Section {
            version: v("0.1.0"),
            changes: vec![a_change_of("feat", None, false, "a feat")],
        }];
        for (what, text) in [
            ("the changelog", super::the_changelog(&sections)),
            ("an empty section", super::NOTHING_TO_LIST.to_owned()),
            (
                "the pull request's title",
                super::carried_back_as(Channel::Staging, v("0.1.0-rc.1")),
            ),
        ] {
            assert_eq!(
                crate::history::credits(&text),
                Vec::new(),
                "{what} holds a line the history step reads as a credit"
            );
            assert_eq!(
                crate::history::subject(&super::carried_back_as(Channel::Main, v("1.0.0"))),
                Ok(()),
                "the back-merge's title is not a conventional subject"
            );
        }
        assert_eq!(
            crate::history::subject(&format!("{}0.1.0-rc.1", super::RELEASE_COMMIT_OPENS)),
            Ok(()),
            "the release commit's subject is not a conventional one"
        );
    }

    /// A repository with a Cargo workspace of one member, its lockfile
    /// committed, and a history of `commits` above it.
    fn a_workspace(what: &str, commits: &[&str]) -> ARepository {
        let repository = ARepository::new(what);
        let at = repository.root();
        for (path, text) in [
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"probe\"]\nresolver = \"3\"\n\n[workspace.package]\n\
                 version = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "probe/Cargo.toml",
                "[package]\nname = \"probe\"\nversion.workspace = true\nedition.workspace = \
                 true\npublish = false\n",
            ),
            ("probe/src/lib.rs", "//! A probe.\n"),
            (
                "Cargo.lock",
                "# This file is automatically @generated by Cargo.\n# It is not intended for \
                 manual editing.\nversion = 4\n\n[[package]]\nname = \"probe\"\nversion = \
                 \"0.1.0\"\n",
            ),
        ] {
            let path = at.join(path);
            std::fs::create_dir_all(path.parent().expect("a file sits in a directory"))
                .expect("a writable scratch");
            std::fs::write(path, text).expect("a writable scratch");
        }
        repository.git(&["add", "-A"]);
        repository.commit("feat: add the probe", &[]);
        for message in commits {
            repository.commit(message, &[]);
        }
        repository
    }

    /// This process's environment, with cargo kept off the network.
    fn this_environment() -> Vec<(OsString, OsString)> {
        let mut inherited: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(name, _)| name != "CARGO_NET_OFFLINE")
            .collect();
        inherited.push(("CARGO_NET_OFFLINE".into(), "true".into()));
        inherited
    }

    /// A setting for the release of `root` on `channel`, as far as `mode`.
    fn a_setting<'a>(
        root: &'a Path,
        channel: Channel,
        mode: Mode,
        inherited: &'a [(OsString, OsString)],
        github: Option<&'a GitHub>,
    ) -> Releasing<'a> {
        Releasing {
            root,
            channel,
            mode,
            inherited,
            github,
        }
    }

    /// The changelog is read from the history and the tags, a candidate folds
    /// into its release, and regenerating it gives the same bytes.
    #[test]
    fn the_changelog_is_read_from_the_history_and_regenerates_byte_for_byte() {
        let repository = a_workspace(
            "changelog",
            &["fix(xtask): read the first fix", "docs: say it"],
        );
        let candidate = repository.head();
        repository.git(&["tag", "v0.1.0-rc.1", &candidate]);
        repository.commit("fix: read the second fix", &[]);
        repository.git(&["tag", "-a", "-m", "the first release", "v0.1.0"]);
        repository.commit("feat(nc-kernel): run a plugin", &[]);
        repository.commit("chore: tidy", &[]);
        let inherited = this_environment();
        let history = Own::of(&repository.root(), &inherited).expect("a repository");
        let tags = super::the_tags(&history).expect("its tags");
        assert_eq!(
            tags.iter()
                .map(|tag| (tag.version.to_string(), tag.merged))
                .collect::<Vec<_>>(),
            [("0.1.0-rc.1".to_owned(), true), ("0.1.0".to_owned(), true)]
        );
        let new = super::Point {
            version: v("0.2.0-rc.1"),
            at: repository.head(),
        };
        let sections = super::the_sections(&history, &tags, &new).expect("the history reads");
        let first = super::the_changelog(&sections);
        let again = super::the_changelog(
            &super::the_sections(&history, &tags, &new).expect("the history reads"),
        );
        assert_eq!(first, again, "the same history gave other bytes");
        let headings: Vec<&str> = first
            .lines()
            .filter(|line| line.starts_with("## "))
            .collect();
        assert_eq!(headings, ["## v0.2.0-rc.1", "## v0.1.0"], "{first}");
        let (newest, oldest) = first
            .split_once("## v0.1.0")
            .expect("the production release has a section");
        assert!(newest.contains("**nc-kernel:** run a plugin"), "{first}");
        assert!(!newest.contains("tidy"), "a chore is listed: {first}");
        for folded in [
            "add the probe",
            "**xtask:** read the first fix",
            "read the second fix",
        ] {
            assert!(
                oldest.contains(folded),
                "`{folded}` is not in v0.1.0's section: {first}"
            );
        }
        assert!(
            !first.contains("say it"),
            "a docs commit is listed: {first}"
        );
        assert!(
            !first.contains("## v0.1.0-rc.1"),
            "a folded candidate kept a section: {first}"
        );
    }

    /// `--dry-run` writes nothing, through the verb itself, and says what the
    /// release would be.
    ///
    /// **The whole tree, not the files the release would write**: a listing
    /// either side holds what the run was not supposed to touch as well as what
    /// it was, and a write that lands elsewhere is exactly what a comparison of
    /// named files cannot see.
    #[test]
    fn a_dry_run_writes_nothing() {
        let repository = a_workspace("dry", &["feat(xtask): something to release"]);
        let before = every_file_under(&repository.root());
        super::run(&repository.root(), Channel::Staging, true, false, false)
            .expect("a dry run of a release that exists");
        super::run(&repository.root(), Channel::Main, true, false, false)
            .expect("a dry run of a release that exists");
        assert_eq!(
            changed_between(&before, &every_file_under(&repository.root())),
            Vec::<String>::new(),
            "`--dry-run` said nothing was written and the tree is not the tree it was given"
        );
    }

    /// A changelog that cannot be written leaves the tree the release was
    /// given.
    ///
    /// **Through `written` itself**, over a workspace whose changelog is
    /// read-only: the manifest is rewritten before the changelog is, and the
    /// refusal puts it back with the rest. The read-only bit refuses a write
    /// to anyone but root, so the test asks first, and fails by name rather
    /// than passing when the bit refuses nothing.
    #[test]
    fn a_changelog_that_cannot_be_written_leaves_the_tree_it_was_given() {
        let repository = a_workspace("unwritable", &[]);
        let root = repository.root();
        let changelog = root.join("CHANGELOG.md");
        let held = "# Changelog\n\nas the tree holds it\n";
        std::fs::write(&changelog, held).expect("a writable scratch");
        let before = every_file_under(&root);
        let inherited = this_environment();
        let was = std::fs::metadata(&changelog)
            .expect("the file is there")
            .permissions();
        let mut read_only = was.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&changelog, read_only).expect("the bit is set");
        // The bytes it holds, so a write the bit lets through changes nothing.
        let the_bit_holds = std::fs::write(&changelog, held).is_err();
        let ran = the_bit_holds.then(|| {
            super::written(
                &a_setting(&root, Channel::Staging, Mode::Write, &inherited, None),
                v("0.2.0"),
                "# Changelog\n\nas the release writes it\n",
            )
        });
        // Off again before anything can panic, so the scratch can be removed.
        std::fs::set_permissions(&changelog, was).expect("the bit comes off");
        assert!(
            the_bit_holds,
            "a read-only file took a write, so this run is root's, which writes past the bit: \
             the changelog's refusal is held only when the tests run as an ordinary user"
        );
        let refused = ran
            .expect("asked above")
            .expect_err("a changelog that cannot be written stops the release");
        assert!(
            refused.contains("CHANGELOG.md could not be written")
                && refused.contains("Nothing was released and the version is back at 0.1.0"),
            "{refused}"
        );
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            Vec::<String>::new(),
            "a release refused at the changelog said nothing was released over a tree it changed"
        );
    }

    /// A changelog that is there and cannot be read, or is not a file, stops
    /// the release before anything is written.
    ///
    /// **Read as no changelog, it is one a failure removes**, so the release
    /// would delete a file it never read. A file its owner may not read is
    /// the first, which the read refuses; the test asks first whether the
    /// mark refuses this user anything, and fails by name rather than passing
    /// when it does not. A directory is the second, which no system reads as
    /// a file, as root or otherwise, and which is refused before any read.
    #[test]
    fn a_changelog_that_cannot_be_read_stops_the_release_before_it_writes() {
        let repository = a_workspace("unreadable", &[]);
        let root = repository.root();
        let changelog = root.join("CHANGELOG.md");
        let inherited = this_environment();
        let a_release = || {
            super::written(
                &a_setting(&root, Channel::Staging, Mode::Write, &inherited, None),
                v("0.2.0"),
                "# Changelog\n",
            )
        };
        std::fs::write(&changelog, "# Changelog\n\nas the tree holds it\n")
            .expect("a writable scratch");
        let before = every_file_under(&root);
        nc_platform::testing::make_unreadable(&changelog).expect("the mark is set");
        let the_mark_holds = std::fs::read(&changelog).is_err();
        let ran = the_mark_holds.then(a_release);
        // Readable again before anything can panic, so the scratch can be removed.
        nc_platform::testing::make_private(&changelog).expect("the mark comes off");
        assert!(
            the_mark_holds,
            "a file its owner may not read was read, so this run is root's, which reads past \
             the mark: the changelog's read is held only when the tests run as an ordinary user"
        );
        let refused = ran
            .expect("asked above")
            .expect_err("a changelog that cannot be read stops the release");
        assert!(
            refused.contains("CHANGELOG.md could not be read, and a release rewrites it")
                && refused.contains("Nothing was written"),
            "{refused}"
        );
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            Vec::<String>::new(),
            "a release refused before it wrote changed the tree"
        );

        std::fs::remove_file(&changelog).expect("a writable scratch");
        std::fs::create_dir_all(changelog.join("kept")).expect("a writable scratch");
        std::fs::write(changelog.join("kept").join("under"), "what is under it\n")
            .expect("a writable scratch");
        let before = every_file_under(&root);
        let refused = a_release().expect_err("a changelog that is a directory stops the release");
        assert!(
            refused.contains("CHANGELOG.md is a directory")
                && refused.ends_with("Nothing was written"),
            "{refused}"
        );
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            Vec::<String>::new(),
            "a release refused before it wrote changed the tree"
        );
    }

    /// A file the release writes that is a link — to a file, or to nothing —
    /// stops the release before anything is written.
    ///
    /// **Each file it writes, each kind of link**: every manifest
    /// `CARRIES_THE_VERSION` names, the lockfile and the changelog, each made
    /// a link to a file of the tree and then a link to nothing. A write would
    /// follow the first to its target and create the second's, and a put-back
    /// would remove the link itself; refused, the target holds what it held and
    /// the file a link to nothing names is not made.
    #[test]
    fn a_file_the_release_writes_that_is_a_link_stops_it_before_it_writes() {
        let repository = a_workspace("linked", &[]);
        let root = repository.root();
        let inherited = this_environment();
        std::fs::create_dir_all(root.join("elsewhere")).expect("a writable scratch");
        for name in CARRIES_THE_VERSION
            .into_iter()
            .chain(["Cargo.lock", super::CHANGELOG])
        {
            let file = root.join(name);
            let kept = std::fs::read(&file).ok();
            let target = root.join("elsewhere").join(name);
            let nowhere = root.join("elsewhere").join(format!("no-{name}"));
            for (kind, points_to) in [("to a file", &target), ("to nothing", &nowhere)] {
                if kind == "to a file" {
                    std::fs::write(&target, kept.as_deref().unwrap_or(b"# Changelog\n"))
                        .expect("a writable scratch");
                }
                let _ = std::fs::remove_file(&file);
                nc_platform::testing::link_file(points_to, &file).expect("a link");
                let before = every_file_under(&root);
                let refused = super::written(
                    &a_setting(&root, Channel::Staging, Mode::Write, &inherited, None),
                    v("0.2.0"),
                    "# Changelog\n",
                )
                .expect_err(kind);
                assert!(
                    refused.starts_with(&format!("{name} is a link"))
                        && refused.ends_with("Nothing was written"),
                    "{name}, a link {kind}: {refused}"
                );
                assert_eq!(
                    changed_between(&before, &every_file_under(&root)),
                    Vec::<String>::new(),
                    "{name}, a link {kind}: a release refused before it wrote changed the tree"
                );
                assert!(
                    std::fs::symlink_metadata(&nowhere).is_err(),
                    "{name}, a link {kind}: the file a link to nothing names was made"
                );
                std::fs::remove_file(&file).expect("a writable scratch");
            }
            if let Some(kept) = kept {
                std::fs::write(&file, kept).expect("a writable scratch");
            }
        }
    }

    /// The verb refuses a release that would repeat one, and one that would
    /// skip the back-merge, before writing anything.
    ///
    /// **The back-merge through `run`, the verb itself**, so what the release
    /// refused is what the verb hands back.
    #[test]
    fn the_verb_refuses_a_release_that_does_not_move_forward() {
        let repository = a_workspace("forward", &[]);
        let first = repository.head();
        repository.git(&["tag", "v0.1.0", &first]);
        repository.commit("fix: a fix after it", &[]);
        let inherited = this_environment();
        let before = every_file_under(&repository.root());
        let root = repository.root();
        // On main, v0.1.1 is the next release; and staging reaches v0.1.0.
        for channel in [Channel::Main, Channel::Staging] {
            super::released(&a_setting(&root, channel, Mode::DryRun, &inherited, None))
                .expect("a release ahead of v0.1.0");
        }
        // A later production tag HEAD does not reach: staging refuses, by name.
        repository.git(&["checkout", "-q", "-b", "elsewhere"]);
        repository.commit("feat: a release made elsewhere", &[]);
        let elsewhere = repository.head();
        repository.git(&["tag", "v0.2.0", &elsewhere]);
        repository.git(&["checkout", "-q", "dev"]);
        let refused = super::run(&root, Channel::Staging, true, false, false)
            .expect_err("v0.2.0 is not merged");
        assert!(refused.contains("staging does not reach it"), "{refused}");
        // And a tag at the version main would release is a refusal.
        repository.git(&["tag", "v0.1.1", &elsewhere]);
        let refused = super::released(&a_setting(
            &root,
            Channel::Main,
            Mode::DryRun,
            &inherited,
            None,
        ))
        .expect_err("v0.1.1 is tagged");
        assert!(refused.contains("v0.1.1 is already tagged"), "{refused}");
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            Vec::<String>::new(),
            "a refused release changed the tree"
        );
    }

    /// The fake `gh` the publishing tests put on `PATH`.
    ///
    /// It records every call — its arguments, the variables of `gh`'s own it
    /// was handed, and what it read — and answers the next fixture in
    /// order.
    const A_FAKE_GH: &str = r#"
use std::io::{Read as _, Write as _};

fn main() {
    let fixtures = std::path::PathBuf::from(std::env::var_os("NC_FAKE_GH").expect("NC_FAKE_GH"));
    let calls = fixtures.join("calls");
    std::fs::create_dir_all(&calls).expect("the calls");
    let nth = std::fs::read_dir(&calls).expect("the calls").count();
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).expect("the input");
    let mut record = String::new();
    for argument in std::env::args().skip(1) {
        record.push_str(&argument);
        record.push('\n');
    }
    for name in ["GH_TOKEN", "GITHUB_TOKEN", "GH_HOST", "GH_DEBUG", "GH_REPO"] {
        let value = std::env::var(name).unwrap_or_else(|_| "unset".to_owned());
        record.push_str(&format!("{name}={value}\n"));
    }
    record.push_str("--\n");
    record.push_str(&String::from_utf8_lossy(&input));
    std::fs::write(calls.join(format!("{nth:03}")), record).expect("the record");
    match std::fs::read_to_string(fixtures.join("answers").join(format!("{nth:03}"))) {
        Ok(answer) => {
            let (code, out) = answer.split_once('\n').unwrap_or((&answer, ""));
            std::io::stdout().write_all(out.as_bytes()).expect("the answer");
            std::process::exit(code.trim().parse().unwrap_or(1));
        }
        Err(_) => {
            std::io::stderr().write_all(b"no answer for this call\n").expect("the refusal");
            std::process::exit(1);
        }
    }
}
"#;

    /// The fake `gh`, compiled into a directory of its own, and that directory.
    ///
    /// **Compiled, not scripted**, so it runs on all three systems: Windows
    /// finds a program on `PATH` by its `.exe` and nothing else.
    fn a_fake_gh() -> (LeavesNothingBehind, PathBuf) {
        let held = LeavesNothingBehind::new("release-fake-gh");
        let source = held.root.join("fake_gh.rs");
        std::fs::write(&source, A_FAKE_GH).expect("a writable scratch");
        let bin = held.root.join("bin");
        std::fs::create_dir_all(&bin).expect("a writable scratch");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        #[expect(
            clippy::disallowed_methods,
            reason = "the compiler builds the fake this test publishes through; its output is read"
        )]
        let built = std::process::Command::new(&rustc)
            .arg("--edition=2021")
            .arg("-o")
            .arg(bin.join(format!("gh{}", std::env::consts::EXE_SUFFIX)))
            .arg(&source)
            .current_dir(root())
            .output()
            .expect("rustc, which compiled this test, runs");
        assert!(
            built.status.success(),
            "the fake gh did not compile: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        (held, bin)
    }

    /// One call the fake recorded.
    #[derive(Debug)]
    struct Call {
        /// Its arguments.
        arguments: Vec<String>,
        /// The variables of `gh`'s own it was handed, as `NAME=value`.
        handed: Vec<String>,
        /// What it read on its standard input, as JSON.
        request: serde_json::Value,
    }

    /// A GitHub for the release to publish to through the fake in `bin`,
    /// answering `answers` in order, with its fixtures in `fixtures`.
    ///
    /// **The environment plants the variables the release must not hand
    /// on** — the workflow's own token, a debug switch, another repository —
    /// so the calls the fake records say whether each was taken out.
    fn a_github(
        bin: &Path,
        fixtures: &Path,
        answers: &[String],
    ) -> (GitHub, Vec<(OsString, OsString)>) {
        let _ = std::fs::remove_dir_all(fixtures);
        std::fs::create_dir_all(fixtures.join("answers")).expect("a writable scratch");
        for (nth, answer) in answers.iter().enumerate() {
            std::fs::write(fixtures.join("answers").join(format!("{nth:03}")), answer)
                .expect("a writable scratch");
        }
        let mut inherited: Vec<(OsString, OsString)> = this_environment()
            .into_iter()
            .filter(|(name, _)| {
                !super::ghs_own(name) && name != "PATH" && name != "GITHUB_REPOSITORY"
            })
            .collect();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let searched = std::env::join_paths(
            std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path)),
        )
        .expect("a PATH");
        inherited.extend([
            ("PATH".into(), searched),
            ("NC_FAKE_GH".into(), fixtures.as_os_str().to_owned()),
            ("GITHUB_REPOSITORY".into(), "noctua-world/probe".into()),
            ("GH_TOKEN".into(), "the-apps-token".into()),
            ("GITHUB_TOKEN".into(), "the-workflows-token".into()),
            ("GH_DEBUG".into(), "api".into()),
            ("GH_REPO".into(), "someone/else".into()),
        ]);
        (GitHub::of(&inherited).expect("a whole setting"), inherited)
    }

    /// Every call the fake recorded, in order.
    fn the_calls(fixtures: &Path) -> Vec<Call> {
        let mut named: Vec<PathBuf> = std::fs::read_dir(fixtures.join("calls"))
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default();
        named.sort();
        named
            .iter()
            .map(|path| {
                let text = std::fs::read_to_string(path).expect("a record");
                let (head, input) = text.split_once("--\n").expect("a record's two halves");
                let lines: Vec<String> = head.lines().map(str::to_owned).collect();
                let split = lines.len() - 5;
                // **Nothing, from a child that is not `gh`**: the gate and the
                // relock, started at the fake, are handed no request.
                let request = if input.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::from_str(input).expect("a JSON request")
                };
                Call {
                    arguments: lines[..split].to_vec(),
                    handed: lines[split..].to_vec(),
                    request,
                }
            })
            .collect()
    }

    /// What each call asked for: the endpoint, and the operation a GraphQL
    /// request names.
    fn asked(calls: &[Call]) -> Vec<String> {
        calls
            .iter()
            .map(|call| {
                let endpoint = call.arguments.get(5).cloned().unwrap_or_default();
                match call
                    .request
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                {
                    Some(query) => query
                        .split(['(', ' '])
                        .nth(1)
                        .unwrap_or_default()
                        .to_owned(),
                    None => endpoint,
                }
            })
            .collect()
    }

    /// An answer the fake gives: success, and `json`.
    fn answered(json: &serde_json::Value) -> String {
        format!("0\n{json}")
    }

    /// GitHub's answer to what it holds of a release.
    fn holding(tag: Option<&str>, release: bool, carried: Option<(&str, bool)>) -> String {
        holding_titled(
            tag,
            release,
            carried.map(|(state, auto)| {
                ("chore(release): carry v0.1.0-rc.1 back to dev", state, auto)
            }),
        )
    }

    /// GitHub's answer to what it holds of a release, with the pull request
    /// from the branch to the one below titled as `carried` says.
    fn holding_titled(
        tag: Option<&str>,
        release: bool,
        carried: Option<(&str, &str, bool)>,
    ) -> String {
        holding_these(
            tag,
            release,
            &carried.map_or_else(Vec::new, |(title, state, auto)| {
                vec![a_pull_request(title, state, auto)]
            }),
        )
    }

    /// GitHub's answer to what it holds of a release, with `nodes` as the pull
    /// requests from a branch named as the released one to the one below.
    fn holding_these(tag: Option<&str>, release: bool, nodes: &[serde_json::Value]) -> String {
        answered(&serde_json::json!({ "data": { "repository": {
            "ref": tag.map(|oid| serde_json::json!({ "target": { "oid": oid } })),
            "release": release.then(|| serde_json::json!({ "tagName": "v0.1.0-rc.1" })),
            "pullRequests": { "nodes": nodes },
        } } }))
    }

    /// This repository's pull request #7 from `staging` to `dev`, with the
    /// title `title` and required checks that have not passed yet.
    fn a_pull_request(title: &str, state: &str, auto: bool) -> serde_json::Value {
        serde_json::json!({
            "number": 7,
            "id": "PR_seven",
            "title": title,
            "state": state,
            "isCrossRepository": false,
            "headRepository": { "nameWithOwner": "noctua-world/probe" },
            "mergeStateStatus": "BLOCKED",
            "autoMergeRequest": auto.then(|| serde_json::json!({ "mergeMethod": "MERGE" })),
        })
    }

    /// A stranger's pull request #9, from a branch named `staging` in a fork
    /// into `dev`, titled as the release and ready to merge, its head
    /// repository named or gone with the fork.
    fn a_forks_pull_request(state: &str, from: Option<&str>) -> serde_json::Value {
        let mut node = a_pull_request(
            "chore(release): carry v0.1.0-rc.1 back to dev",
            state,
            false,
        );
        node["number"] = 9.into();
        node["id"] = "PR_nine".into();
        node["isCrossRepository"] = true.into();
        node["headRepository"] = from.map_or(
            serde_json::Value::Null,
            |named| serde_json::json!({ "nameWithOwner": named }),
        );
        node["mergeStateStatus"] = "CLEAN".into();
        node
    }

    /// GitHub's answers to each step after the question, in order.
    fn the_steps_answered(at: &str) -> [String; 4] {
        the_steps_of("v0.1.0-rc.1", "dev", at)
    }

    /// GitHub's answers to each step after the question, in order.
    ///
    /// **Of the release tagged `tag` at `at`**, whose pull request carries it
    /// back to `below`.
    fn the_steps_of(tag: &str, below: &str, at: &str) -> [String; 4] {
        [
            answered(
                &serde_json::json!({ "ref": format!("refs/tags/{tag}"), "object": { "sha": at, "type": "commit" } }),
            ),
            answered(&serde_json::json!({ "tag_name": tag, "prerelease": below == "dev" })),
            // REST's shape: `id` is a number, and the node is `node_id`.
            answered(
                &serde_json::json!({ "number": 7, "id": 5_147_253_185_u64, "node_id": "PR_seven", "title": format!("chore(release): carry {tag} back to {below}"), "state": "open" }),
            ),
            answered(
                &serde_json::json!({ "data": { "enablePullRequestAutoMerge": { "pullRequest": { "number": 7, "autoMergeRequest": { "mergeMethod": "MERGE" } } } } }),
            ),
        ]
    }

    /// GitHub's answer to whether `dev` holds a release commit: `status`, as
    /// a comparison of the two says it.
    fn the_branch_below(status: &str) -> String {
        answered(
            &serde_json::json!({ "data": { "repository": { "ref": { "compare": { "status": status } } } } }),
        )
    }

    /// The release of `root` on `staging`, published through the GitHub
    /// [`a_github`] made.
    fn published_on_staging(
        root: &Path,
        (github, inherited): &(GitHub, Vec<(OsString, OsString)>),
    ) -> Result<(), String> {
        super::released(&a_setting(
            root,
            Channel::Staging,
            Mode::Publish,
            inherited,
            Some(github),
        ))
    }

    /// The release is published in order, through the App's token alone.
    ///
    /// A run on the release commit carries it on from each step it stopped
    /// after, and makes nothing twice.
    #[test]
    fn the_release_is_published_in_order_and_carried_on_after_each_step() {
        let (fake, bin) = a_fake_gh();
        let repository = a_workspace("publish", &["fix(xtask): something to release"]);
        let root = repository.root();
        let fixtures = fake.root.join("fixtures");
        let made = "1".repeat(40);
        published_whole(&repository, &bin, &fixtures, &made);
        // **Carried on from the release commit**, which the push it causes
        // runs the release on: each run asks first, then makes only what is
        // missing.
        repository.git(&["add", "-A"]);
        repository.commit("chore(release): v0.1.0-rc.1\n", &[]);
        carried_on_after_each_step(&root, &repository.head(), &bin, &fixtures);
        what_github_holds_is_judged_before_anything_is_made(&root, &bin, &fixtures, &made);
        each_steps_answer_is_held_to_the_shape_asked(&root, &repository.head(), &bin, &fixtures);
        each_merge_answer_is_held_to_the_shape_asked(&root, &repository.head(), &bin, &fixtures);
        the_branch_below_is_held_to_the_shape_asked(&root, &repository.head(), &bin, &fixtures);
        the_answer_to_the_question_is_held_to_the_shape_asked(&root, &bin, &fixtures);
        a_pull_request_back_that_cannot_merge_says_how_to_by_hand(
            &root,
            &repository.head(),
            &bin,
            &fixtures,
        );
        a_release_commit_not_as_written_is_not_carried_on(&repository, &bin, &fixtures);
    }

    /// A tag at another commit, a pull request closed without merging, and an
    /// answer that does not say where a pull request's head is or how close
    /// it is to merging, are refused before anything is made.
    fn what_github_holds_is_judged_before_anything_is_made(
        root: &Path,
        bin: &Path,
        fixtures: &Path,
        made: &str,
    ) {
        let mut unplaced = a_pull_request("chore: anything", "OPEN", false);
        unplaced
            .as_object_mut()
            .expect("a pull request is an object")
            .remove("isCrossRepository");
        let mut misplaced = a_pull_request("chore: anything", "OPEN", false);
        misplaced["headRepository"]["nameWithOwner"] = "someone/else".into();
        let mut unweighed = a_pull_request("chore: anything", "OPEN", false);
        unweighed["mergeStateStatus"] = serde_json::Value::Null;
        for (what, holds, refusal) in [
            (
                "a tag elsewhere",
                holding(Some(made), false, None),
                "is tagged at 1111111",
            ),
            (
                "a closed pull request",
                holding(None, false, Some(("CLOSED", false))),
                "was closed without merging",
            ),
            (
                "a pull request with no word of a fork",
                holding_these(None, false, &[unplaced]),
                "without saying whether its head is in a fork",
            ),
            (
                "a pull request of this repository's headed elsewhere",
                holding_these(None, false, &[misplaced]),
                "with its head in someone/else",
            ),
            (
                "a pull request with no merge state",
                holding_these(None, false, &[unweighed]),
                "with no merge state",
            ),
        ] {
            let refused =
                published_on_staging(root, &a_github(bin, fixtures, &[holds])).expect_err(what);
            assert!(refused.contains(refusal), "{what}: {refused}");
            assert_eq!(the_calls(fixtures).len(), 1, "{what}: something was made");
        }
    }

    /// Each step GitHub answers with success in another shape is refused
    /// there.
    ///
    /// **By its own sentence, and nothing is asked after it**: a tag at another
    /// commit or of another name, another release, and another pull request or
    /// one that does not say its number or its node. The merge's steps are
    /// [`each_merge_answer_is_held_to_the_shape_asked`]'s.
    fn each_steps_answer_is_held_to_the_shape_asked(
        root: &Path,
        release_commit: &str,
        bin: &Path,
        fixtures: &Path,
    ) {
        let at = Some(release_commit);
        let title = "chore(release): carry v0.1.0-rc.1 back to dev";
        // **No pull request carries it, and `dev` does not hold it**: the two
        // answers before the tag, the release or the pull request is made.
        let unheld = |tag: Option<&str>, release: bool| {
            vec![holding_these(tag, release, &[]), the_branch_below("AHEAD")]
        };
        let elsewhere = "2".repeat(40);
        // REST's shape: `id` is a number, and the node is `node_id`.
        let id = 5_147_253_185_u64;
        let (refs, releases, pulls) = (
            "repos/noctua-world/probe/git/refs",
            "repos/noctua-world/probe/releases",
            "repos/noctua-world/probe/pulls",
        );
        for (what, before, answer, step, refusal) in [
            (
                "a tag at another commit",
                unheld(None, false),
                serde_json::json!({ "ref": "refs/tags/v0.1.0-rc.1", "object": { "sha": elsewhere } }),
                refs,
                "GitHub answered the tag v0.1.0-rc.1 with another tag, or another commit",
            ),
            (
                "another tag",
                unheld(None, false),
                serde_json::json!({ "ref": "refs/tags/v0.1.0-rc.2", "object": { "sha": release_commit } }),
                refs,
                "GitHub answered the tag v0.1.0-rc.1 with another tag, or another commit",
            ),
            (
                "another release",
                unheld(at, false),
                serde_json::json!({ "tag_name": "v0.1.0-rc.2", "prerelease": true }),
                releases,
                "GitHub answered the release of v0.1.0-rc.1 with another release",
            ),
            (
                "another pull request",
                unheld(at, true),
                serde_json::json!({ "number": 8, "id": id, "node_id": "PR_eight", "title": "chore: something else", "state": "open" }),
                pulls,
                "GitHub answered the pull request that carries v0.1.0-rc.1 back with another one",
            ),
            (
                "a pull request with no number",
                unheld(at, true),
                serde_json::json!({ "id": id, "node_id": "PR_seven", "title": title, "state": "open" }),
                pulls,
                "GitHub answered a pull request with no number",
            ),
            (
                "a pull request with no node, its id a number",
                unheld(at, true),
                serde_json::json!({ "number": 7, "id": id, "title": title, "state": "open" }),
                pulls,
                "GitHub answered pull request #7 with no node",
            ),
        ] {
            let mut expected = ["WhatGitHubHolds", "HeldBelow"][..before.len()].to_vec();
            expected.push(step);
            let mut answers = before;
            answers.push(answered(&answer));
            let refused =
                published_on_staging(root, &a_github(bin, fixtures, &answers)).expect_err(what);
            assert!(refused.contains(refusal), "{what}: {refused}");
            assert_eq!(
                asked(&the_calls(fixtures)),
                expected,
                "{what}: the release went on past a step GitHub answered in another shape"
            );
        }
    }

    /// Auto-merge or the merge, answered by GitHub with success in another
    /// shape, is refused there.
    ///
    /// **By its own sentence**: auto-merge by another method or not on, and a
    /// merge GitHub does not say happened.
    fn each_merge_answer_is_held_to_the_shape_asked(
        root: &Path,
        release_commit: &str,
        bin: &Path,
        fixtures: &Path,
    ) {
        let standing = |status: &str| {
            let mut node = a_pull_request(
                "chore(release): carry v0.1.0-rc.1 back to dev",
                "OPEN",
                false,
            );
            node["mergeStateStatus"] = status.into();
            holding_these(Some(release_commit), true, &[node])
        };
        for (what, holds, answer, step, refusal) in [
            (
                "auto-merge by squashing",
                standing("BLOCKED"),
                serde_json::json!({ "data": { "enablePullRequestAutoMerge": { "pullRequest": { "number": 7, "autoMergeRequest": { "mergeMethod": "SQUASH" } } } } }),
                "AutoMerge",
                "GitHub did not answer pull request #7 with auto-merge on, by merge commit",
            ),
            (
                "auto-merge not on",
                standing("BLOCKED"),
                serde_json::json!({ "data": { "enablePullRequestAutoMerge": { "pullRequest": { "number": 7, "autoMergeRequest": null } } } }),
                "AutoMerge",
                "GitHub did not answer pull request #7 with auto-merge on, by merge commit",
            ),
            (
                "a merge that did not happen",
                standing("CLEAN"),
                serde_json::json!({ "data": { "mergePullRequest": { "pullRequest": { "number": 7, "merged": false } } } }),
                "MergedNow",
                "GitHub did not answer pull request #7 as merged",
            ),
            (
                "a merge answered with nothing",
                standing("CLEAN"),
                serde_json::json!({ "data": { "mergePullRequest": null } }),
                "MergedNow",
                "GitHub did not answer pull request #7 as merged",
            ),
        ] {
            let refused =
                published_on_staging(root, &a_github(bin, fixtures, &[holds, answered(&answer)]))
                    .expect_err(what);
            assert!(refused.contains(refusal), "{what}: {refused}");
            assert_eq!(
                asked(&the_calls(fixtures)),
                ["WhatGitHubHolds", step],
                "{what}: the release went on past a step GitHub answered in another shape"
            );
        }
    }

    /// GitHub's answer to whether the branch below holds the release commit
    /// is one of the four a comparison is, or it is refused.
    ///
    /// **No branch below, a comparison GitHub could not make, and a word that
    /// is none of the four**, each with nothing made after the question.
    fn the_branch_below_is_held_to_the_shape_asked(
        root: &Path,
        release_commit: &str,
        bin: &Path,
        fixtures: &Path,
    ) {
        for (what, answer) in [
            (
                "no branch below",
                serde_json::json!({ "data": { "repository": { "ref": null } } }),
            ),
            (
                "a comparison GitHub could not make",
                serde_json::json!({ "data": { "repository": { "ref": { "compare": null } } } }),
            ),
            (
                "a comparison in a word that is none of the four",
                serde_json::json!({ "data": { "repository": { "ref": { "compare": { "status": "UNRELATED" } } } } }),
            ),
        ] {
            let holds = holding_these(Some(release_commit), true, &[]);
            let refused =
                published_on_staging(root, &a_github(bin, fixtures, &[holds, answered(&answer)]))
                    .expect_err(what);
            assert!(
                refused.contains(&format!(
                    "GitHub did not say whether dev holds {}",
                    &release_commit[..7]
                )),
                "{what}: {refused}"
            );
            assert_eq!(
                asked(&the_calls(fixtures)),
                ["WhatGitHubHolds", "HeldBelow"],
                "{what}: the release went on past a question GitHub answered in another shape"
            );
        }
    }

    /// An answer to the question that does not say what GitHub holds is
    /// refused before anything is made.
    ///
    /// **No repository, no list of pull requests, a tag naming no commit,
    /// and a pull request neither open, merged nor closed**, and from `gh`
    /// an answer that is not JSON or that carries GitHub's errors.
    fn the_answer_to_the_question_is_held_to_the_shape_asked(
        root: &Path,
        bin: &Path,
        fixtures: &Path,
    ) {
        let mut drafted = a_pull_request("chore: anything", "DRAFT", false);
        drafted["mergeStateStatus"] = "CLEAN".into();
        for (what, answer, refusal) in [
            (
                "no repository",
                answered(&serde_json::json!({ "data": { "repository": null } })),
                "GitHub answered no repository noctua-world/probe for the token the release holds",
            ),
            (
                "no list of pull requests",
                answered(
                    &serde_json::json!({ "data": { "repository": { "ref": null, "release": null } } }),
                ),
                "GitHub answered no list of pull requests",
            ),
            (
                "a tag naming no commit",
                answered(
                    &serde_json::json!({ "data": { "repository": { "ref": { "target": {} }, "release": null, "pullRequests": { "nodes": [] } } } }),
                ),
                "GitHub named the tag v0.1.0-rc.1 with no commit",
            ),
            (
                "a pull request neither open, merged nor closed",
                holding_these(None, false, &[drafted]),
                "GitHub answered pull request #7 as DRAFT, which is not open, merged or closed",
            ),
            (
                "an answer that is not JSON",
                "0\nnot an answer".to_owned(),
                "`gh api graphql` answered something that is not JSON",
            ),
            (
                "GitHub's errors",
                answered(&serde_json::json!({ "errors": [{ "message": "refused" }] })),
                "`gh api graphql` answered with GitHub's errors",
            ),
        ] {
            let refused =
                published_on_staging(root, &a_github(bin, fixtures, &[answer])).expect_err(what);
            assert!(refused.contains(refusal), "{what}: {refused}");
            assert_eq!(the_calls(fixtures).len(), 1, "{what}: something was made");
        }
    }

    /// A run on a release commit refuses what this verb would not have written.
    ///
    /// A tree holding what no commit has, a manifest at another version and
    /// another changelog are each refused before GitHub is asked anything.
    fn a_release_commit_not_as_written_is_not_carried_on(
        repository: &ARepository,
        bin: &Path,
        fixtures: &Path,
    ) {
        let root = repository.root();
        let note = root.join("probe/notes.txt");
        std::fs::write(&note, "a note\n").expect("a writable scratch");
        let refused = published_on_staging(&root, &a_github(bin, fixtures, &[]))
            .expect_err("a tree with a file no commit has");
        assert!(
            refused.contains("the tree holds changes no commit has")
                && refused.contains("probe/notes.txt"),
            "{refused}"
        );
        assert!(
            the_calls(fixtures).is_empty(),
            "GitHub was asked about a tree no commit holds"
        );
        std::fs::remove_file(&note).expect("a writable scratch");
        // **A release commit whose manifest carries another version is not
        // carried on.**
        let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("the manifest");
        std::fs::write(
            root.join("Cargo.toml"),
            manifest.replace("version = \"0.1.0-rc.1\"", "version = \"0.1.0\""),
        )
        .expect("a writable scratch");
        repository.git(&["add", "-A"]);
        repository.git(&[
            "commit",
            "-q",
            "--amend",
            "--no-verify",
            "-m",
            "chore(release): v0.1.0-rc.1",
        ]);
        let refused = published_on_staging(&root, &a_github(bin, fixtures, &[]))
            .expect_err("another version");
        assert!(
            refused.contains("and Cargo.toml carries 0.1.0."),
            "{refused}"
        );
        assert!(
            the_calls(fixtures).is_empty(),
            "GitHub was asked about a release refused"
        );
        std::fs::write(root.join("Cargo.toml"), manifest).expect("a writable scratch");
        // **A release commit holding another changelog is not carried on.**
        std::fs::write(root.join(super::CHANGELOG), "# Changelog\n").expect("a writable scratch");
        repository.git(&["add", "-A"]);
        repository.git(&[
            "commit",
            "-q",
            "--amend",
            "--no-verify",
            "-m",
            "chore(release): v0.1.0-rc.1",
        ]);
        let refused = published_on_staging(&root, &a_github(bin, fixtures, &[]))
            .expect_err("another changelog");
        assert!(
            refused.contains("is not the changelog the history gives"),
            "{refused}"
        );
        assert!(
            the_calls(fixtures).is_empty(),
            "GitHub was asked about a release refused"
        );
    }

    /// A release made whole, each step asked as it is written down.
    ///
    /// The question, the commit, the tag, the release, the pull request back
    /// and auto-merge on it, with the App's token and nothing of `gh`'s own
    /// beside it.
    fn published_whole(repository: &ARepository, bin: &Path, fixtures: &Path, made: &str) {
        let root = repository.root();
        let head = repository.head();
        let mut answers = vec![
            holding(None, false, None),
            answered(
                &serde_json::json!({ "data": { "createCommitOnBranch": { "commit": { "oid": made } } } }),
            ),
        ];
        answers.extend(the_steps_answered(made));
        published_on_staging(&root, &a_github(bin, fixtures, &answers))
            .expect("a release published whole");
        let calls = the_calls(fixtures);
        assert_eq!(
            asked(&calls),
            [
                "WhatGitHubHolds",
                "ReleaseCommit",
                "repos/noctua-world/probe/git/refs",
                "repos/noctua-world/probe/releases",
                "repos/noctua-world/probe/pulls",
                "AutoMerge",
            ]
        );
        for call in &calls {
            assert_eq!(
                call.arguments[..5],
                ["api", "--hostname", "github.com", "--method", "POST"],
                "{call:?}"
            );
            assert_eq!(
                call.handed,
                [
                    "GH_TOKEN=the-apps-token",
                    "GITHUB_TOKEN=unset",
                    "GH_HOST=github.com",
                    "GH_DEBUG=unset",
                    "GH_REPO=unset",
                ],
                "gh was handed another token, or a variable of its own the release takes out"
            );
            assert!(
                !call
                    .arguments
                    .iter()
                    .any(|argument| argument.contains("the-apps-token")),
                "the token is an argument, which a process listing shows: {call:?}"
            );
        }
        let input = &calls[1].request["variables"]["input"];
        assert_eq!(
            input["expectedHeadOid"],
            head.as_str(),
            "the commit the job checked out"
        );
        assert_eq!(input["branch"]["branchName"], "staging");
        assert_eq!(
            input["branch"]["repositoryNameWithOwner"],
            "noctua-world/probe"
        );
        assert_eq!(input["message"]["headline"], "chore(release): v0.1.0-rc.1");
        the_files_committed_are_the_files_written(
            &root,
            &input["fileChanges"]["additions"],
            "0.1.0-rc.1",
        );
        assert_eq!(
            calls[2].request,
            serde_json::json!({ "ref": "refs/tags/v0.1.0-rc.1", "sha": made })
        );
        assert_eq!(calls[3].request["prerelease"], true);
        assert_eq!(calls[3].request["make_latest"], "false");
        let notes = calls[3].request["body"].as_str().unwrap_or_default();
        assert!(
            notes.starts_with("### Added\n\n- add the probe (")
                && notes.contains("### Fixed\n\n- **xtask:** something to release ("),
            "the notes are not the release's section: {notes}"
        );
        assert_eq!(calls[4].request["head"], "staging");
        assert_eq!(calls[4].request["base"], "dev");
        assert_eq!(calls[5].request["variables"]["id"], "PR_seven");
    }

    /// The files a release commit carries are the ones it writes, as the tree
    /// holds them.
    ///
    /// The changelog, the lockfile, and every file `CARRIES_THE_VERSION`
    /// names, each listed one carrying the release, `version`.
    fn the_files_committed_are_the_files_written(
        root: &Path,
        additions: &serde_json::Value,
        version: &str,
    ) {
        let additions = additions.as_array().expect("additions");
        let carried: Vec<&str> = additions
            .iter()
            .map(|addition| addition["path"].as_str().unwrap_or_default())
            .collect();
        let mut written: Vec<&str> = [super::CHANGELOG, "Cargo.lock"]
            .into_iter()
            .chain(CARRIES_THE_VERSION)
            .collect();
        written.sort_unstable();
        assert_eq!(
            carried, written,
            "the release commit does not carry every file the release wrote"
        );
        for addition in additions {
            let path = addition["path"].as_str().unwrap_or_default();
            assert_eq!(
                addition["contents"],
                super::base64(&std::fs::read(root.join(path)).expect("written")).as_str(),
                "{path} was committed as other bytes than the tree holds"
            );
        }
        for name in CARRIES_THE_VERSION {
            assert!(
                std::fs::read_to_string(root.join(name))
                    .expect("written")
                    .contains(&format!("version = \"{version}\"")),
                "{name} does not carry {version}"
            );
        }
    }

    /// A run on the release commit at `release_commit` asks what GitHub
    /// holds, and makes only what follows the step it stopped after.
    ///
    /// **Only this repository's own pull request is ever merged or queued**: a
    /// fork's from a branch of the same name is passed over, open or closed,
    /// and the release opens its own.
    fn carried_on_after_each_step(root: &Path, release_commit: &str, bin: &Path, fixtures: &Path) {
        for (stopped_after, holds, missing, expected) in the_runs_carried_on(release_commit)
            .into_iter()
            .chain(the_runs_beside_an_earlier_release(release_commit))
            .chain(the_runs_by_merge_state(release_commit))
        {
            let mut answers = vec![holds];
            answers.extend(missing);
            published_on_staging(root, &a_github(bin, fixtures, &answers))
                .unwrap_or_else(|error| panic!("carried on after {stopped_after}: {error}"));
            let calls = the_calls(fixtures);
            let made = asked(&calls);
            assert_eq!(
                made.first().map(String::as_str),
                Some("WhatGitHubHolds"),
                "carried on after {stopped_after}"
            );
            assert_eq!(
                made[1..],
                expected,
                "carried on after {stopped_after}, the release asked {made:?}"
            );
            for (call, operation) in calls.iter().zip(&made) {
                if operation.ends_with("/git/refs") {
                    assert_eq!(
                        call.request["sha"], release_commit,
                        "the tag is not at HEAD"
                    );
                }
                if operation == "HeldBelow" {
                    assert_eq!(
                        (
                            &call.request["variables"]["below"],
                            &call.request["variables"]["at"]
                        ),
                        (&"refs/heads/dev".into(), &release_commit.into()),
                        "carried on after {stopped_after}, the release asked whether another \
                         branch holds another commit"
                    );
                }
                if operation == "AutoMerge" || operation == "MergedNow" {
                    assert_eq!(
                        call.request["variables"]["id"], "PR_seven",
                        "carried on after {stopped_after}, {operation} was asked for a pull \
                         request that is not the release's"
                    );
                }
            }
        }
    }

    /// Each run carried on from the release commit at `release_commit`.
    ///
    /// What it stopped after, what GitHub holds, the answers to what it then
    /// asks and makes, and what it asks for after the question. **With no pull
    /// request carrying it, it asks whether `dev` holds it**, and carries it
    /// back only when `dev` does not. An earlier release's pull request is
    /// [`the_runs_beside_an_earlier_release`]'s.
    fn the_runs_carried_on(
        release_commit: &str,
    ) -> Vec<(String, String, Vec<String>, Vec<String>)> {
        let [tag, release, opened, queued] = the_steps_answered(release_commit);
        let ours = |state: &str, auto: bool| {
            a_pull_request("chore(release): carry v0.1.0-rc.1 back to dev", state, auto)
        };
        let stranger = a_forks_pull_request("OPEN", Some("stranger/probe"));
        let at = Some(release_commit);
        let (refs, releases, pulls) = (
            "repos/noctua-world/probe/git/refs",
            "repos/noctua-world/probe/releases",
            "repos/noctua-world/probe/pulls",
        );
        let unheld = the_branch_below("AHEAD");
        let carried_back = vec![unheld.clone(), opened.clone(), queued.clone()];
        let and_carried_back = vec!["HeldBelow", pulls, "AutoMerge"];
        owned(vec![
            (
                "the commit",
                holding_these(None, false, &[]),
                vec![
                    unheld.clone(),
                    tag.clone(),
                    release.clone(),
                    opened.clone(),
                    queued.clone(),
                ],
                vec!["HeldBelow", refs, releases, pulls, "AutoMerge"],
            ),
            (
                "the tag",
                holding_these(at, false, &[]),
                vec![unheld, release.clone(), opened, queued.clone()],
                vec!["HeldBelow", releases, pulls, "AutoMerge"],
            ),
            (
                "the release",
                holding_these(at, true, &[]),
                carried_back.clone(),
                and_carried_back.clone(),
            ),
            (
                "the pull request",
                holding_these(at, true, &[ours("OPEN", false)]),
                vec![queued.clone()],
                vec!["AutoMerge"],
            ),
            (
                "auto-merge",
                holding_these(at, true, &[ours("OPEN", true)]),
                vec![],
                vec![],
            ),
            (
                "the merge",
                holding_these(at, true, &[ours("MERGED", false)]),
                vec![],
                vec![],
            ),
            // **A fork's, open and ready to merge, is not this release's**,
            // nor is one closed under this release's title.
            (
                "a fork's pull request, open and ready",
                holding_these(at, true, std::slice::from_ref(&stranger)),
                carried_back.clone(),
                and_carried_back.clone(),
            ),
            (
                "a fork's pull request, closed under the release's title, the fork deleted",
                holding_these(at, true, &[a_forks_pull_request("CLOSED", None)]),
                carried_back,
                and_carried_back,
            ),
            (
                "a fork's pull request beside the release's own",
                holding_these(at, true, &[stranger, ours("OPEN", false)]),
                vec![queued],
                vec!["AutoMerge"],
            ),
        ])
    }

    /// Each run carried on from the release commit at `release_commit` with an
    /// earlier release's pull request from `staging` to `dev`.
    ///
    /// **Open, it carries this release** — its head is the branch. **Merged
    /// before the release commit, it does not**, and the release carries it
    /// back, whether `dev` stopped there or moved on since; **merged since, at
    /// the release commit or past it, it did**: `dev` holds it, and the release
    /// opens nothing.
    fn the_runs_beside_an_earlier_release(
        release_commit: &str,
    ) -> Vec<(String, String, Vec<String>, Vec<String>)> {
        let [tag, release, opened, queued] = the_steps_answered(release_commit);
        let earlier = |state: &str| {
            a_pull_request(
                "chore(release): carry v0.0.9-rc.1 back to dev",
                state,
                false,
            )
        };
        let at = Some(release_commit);
        let (refs, releases, pulls) = (
            "repos/noctua-world/probe/git/refs",
            "repos/noctua-world/probe/releases",
            "repos/noctua-world/probe/pulls",
        );
        owned(vec![
            (
                "an earlier release's pull request, still open",
                holding_these(at, true, &[earlier("OPEN")]),
                vec![queued.clone()],
                vec!["AutoMerge"],
            ),
            (
                "an earlier release's pull request, merged before the release commit",
                holding_these(at, true, &[earlier("MERGED")]),
                vec![the_branch_below("AHEAD"), opened.clone(), queued.clone()],
                vec!["HeldBelow", pulls, "AutoMerge"],
            ),
            (
                "an earlier release's pull request, merged before the release commit, the \
                 branch below moved on since",
                holding_these(at, true, &[earlier("MERGED")]),
                vec![the_branch_below("DIVERGED"), opened, queued.clone()],
                vec!["HeldBelow", pulls, "AutoMerge"],
            ),
            (
                "an earlier release's pull request, merged since at the release commit",
                holding_these(at, true, &[earlier("MERGED")]),
                vec![the_branch_below("IDENTICAL")],
                vec!["HeldBelow"],
            ),
            (
                "an earlier release's pull request, merged since past the release commit",
                holding_these(at, true, &[earlier("MERGED")]),
                vec![the_branch_below("BEHIND")],
                vec!["HeldBelow"],
            ),
            (
                "the commit, an earlier release's pull request merged since",
                holding_these(None, false, &[earlier("MERGED")]),
                vec![the_branch_below("BEHIND"), tag, release],
                vec!["HeldBelow", refs, releases],
            ),
        ])
    }

    /// Each run carried on from the release commit at `release_commit` with
    /// the release's own pull request open, by how close GitHub says it is to
    /// merging.
    ///
    /// **Checks that finished before auto-merge was asked for**: GitHub refuses
    /// auto-merge there, so the pull request is merged — passed clean, with
    /// hooks, or with the required checks passed beside others that failed.
    /// **And one GitHub would not merge now is queued**: behind the branch
    /// below. One in conflict with it is refused, saying how to carry the
    /// release back by hand (`a_release_that_cannot_be_carried_back_says_how_to_by_hand`).
    fn the_runs_by_merge_state(
        release_commit: &str,
    ) -> Vec<(String, String, Vec<String>, Vec<String>)> {
        let [_, _, _, queued] = the_steps_answered(release_commit);
        let merged = answered(
            &serde_json::json!({ "data": { "mergePullRequest": { "pullRequest": { "number": 7, "merged": true } } } }),
        );
        let at = Some(release_commit);
        let standing = |status: &str| {
            let mut node = a_pull_request(
                "chore(release): carry v0.1.0-rc.1 back to dev",
                "OPEN",
                false,
            );
            node["mergeStateStatus"] = status.into();
            node
        };
        owned(vec![
            (
                "the pull request's checks, passed",
                holding_these(at, true, &[standing("CLEAN")]),
                vec![merged.clone()],
                vec!["MergedNow"],
            ),
            (
                "the pull request's checks, passed with hooks",
                holding_these(at, true, &[standing("HAS_HOOKS")]),
                vec![merged.clone()],
                vec!["MergedNow"],
            ),
            (
                "the pull request's required checks, passed beside others that failed",
                holding_these(at, true, &[standing("UNSTABLE")]),
                vec![merged],
                vec!["MergedNow"],
            ),
            (
                "the pull request, behind the branch below",
                holding_these(at, true, &[standing("BEHIND")]),
                vec![queued],
                vec!["AutoMerge"],
            ),
        ])
    }

    /// The runs `the_runs_carried_on` and `the_runs_by_merge_state` write, as
    /// strings of their own.
    ///
    /// **Owned, because this is a function**: one handing back a `&'static str`
    /// is a door into the log's type wall, whatever it is for.
    fn owned(
        runs: Vec<(&str, String, Vec<String>, Vec<&str>)>,
    ) -> Vec<(String, String, Vec<String>, Vec<String>)> {
        runs.into_iter()
            .map(|(after, holds, missing, expected)| {
                (
                    after.to_owned(),
                    holds,
                    missing,
                    expected.into_iter().map(str::to_owned).collect(),
                )
            })
            .collect()
    }

    /// A release this run makes is judged on what GitHub held before anything
    /// is written, and its pull request back is queued, never merged now.
    ///
    /// **A pull request closed under the release's title is refused with the
    /// tree as it was given**, and nothing asked after the question. One open
    /// whose checks had passed is queued, because the release commit moves its
    /// head and no check has run on that; one merged under the release's title
    /// carried what the branch held before, so the release opens its own. A
    /// release commit GitHub answers with no commit stops the release there.
    #[test]
    fn a_release_made_here_is_judged_before_it_is_written_and_queued_after() {
        let (fake, bin) = a_fake_gh();
        let repository = a_workspace("fresh", &["fix: something to release"]);
        let root = repository.root();
        let fixtures = fake.root.join("fixtures");
        a_pull_request_closed_under_its_title_is_refused_before_anything_is_written(
            &root, &bin, &fixtures,
        );
        let made = "1".repeat(40);
        let committed = answered(
            &serde_json::json!({ "data": { "createCommitOnBranch": { "commit": { "oid": made } } } }),
        );
        let [tag, release, opened, queued] = the_steps_answered(&made);
        let mut passed = a_pull_request(
            "chore(release): carry v0.0.9-rc.1 back to dev",
            "OPEN",
            false,
        );
        passed["mergeStateStatus"] = "CLEAN".into();
        let mut merged = a_pull_request(
            "chore(release): carry v0.1.0-rc.1 back to dev",
            "MERGED",
            false,
        );
        merged["number"] = 6.into();
        merged["id"] = "PR_six".into();
        let (refs, releases) = (
            "repos/noctua-world/probe/git/refs",
            "repos/noctua-world/probe/releases",
        );
        for (what, holds, answers, expected) in [
            (
                "an open pull request whose checks had passed",
                holding_these(None, false, &[passed]),
                vec![
                    committed.clone(),
                    tag.clone(),
                    release.clone(),
                    queued.clone(),
                ],
                vec!["ReleaseCommit", refs, releases, "AutoMerge"],
            ),
            (
                "a pull request merged under the release's title",
                holding_these(None, false, &[merged]),
                vec![committed, tag, release, opened, queued],
                vec![
                    "ReleaseCommit",
                    refs,
                    releases,
                    "repos/noctua-world/probe/pulls",
                    "AutoMerge",
                ],
            ),
        ] {
            let mut all = vec![holds];
            all.extend(answers);
            published_on_staging(&root, &a_github(&bin, &fixtures, &all))
                .unwrap_or_else(|error| panic!("{what}: {error}"));
            let calls = the_calls(&fixtures);
            let made = asked(&calls);
            assert_eq!(
                made.first().map(String::as_str),
                Some("WhatGitHubHolds"),
                "{what}"
            );
            assert_eq!(made[1..], expected, "{what}: the release asked {made:?}");
            let queued = calls.last().expect("a call");
            assert_eq!(
                queued.request["variables"]["id"], "PR_seven",
                "{what}: auto-merge was asked for a pull request that does not carry the release"
            );
            repository.git(&["checkout", "-q", "--", "."]);
            std::fs::remove_file(root.join(super::CHANGELOG)).expect("the changelog written");
        }
        let refused = published_on_staging(
            &root,
            &a_github(
                &bin,
                &fixtures,
                &[
                    holding(None, false, None),
                    answered(
                        &serde_json::json!({ "data": { "createCommitOnBranch": { "commit": null } } }),
                    ),
                ],
            ),
        )
        .expect_err("a release commit answered with no commit");
        assert!(
            refused.contains("GitHub answered the release commit with no commit"),
            "{refused}"
        );
        assert_eq!(
            asked(&the_calls(&fixtures)),
            ["WhatGitHubHolds", "ReleaseCommit"]
        );
    }

    /// A release whose pull request back was closed under its title is
    /// refused before its release commit is made.
    ///
    /// **With the tree as it was given, and nothing asked after the
    /// question**, so no release commit is left on the branch with no tag,
    /// which every run after it would carry on into the same refusal.
    fn a_pull_request_closed_under_its_title_is_refused_before_anything_is_written(
        root: &Path,
        bin: &Path,
        fixtures: &Path,
    ) {
        let before = every_file_under(root);
        let refused = published_on_staging(
            root,
            &a_github(
                bin,
                fixtures,
                &[holding(None, false, Some(("CLOSED", false)))],
            ),
        )
        .expect_err("a pull request closed under the release's title");
        assert!(refused.contains("was closed without merging"), "{refused}");
        assert_eq!(asked(&the_calls(fixtures)), ["WhatGitHubHolds"]);
        assert_eq!(
            changed_between(&before, &every_file_under(root)),
            Vec::<String>::new(),
            "a release refused on what GitHub holds changed the tree"
        );
    }

    /// A tree holding what no commit has is neither written nor published.
    ///
    /// **A file no commit holds, and a member's manifest edited**: either would
    /// be relocked into a release commit that does not carry it. The refusal
    /// names it, the tree is the tree it was given, and GitHub is asked
    /// nothing.
    #[test]
    fn a_tree_holding_what_no_commit_has_is_not_released() {
        let (fake, bin) = a_fake_gh();
        let repository = a_workspace("unclean", &["fix: something to release"]);
        let root = repository.root();
        let fixtures = fake.root.join("fixtures");
        let inherited = this_environment();
        let member = root.join("probe/Cargo.toml");
        let committed = std::fs::read_to_string(&member).expect("the member's manifest");
        for (what, named) in [
            ("a file no commit holds", "probe/notes.txt"),
            ("a member's manifest edited", "probe/Cargo.toml"),
        ] {
            let (path, text) = if named == "probe/notes.txt" {
                (root.join(named), "a note\n".to_owned())
            } else {
                (
                    member.clone(),
                    format!("{committed}description = \"edited\"\n"),
                )
            };
            std::fs::write(path, text).expect("a writable scratch");
            let before = every_file_under(&root);
            let written = super::released(&a_setting(
                &root,
                Channel::Staging,
                Mode::Write,
                &inherited,
                None,
            ))
            .expect_err(what);
            let published = published_on_staging(
                &root,
                &a_github(&bin, &fixtures, &[holding(None, false, None)]),
            )
            .expect_err(what);
            for refused in [written, published] {
                assert!(
                    refused.contains("the tree holds changes no commit has")
                        && refused.contains(named),
                    "{what}: {refused}"
                );
            }
            assert!(
                the_calls(&fixtures).is_empty(),
                "{what}: GitHub was asked about a tree no commit holds"
            );
            assert_eq!(
                changed_between(&before, &every_file_under(&root)),
                Vec::<String>::new(),
                "{what}: a refused release changed the tree"
            );
            repository.git(&["checkout", "-q", "--", "."]);
            let _ = std::fs::remove_file(root.join("probe/notes.txt"));
        }
    }

    /// A branch that moved fails the release, and a release already
    /// on GitHub is refused before anything is written.
    #[test]
    fn a_release_github_already_holds_or_cannot_take_is_refused() {
        let (fake, bin) = a_fake_gh();
        let repository = a_workspace("refused", &["fix: something to release"]);
        let root = repository.root();
        let fixtures = fake.root.join("fixtures");
        let before = every_file_under(&root);
        let (github, inherited) = a_github(
            &bin,
            &fixtures,
            &[holding(Some(&"2".repeat(40)), false, None)],
        );
        let refused = super::released(&a_setting(
            &root,
            Channel::Staging,
            Mode::Publish,
            &inherited,
            Some(&github),
        ))
        .expect_err("already tagged on GitHub");
        assert!(
            refused.contains("is already tagged or released on GitHub"),
            "{refused}"
        );
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            Vec::<String>::new(),
            "a release refused before writing changed the tree"
        );
        // The branch moved: GitHub refuses the commit, and nothing after it is
        // asked for.
        let (github, inherited) = a_github(
            &bin,
            &fixtures,
            &[holding(None, false, None), "1\n".to_owned()],
        );
        let refused = super::released(&a_setting(
            &root,
            Channel::Staging,
            Mode::Publish,
            &inherited,
            Some(&github),
        ))
        .expect_err("the branch moved");
        assert!(refused.contains("`gh api graphql` refused"), "{refused}");
        assert_eq!(
            asked(&the_calls(&fixtures)),
            ["WhatGitHubHolds", "ReleaseCommit"]
        );
        // And a history with nothing to release asks GitHub nothing.
        repository.git(&["checkout", "-q", "--", "."]);
        let last = repository.head();
        repository.git(&["tag", "v0.1.0", &last]);
        repository.commit("chore: carry v0.1.0 back to staging (#9)", &[]);
        let (github, inherited) = a_github(&bin, &fixtures, &[]);
        super::released(&a_setting(
            &root,
            Channel::Staging,
            Mode::Publish,
            &inherited,
            Some(&github),
        ))
        .expect("nothing to release");
        assert!(
            the_calls(&fixtures).is_empty(),
            "GitHub was asked about nothing to release"
        );
        // A setting with no repository or no token is refused by name.
        for missing in ["GITHUB_REPOSITORY", "GH_TOKEN"] {
            let without: Vec<(OsString, OsString)> = inherited
                .iter()
                .filter(|(name, _)| name != missing)
                .cloned()
                .collect();
            let refused = GitHub::of(&without).expect_err(missing);
            assert!(refused.contains(missing), "{refused}");
        }
    }

    /// A production release is made on `main`, published as the latest
    /// release and not as a pre-release, and carried back to `staging`.
    ///
    /// **The channel decides each of those, and the branch a run carried on
    /// asks about**, so each is held here to what GitHub is asked. The
    /// candidate it ships was released on `staging` and merged, so the
    /// manifest carries the candidate's version and its section folds into
    /// the release's.
    #[test]
    fn a_production_release_is_made_on_main_and_carried_back_to_staging() {
        let (fake, bin) = a_fake_gh();
        let repository = a_workspace("production", &[]);
        let root = repository.root();
        // The candidate's release commit, as `staging` made it.
        for name in ["Cargo.toml", "Cargo.lock"] {
            let text = std::fs::read_to_string(root.join(name)).expect("the workspace");
            std::fs::write(
                root.join(name),
                text.replace("version = \"0.1.0\"", "version = \"0.1.0-rc.1\""),
            )
            .expect("a writable scratch");
        }
        repository.git(&["add", "-A"]);
        repository.commit("chore(release): v0.1.0-rc.1\n", &[]);
        let candidate = repository.head();
        repository.git(&["tag", "v0.1.0-rc.1", &candidate]);
        repository.commit("fix(xtask): something after the candidate", &[]);
        let head = repository.head();
        let fixtures = fake.root.join("fixtures");
        let on_main = |answers: &[String]| {
            let (github, inherited) = a_github(&bin, &fixtures, answers);
            super::released(&a_setting(
                &root,
                Channel::Main,
                Mode::Publish,
                &inherited,
                Some(&github),
            ))
        };
        let made = "1".repeat(40);
        let mut answers = vec![
            holding_these(None, false, &[]),
            answered(
                &serde_json::json!({ "data": { "createCommitOnBranch": { "commit": { "oid": made } } } }),
            ),
        ];
        answers.extend(the_steps_of("v0.1.0", "staging", &made));
        on_main(&answers).expect("a production release published whole");
        let calls = the_calls(&fixtures);
        let pulls = "repos/noctua-world/probe/pulls";
        assert_eq!(
            asked(&calls),
            [
                "WhatGitHubHolds",
                "ReleaseCommit",
                "repos/noctua-world/probe/git/refs",
                "repos/noctua-world/probe/releases",
                pulls,
                "AutoMerge",
            ]
        );
        a_production_release_asked(&root, &calls, &head, &made);
        // **Carried on from the release commit**, which the push it causes runs
        // the release on: with no pull request carrying it, the release asks
        // whether `staging` holds it.
        repository.git(&["add", "-A"]);
        repository.commit("chore(release): v0.1.0\n", &[]);
        let release_commit = repository.head();
        let [_, _, opened, queued] = the_steps_of("v0.1.0", "staging", &release_commit);
        on_main(&[
            holding_these(Some(release_commit.as_str()), true, &[]),
            the_branch_below("AHEAD"),
            opened,
            queued,
        ])
        .expect("a production release carried on");
        let calls = the_calls(&fixtures);
        assert_eq!(
            asked(&calls),
            ["WhatGitHubHolds", "HeldBelow", pulls, "AutoMerge"]
        );
        assert_eq!(
            (
                &calls[1].request["variables"]["below"],
                &calls[1].request["variables"]["at"]
            ),
            (
                &"refs/heads/staging".into(),
                &release_commit.as_str().into()
            ),
            "a run carried on asked whether another branch holds the release"
        );
        assert_eq!(calls[2].request["base"], "staging");
    }

    /// What a production release made whole asked GitHub for, as the fake
    /// recorded it in `calls`: `v0.1.0` of `head`, in `root`, its release
    /// commit answered as `made`.
    ///
    /// **On `main` and back to `staging`**: the pull request back is looked
    /// for and opened between the two, the release commit is made on `main`,
    /// and the release is the latest and not a pre-release.
    fn a_production_release_asked(root: &Path, calls: &[Call], head: &str, made: &str) {
        let question = &calls[0].request["variables"];
        assert_eq!(question["tag"], "v0.1.0");
        assert_eq!(
            (&question["head"], &question["base"]),
            (&"main".into(), &"staging".into()),
            "the pull request back is looked for between another pair of branches"
        );
        let input = &calls[1].request["variables"]["input"];
        assert_eq!(
            input["branch"]["branchName"], "main",
            "the release commit is made on another branch"
        );
        assert_eq!(
            input["expectedHeadOid"], head,
            "the commit the job checked out"
        );
        assert_eq!(input["message"]["headline"], "chore(release): v0.1.0");
        the_files_committed_are_the_files_written(
            root,
            &input["fileChanges"]["additions"],
            "0.1.0",
        );
        assert_eq!(
            calls[2].request,
            serde_json::json!({ "ref": "refs/tags/v0.1.0", "sha": made })
        );
        let release = &calls[3].request;
        assert_eq!(release["tag_name"], "v0.1.0");
        assert_eq!(
            release["prerelease"], false,
            "a production release is published as a pre-release"
        );
        assert_eq!(
            release["make_latest"], "true",
            "a production release is not published as the latest"
        );
        let notes = release["body"].as_str().unwrap_or_default();
        assert!(
            notes.starts_with("### Added\n\n- add the probe (")
                && notes.contains("### Fixed\n\n- **xtask:** something after the candidate ("),
            "the candidate's section does not fold into the release's notes: {notes}"
        );
        assert_eq!(
            (
                &calls[4].request["head"],
                &calls[4].request["base"],
                &calls[4].request["title"]
            ),
            (
                &"main".into(),
                &"staging".into(),
                &"chore(release): carry v0.1.0 back to staging".into()
            ),
            "a production release is not carried back to staging"
        );
    }

    /// A gate that refused stops the release before anything is written, and
    /// one that passed lets it on to the write.
    ///
    /// **The verb itself, with the fake as its gate**, which exits 1 and then
    /// 0, over a workspace with something to release: the release reads
    /// nothing of the gate but its exit status, so every place that hands a
    /// refusal on lies between the status and what this asserts. The fake
    /// records each start, so both are the gate's own `check`.
    #[test]
    fn a_gate_that_refused_stops_the_release() {
        let (held, bin) = a_fake_gh();
        let fixtures = held.root.join("fixtures");
        let fake = bin.join(format!("gh{}", std::env::consts::EXE_SUFFIX));
        std::fs::create_dir_all(fixtures.join("answers")).expect("a writable scratch");
        for (nth, exit) in ["1\n", "0\n"].into_iter().enumerate() {
            std::fs::write(fixtures.join("answers").join(format!("{nth:03}")), exit)
                .expect("a writable scratch");
        }
        let repository = a_workspace("gated", &["fix: something to release"]);
        let root = repository.root();
        let mut inherited = this_environment();
        inherited.push(("NC_FAKE_GH".into(), fixtures.as_os_str().to_owned()));
        let before = every_file_under(&root);
        let refused = super::run_at(
            &fake,
            &root,
            Channel::Staging,
            false,
            false,
            false,
            &inherited,
        )
        .expect_err("a gate that exits 1 refused");
        assert!(
            refused.contains("the gate refused, as its lines above say, so nothing was written"),
            "{refused}"
        );
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            Vec::<String>::new(),
            "a release whose gate refused changed the tree"
        );
        super::run_at(
            &fake,
            &root,
            Channel::Staging,
            false,
            false,
            false,
            &inherited,
        )
        .expect("a gate that exits 0 passed, and the release goes on");
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            [
                "CHANGELOG.md appeared",
                "Cargo.lock was rewritten",
                "Cargo.toml was rewritten",
            ],
            "a release whose gate passed did not write the release"
        );
        assert_eq!(
            the_calls(&fixtures)
                .iter()
                .map(|call| call.arguments.join(" "))
                .collect::<Vec<_>>(),
            ["check", "check"],
            "the gate was not the child started"
        );
    }

    /// A run carried on from the release commit at `release_commit`, whose pull
    /// request back GitHub says conflicts with the branch below or refuses to
    /// queue, stops, and says how a person carries the release back.
    ///
    /// **Its head is the released branch, which takes nothing but pull requests
    /// and the release App**, so the conflict is resolved on a branch made from
    /// the one below; nothing is asked after GitHub refuses, and a conflict is
    /// refused with auto-merge on as well as off.
    fn a_pull_request_back_that_cannot_merge_says_how_to_by_hand(
        root: &Path,
        release_commit: &str,
        bin: &Path,
        fixtures: &Path,
    ) {
        let standing = |status: &str, auto: bool| {
            let mut node = a_pull_request(
                "chore(release): carry v0.1.0-rc.1 back to dev",
                "OPEN",
                auto,
            );
            node["mergeStateStatus"] = status.into();
            holding_these(Some(release_commit), true, &[node])
        };
        for (what, answers, expected, why) in [
            (
                "in conflict",
                vec![standing("DIRTY", false)],
                vec!["WhatGitHubHolds"],
                "GitHub says it conflicts with that branch",
            ),
            (
                "in conflict, queued",
                vec![standing("DIRTY", true)],
                vec!["WhatGitHubHolds"],
                "GitHub says it conflicts with that branch",
            ),
            (
                "not queued",
                vec![standing("BLOCKED", false), "1\n".to_owned()],
                vec!["WhatGitHubHolds", "AutoMerge"],
                "",
            ),
        ] {
            let refused =
                published_on_staging(root, &a_github(bin, fixtures, &answers)).expect_err(what);
            assert!(
                refused.starts_with(
                    "pull request #7, which carries v0.1.0-rc.1 back to dev, cannot be merged on \
                     GitHub: "
                ) && refused.contains(why)
                    && refused.contains(
                        "v0.1.0-rc.1 is tagged and released. Carry it back by hand: make a \
                         branch from dev, merge staging into it, keep staging's version and \
                         CHANGELOG.md wherever the two disagree, run `cargo update --workspace`, \
                         commit it signed, and open a pull request into dev. Once that merges, \
                         GitHub marks #7 merged",
                    ),
                "{what}: {refused}"
            );
            assert_eq!(
                asked(&the_calls(fixtures)),
                expected,
                "{what}: the release went on past a pull request back that cannot merge"
            );
        }
    }

    /// The environment CI's release job runs in: GitHub Actions, the job
    /// `release` of `release.yml` on `staging`, in the repository the fake
    /// publishes to.
    fn the_release_jobs_environment() -> Vec<(OsString, OsString)> {
        vec![
            ("GITHUB_ACTIONS".into(), "true".into()),
            ("GITHUB_JOB".into(), "release".into()),
            ("GITHUB_REPOSITORY".into(), "noctua-world/probe".into()),
            (
                "GITHUB_WORKFLOW_REF".into(),
                "noctua-world/probe/.github/workflows/release.yml@refs/heads/staging".into(),
            ),
        ]
    }

    /// The gate stands aside under `--publish`, as the release job of
    /// `release.yml` on the channel's branch, and nowhere else.
    ///
    /// **Driven with an environment of its own, never this process's**, which
    /// in CI is a job of `ci.yml`: each variable the job is known by is taken
    /// away or named otherwise, one at a time.
    #[test]
    fn the_gate_stands_aside_in_the_release_job_alone() {
        let job = the_release_jobs_environment();
        super::the_gate_ran_in_the_job_before(Channel::Staging, true, &job)
            .expect("CI's release job, on staging");
        let refused = super::the_gate_ran_in_the_job_before(Channel::Staging, false, &job)
            .expect_err("without --publish");
        assert!(
            refused.contains("is passed with --publish alone")
                && refused.ends_with("Nothing was written"),
            "{refused}"
        );
        let with = |name: &str, value: Option<&str>| -> Vec<(OsString, OsString)> {
            let mut changed: Vec<(OsString, OsString)> =
                job.iter().filter(|(key, _)| key != name).cloned().collect();
            if let Some(value) = value {
                changed.push((name.into(), value.into()));
            }
            changed
        };
        let at = |workflow: &str, branch: &str| {
            format!("noctua-world/probe/.github/workflows/{workflow}@refs/heads/{branch}")
        };
        super::the_gate_ran_in_the_job_before(
            Channel::Main,
            true,
            &with("GITHUB_WORKFLOW_REF", Some(&at("release.yml", "main"))),
        )
        .expect("CI's release job, on main");
        for (what, inherited, channel) in [
            (
                "outside GitHub Actions",
                with("GITHUB_ACTIONS", None),
                Channel::Staging,
            ),
            (
                "GitHub Actions said otherwise",
                with("GITHUB_ACTIONS", Some("false")),
                Channel::Staging,
            ),
            ("no job", with("GITHUB_JOB", None), Channel::Staging),
            (
                "the gate's job",
                with("GITHUB_JOB", Some("gate")),
                Channel::Staging,
            ),
            (
                "the gate's workflow",
                with("GITHUB_WORKFLOW_REF", Some(&at("ci.yml", "staging"))),
                Channel::Staging,
            ),
            (
                "another branch",
                with("GITHUB_WORKFLOW_REF", Some(&at("release.yml", "dev"))),
                Channel::Staging,
            ),
            ("another channel", job.clone(), Channel::Main),
            (
                "a pull request",
                with(
                    "GITHUB_WORKFLOW_REF",
                    Some("noctua-world/probe/.github/workflows/release.yml@refs/pull/7/merge"),
                ),
                Channel::Staging,
            ),
            (
                "another repository",
                with(
                    "GITHUB_WORKFLOW_REF",
                    Some("someone/else/.github/workflows/release.yml@refs/heads/staging"),
                ),
                Channel::Staging,
            ),
            (
                "no repository",
                with("GITHUB_REPOSITORY", None),
                Channel::Staging,
            ),
            (
                "no workflow",
                with("GITHUB_WORKFLOW_REF", None),
                Channel::Staging,
            ),
            ("nothing at all", Vec::new(), Channel::Staging),
        ] {
            let refused =
                super::the_gate_ran_in_the_job_before(channel, true, &inherited).expect_err(what);
            assert!(
                refused.contains("and this run is not that job")
                    && refused.ends_with("Nothing was written"),
                "{what}: {refused}"
            );
        }
    }

    /// The names the gate is stood aside by are the ones the workflow reader
    /// holds `release.yml` to: the flag the verb declares, the workflow and the
    /// job.
    #[test]
    fn the_gate_is_stood_aside_by_the_names_the_workflow_is_held_to() {
        use clap::CommandFactory as _;
        let cli = crate::Cli::command();
        let release = cli.find_subcommand("release").expect("the release verb");
        assert!(
            release.get_arguments().any(|argument| argument
                .get_long()
                .map(|long| format!("--{long}"))
                .as_deref()
                == Some(super::AFTER_THE_GATE_JOB)),
            "the release verb declares no `{}`",
            super::AFTER_THE_GATE_JOB
        );
        assert_eq!(
            super::THE_RELEASE_WORKFLOW,
            crate::workflow::RELEASE_WORKFLOW
        );
        assert_eq!(super::THE_RELEASE_JOB, crate::workflow::THE_RELEASE_JOB);
    }

    /// The verb stands its gate aside after the gate job and nowhere else,
    /// through `run_at` itself: refused, nothing is written and no gate and no
    /// `gh` is started; accepted, the release is made with no gate started.
    #[test]
    fn the_verb_stands_its_gate_aside_after_the_gate_job_and_nowhere_else() {
        let (held, bin) = a_fake_gh();
        let fixtures = held.root.join("fixtures");
        let fake = bin.join(format!("gh{}", std::env::consts::EXE_SUFFIX));
        let repository = a_workspace("after-the-gate", &["fix: something to release"]);
        let root = repository.root();
        let before = every_file_under(&root);
        let made = "1".repeat(40);
        let mut answers = vec![
            holding(None, false, None),
            answered(
                &serde_json::json!({ "data": { "createCommitOnBranch": { "commit": { "oid": made } } } }),
            ),
        ];
        answers.extend(the_steps_answered(&made));
        let (_, mut inherited) = a_github(&bin, &fixtures, &answers);
        inherited.retain(|(name, _)| !name.to_string_lossy().starts_with("GITHUB_"));
        let refused = super::run_at(
            &fake,
            &root,
            Channel::Staging,
            false,
            false,
            true,
            &inherited,
        )
        .expect_err("without --publish");
        assert!(
            refused.contains("is passed with --publish alone"),
            "{refused}"
        );
        let refused = super::run_at(
            &fake,
            &root,
            Channel::Staging,
            false,
            true,
            true,
            &inherited,
        )
        .expect_err("outside the release job");
        assert!(
            refused.contains("and this run is not that job"),
            "{refused}"
        );
        assert_eq!(
            changed_between(&before, &every_file_under(&root)),
            Vec::<String>::new(),
            "a gate stood aside where none ran changed the tree"
        );
        assert!(
            the_calls(&fixtures).is_empty(),
            "a gate stood aside where none ran started the gate or `gh`"
        );
        inherited.extend(the_release_jobs_environment());
        super::run_at(
            &fake,
            &root,
            Channel::Staging,
            false,
            true,
            true,
            &inherited,
        )
        .expect("the release job, after the gate job");
        assert_eq!(
            asked(&the_calls(&fixtures)),
            [
                "WhatGitHubHolds",
                "ReleaseCommit",
                "repos/noctua-world/probe/git/refs",
                "repos/noctua-world/probe/releases",
                "repos/noctua-world/probe/pulls",
                "AutoMerge",
            ],
            "the release job started the gate, which ran in the job before it"
        );
    }

    /// A refusal at the release commit or after it puts back every file the
    /// run wrote, and says so; and a pull request back that GitHub says
    /// conflicts, or will not queue, says how to carry the release back.
    ///
    /// **Through a release made fresh**, over one workspace, which each refusal
    /// leaves as it was given: the release commit refused, the tag refused, a
    /// pull request REST answers as conflicting, and auto-merge refused.
    #[test]
    fn a_refusal_after_the_first_write_on_github_puts_the_tree_back() {
        let (fake, bin) = a_fake_gh();
        let repository = a_workspace("put-back", &["fix(xtask): something to release"]);
        let root = repository.root();
        let fixtures = fake.root.join("fixtures");
        let made = "1".repeat(40);
        let before = every_file_under(&root);
        let committed = answered(
            &serde_json::json!({ "data": { "createCommitOnBranch": { "commit": { "oid": made } } } }),
        );
        let [tag, release, opened, _] = the_steps_answered(&made);
        let conflicting = answered(
            &serde_json::json!({ "number": 7, "id": 5_147_253_185_u64, "node_id": "PR_seven", "title": "chore(release): carry v0.1.0-rc.1 back to dev", "state": "open", "mergeable": false }),
        );
        let refused_by_gh = "1\n".to_owned();
        let (refs, releases, pulls) = (
            "repos/noctua-world/probe/git/refs",
            "repos/noctua-world/probe/releases",
            "repos/noctua-world/probe/pulls",
        );
        for (what, answers, expected, said) in [
            (
                "the release commit refused",
                vec![refused_by_gh.clone()],
                vec![],
                "",
            ),
            (
                "the tag refused",
                vec![committed.clone(), refused_by_gh.clone()],
                vec![refs],
                "",
            ),
            (
                "a pull request back in conflict",
                vec![committed.clone(), tag.clone(), release.clone(), conflicting],
                vec![refs, releases, pulls],
                "Carry it back by hand",
            ),
            (
                "auto-merge refused",
                vec![committed.clone(), tag, release, opened, refused_by_gh],
                vec![refs, releases, pulls, "AutoMerge"],
                "Carry it back by hand",
            ),
        ] {
            let mut all = vec![holding(None, false, None)];
            all.extend(answers);
            let refused =
                published_on_staging(&root, &a_github(&bin, &fixtures, &all)).expect_err(what);
            assert!(
                refused.contains(said)
                    && refused.ends_with("Every file this run wrote is put back as HEAD holds it"),
                "{what}: {refused}"
            );
            let mut asked_for = vec!["WhatGitHubHolds", "ReleaseCommit"];
            asked_for.extend(expected);
            assert_eq!(asked(&the_calls(&fixtures)), asked_for, "{what}");
            assert_eq!(
                changed_between(&before, &every_file_under(&root)),
                Vec::<String>::new(),
                "{what}: a release refused on GitHub left the tree it wrote"
            );
        }
    }

    /// What tells this test binary, run again, that it is the child of
    /// [`no_child_but_gh_is_handed_ghs_variables`], and where the fake it
    /// starts records.
    const CHILD: &str = "NC_RELEASE_TESTS_CHILD";

    /// No child of the release but `gh` is handed `gh`'s variables, though
    /// this process holds them in its own environment, as it does under
    /// `--publish`.
    ///
    /// **This test binary is run again with them exported** — the App's
    /// token, the workflow's, another host, a debug switch and another
    /// repository — the one way to give a process an environment without
    /// setting it in this one. There the gate and the relock are each started
    /// at the fake, which records what it was handed: each is cleared before
    /// it is handed anything, so what this process holds reaches it only
    /// through its list, and `gh`'s are taken out of that. The git the release
    /// reads its history through is `crate::history`'s, whose own test holds
    /// it to the same.
    #[test]
    fn no_child_but_gh_is_handed_ghs_variables() {
        if let Some(fixtures) = std::env::var_os(CHILD) {
            let fixtures = PathBuf::from(fixtures);
            let fake = fixtures
                .with_file_name("bin")
                .join(format!("gh{}", std::env::consts::EXE_SUFFIX));
            let mut inherited: Vec<(OsString, OsString)> = std::env::vars_os().collect();
            let gated = super::a_gate(&fake, &fixtures, &inherited).status();
            inherited.retain(|(name, _)| name != "CARGO");
            inherited.push(("CARGO".into(), fake.into_os_string()));
            let relocked = super::relock(&fixtures, &inherited);
            let ran = format!("{:?} {relocked:?}", gated.map(|status| status.success()));
            std::fs::write(fixtures.join("ran"), ran).expect("a writable scratch");
            return;
        }
        let (held, _) = a_fake_gh();
        let fixtures = held.root.join("fixtures");
        std::fs::create_dir_all(fixtures.join("answers")).expect("a writable scratch");
        for nth in 0..2 {
            std::fs::write(fixtures.join("answers").join(format!("{nth:03}")), "0\n")
                .expect("a writable scratch");
        }
        let child = this_binary_again(&fixtures);
        assert!(child.success(), "the child failed: {child}");
        assert_eq!(
            std::fs::read_to_string(fixtures.join("ran"))
                .expect("the child writes what it ran, when it runs this test by its name"),
            "Ok(true) Ok(())"
        );
        let calls = the_calls(&fixtures);
        assert_eq!(
            calls
                .iter()
                .map(|call| call.arguments.join(" "))
                .collect::<Vec<_>>(),
            ["check", "update --workspace"],
            "the gate and the relock are not the children that were started"
        );
        for call in &calls {
            assert_eq!(
                call.handed,
                [
                    "GH_TOKEN=unset",
                    "GITHUB_TOKEN=unset",
                    "GH_HOST=unset",
                    "GH_DEBUG=unset",
                    "GH_REPO=unset",
                ],
                "`{}` was handed a variable of `gh`'s, and the App's token is one",
                call.arguments.join(" ")
            );
        }
    }

    /// This test binary, run again as [`CHILD`] says, with `gh`'s variables
    /// in its environment and nothing it prints shown.
    #[expect(
        clippy::disallowed_methods,
        reason = "the child is this same test binary, its output discarded: the one way to \
                  give a process an environment without setting it in this one"
    )]
    fn this_binary_again(fixtures: &Path) -> std::process::ExitStatus {
        std::process::Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "release::tests::no_child_but_gh_is_handed_ghs_variables",
                "--test-threads=1",
            ])
            .env(CHILD, fixtures)
            .env("NC_FAKE_GH", fixtures)
            .envs([
                ("GH_TOKEN", "the-apps-token"),
                ("GITHUB_TOKEN", "the-workflows-token"),
                ("GH_HOST", "elsewhere.example"),
                ("GH_DEBUG", "api"),
                ("GH_REPO", "someone/else"),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the child runs")
    }
}

#[cfg(test)]
mod relock_tests {
    use crate::check::LeavesNothingBehind;
    use std::ffi::OsString;
    use std::path::Path;

    /// The files a release writes, each with its bytes, or `None` for one the
    /// tree does not have.
    fn the_files_written(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
        ["CHANGELOG.md", "Cargo.lock", "Cargo.toml"]
            .into_iter()
            .map(|name| (name.to_owned(), std::fs::read(root.join(name)).ok()))
            .collect()
    }

    /// The release of `root` at 0.2.0 through `written`, with the cargo
    /// that `inherited` names.
    fn a_release_of(root: &Path, inherited: &[(OsString, OsString)]) -> Result<(), String> {
        super::written(
            &super::Releasing {
                root,
                channel: super::Channel::Staging,
                mode: super::Mode::Write,
                inherited,
                github: None,
            },
            super::Version::parse("0.2.0").expect("a version"),
            "# Changelog\n\nas the release writes it\n",
        )
        .map(|_| ())
    }

    /// A failed relock leaves the version where it was, and every other file
    /// the release wrote.
    ///
    /// **The window between the write and the lockfile.** `relock` runs
    /// `cargo update --workspace`, which is one registry away from the network,
    /// and a failure there left `Cargo.toml` at the new version and `Cargo.lock`
    /// at the old one, under a message saying nothing was released.
    ///
    /// **A refused release leaves the tree it was given.** Left as the failure
    /// put it, the manifest and the lockfile disagree, so cargo refuses every
    /// `cargo xtask` verb before it starts — the alias passes `--locked` — and
    /// the release run again refuses a tree holding changes no commit has.
    ///
    /// **Through `written` itself, twice.** Once with a cargo that cannot be
    /// started, so the relock refuses before it writes, over a tree with no
    /// changelog; and once with the cargo running this test, over a member
    /// whose own version the release does not write and a lockfile cargo
    /// rewrites whole, so the read-back refuses what the relock wrote. Each
    /// file the release wrote comes back byte for byte, and a changelog the
    /// tree had none of is removed.
    #[test]
    fn a_failed_relock_puts_the_version_back() {
        let scratch = LeavesNothingBehind::new("release-relock");
        let root = &scratch.root;
        for (path, text) in [
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"probe\"]\nresolver = \"3\"\n\n[workspace.package]\n\
                 version = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "probe/Cargo.toml",
                "[package]\nname = \"probe\"\nversion = \"9.9.9\"\nedition.workspace = true\n\
                 publish = false\n",
            ),
            ("probe/src/lib.rs", "//! A probe.\n"),
            // Not the bytes cargo writes, so a relock that ran rewrote them.
            (
                "Cargo.lock",
                "# The lockfile as the tree holds it.\nversion = 4\n\n[[package]]\nname = \
                 \"probe\"\nversion = \"0.1.0\"\n",
            ),
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().expect("a file sits in a directory"))
                .expect("a writable scratch");
            std::fs::write(path, text).expect("a writable scratch");
        }

        let before = the_files_written(root);
        let no_cargo: Vec<(OsString, OsString)> =
            vec![("CARGO".into(), root.join("no-cargo-here").into_os_string())];
        let refused =
            a_release_of(root, &no_cargo).expect_err("a cargo that cannot start relocks nothing");
        assert!(
            refused.contains("could not run cargo update")
                && refused.contains("Nothing was released and the version is back at 0.1.0"),
            "{refused}"
        );
        assert_eq!(
            the_files_written(root),
            before,
            "the version was not put back, and a refused release leaves the tree it was given"
        );

        std::fs::write(
            root.join("CHANGELOG.md"),
            "# Changelog\n\nas the tree holds it\n",
        )
        .expect("a writable scratch");
        let before = the_files_written(root);
        let mut inherited: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(name, _)| name != "CARGO_NET_OFFLINE")
            .collect();
        inherited.push(("CARGO_NET_OFFLINE".into(), "true".into()));
        let refused = a_release_of(root, &inherited)
            .expect_err("a member whose version the release does not write fails the read-back");
        assert!(
            refused.contains("Cargo.lock reads back with")
                && refused.contains("Nothing was released and the version is back at 0.1.0"),
            "{refused}"
        );
        assert_eq!(
            the_files_written(root),
            before,
            "a lockfile the relock rewrote, or another file the release wrote, was not put back"
        );
    }
}
