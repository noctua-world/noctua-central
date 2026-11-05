//! Who is in this workspace, and which of them can reach the log.
//!
//! **Both questions, answered by reading manifests, come out wrong for
//! manifests cargo accepts.** They are resolution questions — cargo decides
//! them from the workspace table, the member globs, the path dependencies and
//! the target-specific tables together — and a reader that works from one
//! manifest at a time cannot see the other files the answer depends on. Two
//! measured misses of such a reader, each switching a gate off in silence:
//!
//! ```text
//! [workspace.dependencies] mylog = { package = "nc-log", … }
//! nc-prune/Cargo.toml       mylog.workspace = true
//!   -> ok 0 crate(s) outside nc-log may log … check passed
//!      (with a live leak at a logging call site in nc-prune)
//!
//! tools/nc-loud             a path dependency of nc-prune, not in `members`
//!   -> a workspace member cargo builds, that the gate never opened
//! ```
//!
//! So this asks cargo. `cargo metadata` lists exactly the workspace members and
//! resolves the whole dependency graph, which is the fact both gates want and a
//! manifest reader cannot compute. **The graph and not the declarations**: a
//! crate that writes `pub use nc_log;` hands the log to everything depending on
//! *it*, and the consumer's own manifest never spells it — so reaching the log
//! is a question about the graph, and only cargo has the graph.
//!
//! It is not a silent fallback. A `cargo metadata` that does not run is
//! reported, because a scope that collapses quietly is the defect this module
//! exists to close, one level up.

use std::path::Path;

/// The crate whose dependency decides whether a crate may log.
const LOG: &str = "nc-log";

/// One member of this cargo workspace, as cargo resolves it.
#[derive(Debug)]
pub(crate) struct Member {
    /// Its directory, relative to the workspace root, with `/` separators.
    pub(crate) path: String,
    /// The name cargo builds it under, as cargo resolves it.
    ///
    /// **Not the directory it sits in.** A package's name and its directory
    /// agree in this tree today and cargo does not require it, and a reader that
    /// derives one from the other is asking a manifest a resolution question —
    /// which is the whole reason this module exists. `release.rs`'s reach census
    /// needs the name a `use` writes, and this is where cargo says it.
    pub(crate) name: String,
    /// Whether `nc-log` is reachable from it, at any depth.
    ///
    /// Not "depends on": a re-export makes the log reachable through a crate
    /// that declares it, and the consumer's manifest says nothing about that.
    pub(crate) can_log: bool,
    /// Every source root cargo compiles for it, relative to the workspace.
    ///
    /// **A number the walk cannot fake, and finer than one per member.** A
    /// floor that asks only that *some* source file sit under each member
    /// directory lets a walk that lost 99% of its files still report
    /// "across all … member(s)". Cargo knows every target's `src_path` — one
    /// per library, binary and integration test, and `cargo xtask check` prints
    /// how many — and a walk that reaches all of those has reached the tree's
    /// own entry points.
    ///
    /// The number is not written here: it moves with every suite added, and a
    /// number in a comment that moves is one that rots. `cargo metadata
    /// --no-deps` answers it.
    pub(crate) sources: Vec<String>,
    /// Every source root `cargo doc` is asked to cover for it.
    ///
    /// The libraries and the binaries, which is what `cargo doc --workspace`
    /// documents; an integration test is not documented and is not in this list.
    ///
    /// **Chosen by the target's *kind*, never by cargo's `doc` flag.** That flag
    /// is what a manifest sets, so a set derived from it agrees with the
    /// manifest by construction and can say nothing about it: `[lib] doc =
    /// false` takes the library out of the run *and* out of the expectation, and
    /// the step reads `ok` over a crate rustdoc never opened. Measured with such
    /// a set, exactly that pair — a broken intra-doc link in
    /// `crates/nc-prune/src/lib.rs` with `doc = false` beside it — gives
    /// `rustdoc` exit 0, `ok documentation`, and `check passed` with every
    /// number unmoved. A kind cannot be switched off in a manifest, so the
    /// expectation stands where the flag falls.
    pub(crate) documented: Vec<String>,
}

/// Every member of the workspace at `root`, or why cargo could not say.
pub(crate) fn members(root: &Path) -> Result<Vec<Member>, String> {
    // **The resolved graph, not the declared dependencies.** `--no-deps` answers
    // "what does this manifest name", and reaching the log is not that: a crate
    // that writes `pub use nc_log;` hands it to everything depending on *it*,
    // and the consumer's own manifest never spells `nc-log`. Measured on a
    // scratch workspace where `nc-relay` depends on the log and re-exports it,
    // and `nc-user` depends only on `nc-relay`:
    //
    // ```text
    // crates/nc-relay   can_log=true
    // crates/nc-user    can_log=false   <- and it calls the log through the re-export
    // ```
    //
    // With `--no-deps`, `nc-user`'s whole source is outside the type wall's
    // source gate. The graph costs about a third of a second here, and it
    // answers reachability — which is the question the scope is asking.
    // **Every feature, because a feature is not a boundary.** `cargo metadata`
    // resolves *default* features, so an optional dependency behind one that is
    // off is absent from the graph entirely. Measured, with `nc-log` made
    // `optional = true` behind a `noisy` feature and a `format!(…).leak()` at a
    // logging call site in the crate that takes it:
    //
    // ```text
    // ok  0 crate(s) outside nc-log may log … check passed
    // ```
    //
    // A crate that can log under *some* feature can log, and the source is in
    // the tree either way — the gate reads text, not a build plan. Control: the
    // same dependency non-optional gives `can_log=true`.
    //
    // **Through [`metadata`], so the invocation has one copy.** Two argument
    // lists can drift a flag apart with nothing to say so, and a doc saying
    // they are one is then a sentence rather than a fact.
    from_metadata(&metadata(root)?)
}

/// `cargo metadata`'s answer for the workspace at `root`, parsed.
///
/// The one invocation, which [`members`] and every other reader of the resolved
/// graph go through, so they cannot ask cargo two different questions.
///
/// **One copy of the argument list, because a second one drifts.**
/// `--all-features` is load-bearing — an optional dependency behind an
/// off-by-default feature is absent from the graph, and the crate that takes it
/// is then outside the type wall's scope entirely — so a flag present in one
/// list and missing from the other is a gate switched off for some crates and
/// not others, with both numbers still printed.
fn metadata(root: &Path) -> Result<serde_json::Value, String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let output = std::process::Command::new("cargo")
        .args([
            "metadata",
            "--all-features",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("`cargo metadata` did not run: {error}"))?;
    if !output.status.success() {
        return Err(what_a_failed_graph_says(&String::from_utf8_lossy(
            &output.stderr,
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("`cargo metadata` did not answer in JSON: {error}"))
}

/// Why `cargo metadata` failed, and the fix where cargo's own words point to it.
///
/// **`--offline` asks for a package that is not on this machine.** A fresh
/// clone holds only what its first build fetched, and cargo's answer then names
/// a package and offline mode — `no matching package named … found`, `offline
/// mode (via --offline) can sometimes cause surprising resolution failures` —
/// and never the command that fixes it. So the refusal names it: `cargo xtask
/// check` without `--offline` fetches every package the lockfile names before it
/// asks for the graph.
fn what_a_failed_graph_says(stderr: &str) -> String {
    let said = stderr.trim();
    if said.contains("offline") {
        format!(
            "`cargo metadata` failed: {said}\nA package the lockfile names is not on this \
             machine. `cargo fetch --locked` downloads every one of them, and `cargo xtask \
             check` without `--offline` runs it before anything reads the graph"
        )
    } else {
        format!("`cargo metadata` failed: {said}")
    }
}

/// Every package the build resolves from a path outside this repository.
///
/// Returns `(name, manifest path)` for each. **`from_metadata` drops these**: it
/// keeps only packages whose manifest sits under `workspace_root` and silently
/// drops the rest, so a dependency reaching outside the tree is absent from the
/// member list every gate is scoped by, and this is where it is named.
///
/// # Errors
///
/// Returns why `cargo metadata` could not be read.
pub(crate) fn resolved_from_outside(root: &Path) -> Result<Vec<(String, String)>, String> {
    outside_of(&metadata(root)?)
}

/// The member directories a package is built from: its own, and every member it
/// reaches through a normal or a build edge, at any depth.
///
/// **A question about the graph, for the reason [`members`] gives.** A plugin
/// built for `wasm32` is built from `crates/nc-guest` whatever its own manifest
/// spells, and a dev-dependency is built only for a test the component never
/// contains — so a component's inputs are this set, and a list read from one
/// manifest would miss the first and add the second.
///
/// # Errors
///
/// Returns why `cargo metadata` could not be read, or that no member of this
/// workspace is `package`.
pub(crate) fn built_from(root: &Path, package: &str) -> Result<Vec<String>, String> {
    built_from_in(&metadata(root)?, package)
}

/// The same question, asked of an answer that is already parsed.
///
/// Split from the call so the walk can be driven without a workspace on disk:
/// the edge it must not follow, a dev-dependency, is one no plugin of this
/// tree has today, and a clause that has never had anything to judge is one
/// nobody knows is wired up.
fn built_from_in(parsed: &serde_json::Value, package: &str) -> Result<Vec<String>, String> {
    let workspace = parsed
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "`cargo metadata` named no workspace root".to_owned())?;
    let packages = parsed
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` listed no packages".to_owned())?;
    let nodes = parsed
        .get("resolve")
        .and_then(|resolve| resolve.get("nodes"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` answered with no resolved graph".to_owned())?;
    let id = |package: &serde_json::Value| {
        package
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let start = packages
        .iter()
        .find(|candidate| {
            candidate.get("name").and_then(serde_json::Value::as_str) == Some(package)
                && candidate
                    .get("source")
                    .is_some_and(serde_json::Value::is_null)
        })
        .and_then(id)
        .ok_or_else(|| format!("no member of this workspace is called `{package}`"))?;
    let mut reached = vec![start.clone()];
    let mut frontier = vec![start];
    while let Some(at) = frontier.pop() {
        let Some(node) = nodes
            .iter()
            .find(|node| node.get("id").and_then(serde_json::Value::as_str) == Some(&at))
        else {
            continue;
        };
        for dependency in node
            .get("deps")
            .and_then(serde_json::Value::as_array)
            .map_or(&[][..], Vec::as_slice)
        {
            // **Built, not tested.** An edge whose every kind is `dev` is a
            // dependency of a test target, which a component never holds.
            let built = dependency
                .get("dep_kinds")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|kinds| {
                    kinds.iter().any(|kind| {
                        kind.get("kind").and_then(serde_json::Value::as_str) != Some("dev")
                    })
                });
            let Some(pkg) = dependency.get("pkg").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if built && !reached.iter().any(|seen| seen == pkg) {
                reached.push(pkg.to_owned());
                frontier.push(pkg.to_owned());
            }
        }
    }
    let mut directories: Vec<String> = packages
        .iter()
        .filter(|candidate| id(candidate).is_some_and(|found| reached.contains(&found)))
        .filter_map(|candidate| candidate.get("manifest_path")?.as_str())
        .filter_map(|manifest| {
            inside_the_root(workspace, manifest)?
                .strip_suffix("/Cargo.toml")
                .map(str::to_owned)
        })
        .collect();
    directories.sort();
    Ok(directories)
}

#[cfg(test)]
mod built_from_tests {
    /// A plugin's graph, in the shape `cargo metadata` answers.
    ///
    /// The plugin, the bindings it builds against, the crate those are built
    /// with, a registry crate, and a dev-dependency.
    fn graph() -> serde_json::Value {
        let path = |name: &str| format!("path+file:///w/{name}#0.1.0");
        let edge = |to: String, kind: Option<&str>| serde_json::json!({ "pkg": to, "dep_kinds": [{ "kind": kind, "target": null }] });
        serde_json::json!({
            "workspace_root": "/w",
            "packages": [
                { "name": "nc-plugin-a", "id": path("plugins/a"), "source": null,
                  "manifest_path": "/w/plugins/a/Cargo.toml" },
                { "name": "nc-guest", "id": path("crates/nc-guest"), "source": null,
                  "manifest_path": "/w/crates/nc-guest/Cargo.toml" },
                { "name": "nc-build", "id": path("crates/nc-build"), "source": null,
                  "manifest_path": "/w/crates/nc-build/Cargo.toml" },
                { "name": "nc-schema", "id": path("crates/nc-schema"), "source": null,
                  "manifest_path": "/w/crates/nc-schema/Cargo.toml" },
                { "name": "serde", "id": "registry+x#serde@1.0.0",
                  "source": "registry+https://github.com/rust-lang/crates.io-index",
                  "manifest_path": "/home/someone/registry/serde/Cargo.toml" },
            ],
            "resolve": { "nodes": [
                { "id": path("plugins/a"), "deps": [
                    edge(path("crates/nc-guest"), None),
                    edge(path("crates/nc-schema"), Some("dev")),
                ] },
                { "id": path("crates/nc-guest"), "deps": [
                    edge(path("crates/nc-build"), Some("build")),
                    edge("registry+x#serde@1.0.0".to_owned(), None),
                ] },
                { "id": path("crates/nc-build"), "deps": [] },
                { "id": path("crates/nc-schema"), "deps": [] },
            ] }
        })
    }

    #[test]
    fn a_component_is_built_from_what_it_reaches_and_not_from_its_tests() {
        assert_eq!(
            super::built_from_in(&graph(), "nc-plugin-a").expect("the answer parses"),
            ["crates/nc-build", "crates/nc-guest", "plugins/a"],
            "a build edge is followed at depth, a registry crate is not a member, and a \
             dev-dependency is built for a test the component does not hold"
        );
    }

    #[test]
    fn a_package_that_is_not_a_member_is_refused_by_name() {
        let refused = super::built_from_in(&graph(), "serde").expect_err("not a member");
        assert!(refused.contains("`serde`"), "{refused}");
    }
}

/// A path's place under a root, or nothing when it only shares a prefix with it.
///
/// **A prefix is not a directory.** `/w-vendor` starts with `/w` and is a
/// sibling of it: measured, a textual prefix admits such a package as a member
/// with the nonsense relative path `-vendor`, and three pinned counts then fail
/// for a reason that names the wrong thing. What makes a path *inside* another
/// is a separator after it.
///
/// **One copy, for the three places that need it.** Written out at each, a copy
/// can be decided by nothing: on a target's `src_path`, deleting such a copy's
/// separator test would change no verdict anywhere, and `Member::sources` is
/// the floor under the type wall's count of source roots. A rule with one copy
/// has one test.
///
/// **And either separator, answered with `/`.** On Windows cargo writes every
/// manifest path with a backslash, under a root written the same way, so
/// read with `/` alone no member is found there, and every gate scoped by the
/// members fails for having nothing to judge. Both spellings are turned
/// into `/` before they are compared, which is the rule [`crate::relative_to`]
/// keeps for every path this tool prints or compares, and what comes back is
/// the form every list here is written in.
fn inside_the_root(root: &str, path: &str) -> Option<String> {
    let (root, path) = (root.replace('\\', "/"), path.replace('\\', "/"));
    path.strip_prefix(&root)
        .filter(|rest| rest.starts_with('/'))
        .map(|rest| rest.trim_start_matches('/').to_owned())
}

/// The same question, asked of an answer that is already parsed.
///
/// Split out so it can be driven: the writer of this list — the one `if` that
/// decides whether a package outside the tree is reported — is cold in a clean
/// tree, so deleting it leaves `cargo xtask check` printing
/// `nothing is built from outside this repository, of 0 path package(s)`
/// exactly as before. It is cold for an ordinary reason: this workspace has no
/// such package, so in a clean tree the branch is never taken.
/// A gate that has never had anything to judge is one nobody knows is wired up.
///
/// # Errors
///
/// Returns why the answer could not be read.
fn outside_of(parsed: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let workspace = parsed
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "`cargo metadata` named no workspace root".to_owned())?;
    let packages = parsed
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` listed no packages".to_owned())?;
    let mut outside = Vec::new();
    for package in packages {
        // A registry crate carries a `source`; a path dependency carries none.
        if !package
            .get("source")
            .is_some_and(serde_json::Value::is_null)
        {
            continue;
        }
        let (Some(name), Some(manifest)) = (
            package.get("name").and_then(serde_json::Value::as_str),
            package
                .get("manifest_path")
                .and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        if inside_the_root(workspace, manifest).is_none() {
            outside.push((name.to_owned(), manifest.to_owned()));
        }
    }
    outside.sort();
    Ok(outside)
}

#[cfg(test)]
mod outside_tests {
    /// One package inside the tree, one outside, and the answer names the second.
    #[test]
    fn a_package_resolved_from_outside_the_tree_is_named() {
        let answer = serde_json::json!({
            "workspace_root": "/w",
            "packages": [
                { "name": "nc-inside", "source": null,
                  "manifest_path": "/w/crates/nc-inside/Cargo.toml" },
                { "name": "nc-outside", "source": null,
                  "manifest_path": "/elsewhere/nc-outside/Cargo.toml" },
                // A registry crate carries a source and is never a path package,
                // wherever cargo unpacked it.
                { "name": "serde", "source": "registry+https://github.com/rust-lang/crates.io-index",
                  "manifest_path": "/home/someone/.cargo/registry/src/serde/Cargo.toml" },
            ]
        });
        assert_eq!(
            super::outside_of(&answer).expect("the answer parses"),
            vec![(
                "nc-outside".to_owned(),
                "/elsewhere/nc-outside/Cargo.toml".to_owned()
            )],
            "a path package outside the workspace root is what this reports, and it is \
             the only writer of the list `check` refuses on"
        );
    }

    /// And a tree with nothing outside it answers with nothing.
    ///
    /// The floor: a writer that reported *every* package would satisfy the test
    /// above by naming the outside one among them.
    #[test]
    fn a_tree_with_nothing_outside_it_reports_nothing() {
        let answer = serde_json::json!({
            "workspace_root": "/w",
            "packages": [
                { "name": "nc-inside", "source": null,
                  "manifest_path": "/w/crates/nc-inside/Cargo.toml" },
            ]
        });
        assert_eq!(
            super::outside_of(&answer).expect("the answer parses"),
            [] as [(std::string::String, std::string::String); 0]
        );
    }

    /// A prefix is not a directory.
    ///
    /// `/w-vendor` starts with `/w` and is a sibling, not a child. Measured: read
    /// as a textual prefix, it is admitted as a member with the nonsense
    /// relative path `-vendor`, and three pinned counts fail for the wrong
    /// reason.
    #[test]
    fn a_sibling_whose_name_begins_with_the_roots_is_outside() {
        let answer = serde_json::json!({
            "workspace_root": "/w",
            "packages": [
                { "name": "nc-vendored", "source": null,
                  "manifest_path": "/w-vendor/nc-vendored/Cargo.toml" },
            ]
        });
        assert_eq!(
            super::outside_of(&answer).expect("the answer parses").len(),
            1,
            "`/w-vendor` is a sibling of `/w`, not a directory inside it"
        );
    }
}

/// Every package id from which `nc-log` is reachable, itself excluded.
///
/// Walks `resolve.nodes` outward from the log. A crate that depends on one that
/// re-exports the log can call it, and its own manifest says nothing about that
/// — so the question the type wall's scope asks is reachability, and cargo's
/// resolved graph is where that is written.
///
/// Empty when the answer carries no `resolve` — which is what `--no-deps`
/// produces, and is why this function's caller does not pass it.
fn reaching_the_log(parsed: &serde_json::Value, packages: &[serde_json::Value]) -> Vec<String> {
    let log: Vec<&str> = packages
        .iter()
        .filter(|package| package.get("name").and_then(serde_json::Value::as_str) == Some(LOG))
        .filter_map(|package| package.get("id").and_then(serde_json::Value::as_str))
        .collect();
    let Some(nodes) = parsed
        .get("resolve")
        .and_then(|resolve| resolve.get("nodes"))
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    // `id -> what it depends on`, then the transitive closure the other way.
    let mut reaches: Vec<String> = Vec::new();
    let mut frontier: Vec<String> = log.iter().map(|id| (*id).to_owned()).collect();
    while let Some(target) = frontier.pop() {
        for node in nodes {
            let Some(id) = node.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if reaches.iter().any(|seen| seen == id) || log.contains(&id) {
                continue;
            }
            let depends = node
                .get("deps")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|deps| {
                    deps.iter().any(|dep| {
                        dep.get("pkg").and_then(serde_json::Value::as_str) == Some(&target)
                    })
                });
            if depends {
                reaches.push(id.to_owned());
                frontier.push(id.to_owned());
            }
        }
    }
    reaches
}

/// The packages whose normal dependency hands a member SQLite: the driver's
/// own crates, `sqlx-core` — whose `AnyPool` connects through whichever driver
/// the build registered — and `nc-local`, whose `LocalStore::pool` is a
/// connection to the user's file. `sqlx` is judged by its features instead,
/// below, because the server's crates take it for PostgreSQL.
#[cfg(test)]
const OPENS_SQLITE: [&str; 5] = [
    "nc-local",
    "sqlx-sqlite",
    "sqlx-core",
    "libsqlite3-sys",
    "rusqlite",
];

/// The members that can run a statement on the local database: each one whose
/// manifest names, as a normal or a build dependency, `sqlx` with a `sqlite`
/// feature or its `any` feature, or a package in [`OPENS_SQLITE`].
///
/// **`any` too, whatever it is asked for beside it.** `AnyPool` connects to a
/// `sqlite:` URL through the driver any crate of the build registered, and
/// `nc-local` is in every build that holds the kernel, which turns on the
/// `any` feature of `sqlx-sqlite`: with `any` unread, `nc-journal` given `sqlx`
/// with `any` alone could drop a trigger of the user's file through it with
/// every gate green, the member read by neither list.
///
/// **The declaration, not the graph, and cargo's reading of it.** The graph
/// unifies features across the workspace, so every crate that takes `sqlx` for
/// PostgreSQL reaches `sqlx-sqlite` in it the moment one member enables
/// SQLite; what a member *declares* is what it asked for. Cargo's answer
/// already carries the features `[workspace.dependencies]` contributes and the
/// real name behind a `package =` rename, which a manifest read on its own does
/// not. A dev-dependency is left out: a test tampers with the file on purpose.
///
/// # Errors
///
/// Returns why `cargo metadata` could not be read.
#[cfg(test)]
pub(crate) fn opening_the_local_database(root: &Path) -> Result<Vec<String>, String> {
    opening_the_local_database_in(&metadata(root)?)
}

/// The same question, asked of an answer that is already parsed, so each
/// clause can be driven without a workspace on disk.
#[cfg(test)]
fn opening_the_local_database_in(parsed: &serde_json::Value) -> Result<Vec<String>, String> {
    let root = parsed
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "`cargo metadata` named no workspace root".to_owned())?;
    let members: Vec<&str> = parsed
        .get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` listed no workspace members".to_owned())?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let packages = parsed
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` listed no packages".to_owned())?;
    let mut found = Vec::new();
    for package in packages {
        let is_a_member = package
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| members.contains(&id));
        let directory = package
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .and_then(|manifest| inside_the_root(root, manifest))
            .and_then(|rest| {
                rest.strip_suffix("Cargo.toml")
                    .map(|directory| directory.trim_end_matches('/').to_owned())
            });
        let (true, Some(directory)) = (is_a_member, directory) else {
            continue;
        };
        let opens = package
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|dependencies| dependencies.iter().any(hands_over_sqlite));
        if opens {
            found.push(directory);
        }
    }
    found.sort();
    Ok(found)
}

/// Every feature cargo resolves for the registry package `name`, in any build
/// of this workspace: none when nothing here takes it.
///
/// **What a feature list in a manifest cannot say.** A feature one crate asks
/// of a dependency is on for every crate built beside it, so the kernel's
/// `default-features = false` holds only while nothing else in the graph turns
/// the default back on; the resolved graph is what the build gets.
///
/// # Errors
///
/// Returns why `cargo metadata` could not be read.
#[cfg(test)]
pub(crate) fn resolved_features(root: &Path, name: &str) -> Result<Vec<String>, String> {
    resolved_features_in(&metadata(root)?, name)
}

/// The same question, asked of an answer that is already parsed.
#[cfg(test)]
fn resolved_features_in(parsed: &serde_json::Value, name: &str) -> Result<Vec<String>, String> {
    let packages = parsed
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` listed no packages".to_owned())?;
    let nodes = parsed
        .get("resolve")
        .and_then(|resolve| resolve.get("nodes"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` answered with no resolved graph".to_owned())?;
    let ids: Vec<&str> = packages
        .iter()
        .filter(|package| {
            package.get("name").and_then(serde_json::Value::as_str) == Some(name)
                && package
                    .get("source")
                    .is_some_and(|source| !source.is_null())
        })
        .filter_map(|package| package.get("id").and_then(serde_json::Value::as_str))
        .collect();
    let mut features: Vec<String> = nodes
        .iter()
        .filter(|node| {
            node.get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| ids.contains(&id))
        })
        .filter_map(|node| node.get("features").and_then(serde_json::Value::as_array))
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect();
    features.sort();
    features.dedup();
    Ok(features)
}

/// Whether one declared dependency hands its member SQLite.
#[cfg(test)]
fn hands_over_sqlite(dependency: &serde_json::Value) -> bool {
    let normal = dependency
        .get("kind")
        .is_none_or(|kind| kind.is_null() || kind.as_str() == Some("build"));
    let name = dependency
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let sqlite_feature = dependency
        .get("features")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|features| {
            features
                .iter()
                .filter_map(serde_json::Value::as_str)
                .any(|feature| feature.starts_with("sqlite") || feature == "any")
        });
    normal && (OPENS_SQLITE.contains(&name) || (name == "sqlx" && sqlite_feature))
}

/// Whether `cargo doc --workspace` is asked to document this target.
///
/// **Its kind, and not its `doc` field.** Cargo reports `"doc": false` for a
/// target a manifest switched off, so an expectation read from that field is the
/// manifest agreeing with itself — see [`Member::documented`] for the pair that
/// measured it. A library or a binary is documented; an integration test, a
/// bench and an example are not, which is what `cargo doc --workspace --no-deps`
/// does.
fn is_documented(target: &serde_json::Value) -> bool {
    target
        .get("kind")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|kinds| {
            kinds
                .iter()
                .filter_map(serde_json::Value::as_str)
                // **And the two `cargo doc` also builds.** Without `cdylib` and
                // `staticlib`, a member that ships one drops out of the only
                // floor under the documentation step — measured outside this
                // tree, because no member ships either today.
                .any(|kind| {
                    matches!(
                        kind,
                        "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro" | "bin"
                    )
                })
        })
}

/// The members in one `cargo metadata` answer.
///
/// Split from the call so the shape can be tested without a workspace on disk,
/// and so a metadata answer this cannot read is a stated error rather than an
/// empty list — an empty list is a scope that covers nothing, which is what
/// both misses in this module's header look like from the outside.
fn from_metadata(parsed: &serde_json::Value) -> Result<Vec<Member>, String> {
    let root = parsed
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "`cargo metadata` named no workspace root".to_owned())?;
    let packages = parsed
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`cargo metadata` listed no packages".to_owned())?;

    let reaches = reaching_the_log(parsed, packages);
    let mut found = Vec::new();
    for package in packages {
        let Some(manifest) = package
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        // `<root>/crates/nc-log/Cargo.toml` is the crate at `crates/nc-log`.
        // The root package itself, whose manifest is `<root>/Cargo.toml`, has an
        // empty directory and is the workspace — kept, because a root package
        // that logs is a crate that logs.
        // **And a prefix is not a directory here either.** `outside_of` knows
        // that `/w-vendor` is a sibling of `/w` and not a child of it, and this
        // function — the one that decides the scope of every member-keyed gate
        // in the tree — has to know it too. With the bare textual prefix, a
        // package at `/w-vendor/nc-vendored/Cargo.toml` is
        // admitted with the relative path `-vendor/nc-vendored`, so the lint walk, the source walk and the type-wall scope all judge a manifest outside the repository and count it among the members. <!-- allow-reference: `-vendor/nc-vendored` is the nonsense path the defect produced, in a fixture workspace at `/w` -->
        // A second copy of one rule is a copy proved by nothing.
        let Some(relative) = inside_the_root(root, manifest).and_then(|rest| {
            rest.strip_suffix("Cargo.toml")
                .map(|directory| directory.trim_end_matches('/').to_owned())
        }) else {
            continue;
        };
        // **Reaches the log, at any depth.** `cargo metadata` has already
        // resolved `mylog = { package = "nc-log" }` and the workspace
        // inheritance behind it, and the graph below resolves a re-export's
        // consumer too. Asking a manifest for its own keys misses both.
        let can_log = package
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| reaches.iter().any(|reaching| reaching == id));
        let targets = package
            .get("targets")
            .and_then(serde_json::Value::as_array)
            .map_or_else(Vec::new, |targets| {
                targets
                    .iter()
                    .filter_map(|target| {
                        let relative = target
                            .get("src_path")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|path| inside_the_root(root, path))?;
                        Some((relative, is_documented(target)))
                    })
                    .collect::<Vec<(String, bool)>>()
            });
        let sources: Vec<String> = targets
            .iter()
            .map(|(relative, _)| relative.clone())
            .collect();
        let documented: Vec<String> = targets
            .into_iter()
            .filter_map(|(relative, documented)| documented.then_some(relative))
            .collect();
        let Some(name) = package.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        found.push(Member {
            path: relative,
            name: name.to_owned(),
            can_log,
            sources,
            documented,
        });
    }
    if found.is_empty() {
        return Err("`cargo metadata` listed no workspace member".to_owned());
    }
    found.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::{from_metadata, is_documented, members};
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// The kernel's HTTP client is built with no decoder, no transcoding and
    /// no cookie jar, in any build of this workspace.
    ///
    /// **What this holds**: one word in ureq's features — or a crate built
    /// beside the kernel taking ureq's defaults — compiles in `gzip`, whose
    /// decoder sits outside the limit the response body is read to, so one
    /// answer at the bound inflates past it a thousandfold. `cookies` would
    /// keep one jar for every plugin's requests, and `charset` turns bytes into
    /// text the kernel never asks for.
    #[test]
    fn the_kernels_http_client_is_built_with_no_decoder_and_no_cookie_jar() {
        let features = super::resolved_features(root(), "ureq").expect("cargo metadata");
        assert!(
            features.iter().any(|feature| feature == "rustls"),
            "ureq is not built as the kernel takes it, so this asks nothing: {features:?}"
        );
        for refused in ["gzip", "brotli", "charset", "cookies"] {
            assert!(
                !features.iter().any(|feature| feature == refused),
                "ureq is built with `{refused}`: {features:?}. The response body is \
                 bounded under the decoder, and a jar is one for every plugin — keep it off \
                 wherever it came from"
            );
        }
    }

    /// The features resolved for a registry package are those of its node,
    /// every version of it together, and never a workspace member's of the
    /// same name.
    #[test]
    fn the_features_resolved_for_a_package_are_its_nodes() {
        let parsed = serde_json::json!({
            "packages": [
                {"name": "ureq", "id": "ureq 3.5.0", "source": "registry+crates.io"},
                {"name": "ureq", "id": "ureq 2.0.0", "source": "registry+crates.io"},
                {"name": "ureq", "id": "path+file:///w/ureq", "source": null},
            ],
            "resolve": {"nodes": [
                {"id": "ureq 3.5.0", "features": ["rustls", "gzip"]},
                {"id": "ureq 2.0.0", "features": ["rustls", "cookies"]},
                {"id": "path+file:///w/ureq", "features": ["charset"]},
            ]},
        });
        assert_eq!(
            super::resolved_features_in(&parsed, "ureq"),
            Ok(vec![
                "cookies".to_owned(),
                "gzip".to_owned(),
                "rustls".to_owned()
            ])
        );
        assert_eq!(
            super::resolved_features_in(&parsed, "nothing"),
            Ok(Vec::new())
        );
    }

    /// A member opens the local database by what it declares for its own
    /// build, and not by what a test of it takes or what the server's crates
    /// take for PostgreSQL.
    #[test]
    fn a_member_opens_the_local_database_by_what_it_declares() {
        let package = |name: &str, at: &str, dependencies: serde_json::Value| {
            serde_json::json!({
                "id": format!("{name} 0.1.0"),
                "name": name,
                "manifest_path": format!("/w/{at}/Cargo.toml"),
                "dependencies": dependencies,
            })
        };
        let dependency = |name: &str, kind: Option<&str>, features: &[&str]| serde_json::json!({ "name": name, "kind": kind, "features": features });
        let parsed = serde_json::json!({
            "workspace_root": "/w",
            "workspace_members": [
                "server 0.1.0", "local 0.1.0", "user 0.1.0", "tested 0.1.0", "built 0.1.0",
                "anything 0.1.0", "core 0.1.0"
            ],
            "packages": [
                package("server", "crates/server", serde_json::json!([
                    dependency("sqlx", None, &["postgres", "migrate"]),
                ])),
                package("local", "crates/local", serde_json::json!([
                    dependency("sqlx", None, &["postgres", "sqlite-bundled"]),
                ])),
                package("user", "crates/user", serde_json::json!([
                    dependency("nc-local", None, &[]),
                ])),
                package("tested", "crates/tested", serde_json::json!([
                    dependency("nc-local", Some("dev"), &[]),
                    dependency("sqlx", Some("dev"), &["sqlite"]),
                ])),
                package("built", "tools/built", serde_json::json!([
                    dependency("rusqlite", Some("build"), &[]),
                ])),
                package("anything", "crates/anything", serde_json::json!([
                    dependency("sqlx", None, &["postgres", "any"]),
                ])),
                package("core", "crates/core", serde_json::json!([
                    dependency("sqlx-core", None, &[]),
                ])),
                package("outside", "vendor/outside", serde_json::json!([
                    dependency("sqlx", None, &["sqlite"]),
                ])),
            ],
        });
        assert_eq!(
            super::opening_the_local_database_in(&parsed),
            Ok(vec![
                "crates/anything".to_owned(),
                "crates/core".to_owned(),
                "crates/local".to_owned(),
                "crates/user".to_owned(),
                "tools/built".to_owned(),
            ])
        );
        // And this tree, which is what the source gate is scoped by.
        assert_eq!(
            super::opening_the_local_database(root()).expect("cargo metadata"),
            [
                "crates/nc-kernel",
                "crates/nc-local",
                "crates/nc-prune",
                "xtask"
            ]
        );
    }

    /// Every target kind `cargo doc` builds, and the ones it does not.
    ///
    /// **Three of the five names decide nothing in this tree.** This workspace
    /// has `lib` and `bin` targets and no `rlib`, `dylib` or `proc-macro`, so
    /// no member reaches those arms, and this test hands each of them a target.
    /// Without it, a crate that gains a procedural macro would be documented or
    /// not by a clause nobody had run.
    #[test]
    fn every_kind_cargo_documents_is_named_and_the_others_are_not() {
        let target = |kind: &str| serde_json::json!({ "kind": [kind] });
        for kind in [
            "lib",
            "rlib",
            "dylib",
            "cdylib",
            "staticlib",
            "proc-macro",
            "bin",
        ] {
            assert!(
                is_documented(&target(kind)),
                "`cargo doc` builds a `{kind}` target and this says it does not"
            );
        }
        for kind in ["test", "bench", "example", "custom-build"] {
            assert!(
                !is_documented(&target(kind)),
                "`cargo doc` does not build a `{kind}` target and this says it does"
            );
        }
        // A target with no `kind` at all is not documented, and does not panic.
        assert!(!is_documented(&serde_json::json!({ "name": "x" })));
    }

    #[test]
    fn this_workspaces_members_are_the_ones_cargo_builds() {
        let found = members(root()).expect("cargo metadata runs here");
        let paths: Vec<&str> = found.iter().map(|member| member.path.as_str()).collect();
        assert!(
            paths.contains(&"crates/nc-log") && paths.contains(&"xtask"),
            "{paths:?}"
        );
        // The floor is a real number, not `> 0`: this workspace has sixteen
        // members and a resolution that lost one is the defect being closed.
        assert_eq!(found.len(), 16, "{paths:?}");
        // And `nc-log` does not depend on itself, so the consumer count this
        // feeds cannot be inflated by the log's own manifest.
        assert!(
            found
                .iter()
                .all(|member| !member.can_log || member.path != "crates/nc-log"),
            "nc-log was counted as a consumer of itself"
        );
    }

    /// Every spelling that gives a crate the log, asked of cargo itself.
    ///
    /// **A hand-written manifest reader answers this question short**, switching
    /// the type wall's source gate off for a crate holding a real logging call
    /// site, with `check passed`. So the spellings are written out here and the
    /// answer comes from `cargo metadata` — one real workspace on disk, one
    /// call, and cargo deciding what a dependency is.
    ///
    /// The last two rows are the ones such a reader misses: a dependency renamed
    /// in `[workspace.dependencies]`, which no member's own manifest names, and a
    /// path dependency inside the workspace directory, which is a member cargo
    /// builds without `members` listing it.
    #[test]
    fn every_spelling_that_reaches_the_log_is_cargos_answer_and_not_a_list() {
        let scratch = std::env::temp_dir().join(format!("nc-members-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);

        let crate_at = |path: &str, name: &str, body: &str| {
            let directory = scratch.join(path);
            std::fs::create_dir_all(directory.join("src")).expect("writable");
            std::fs::write(
                directory.join("Cargo.toml"),
                format!(
                    "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{body}"
                ),
            )
            .expect("writable");
            std::fs::write(directory.join("src/lib.rs"), "//! probe\n").expect("writable");
        };

        std::fs::create_dir_all(&scratch).expect("writable");
        std::fs::write(
            scratch.join("Cargo.toml"),
            "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n\
             [workspace.dependencies]\n\
             nc-log = { path = \"crates/nc-log\" }\n\
             mylog = { package = \"nc-log\", path = \"crates/nc-log\" }\n",
        )
        .expect("writable");

        crate_at("crates/nc-log", "nc-log", "");
        crate_at("crates/nc-logger", "nc-logger", "");
        // `(directory, the manifest body, whether cargo says it reaches nc-log)`.
        let spellings: [(&str, &str, bool); 11] = [
            (
                "crates/pathed",
                "[dependencies]\nnc-log = { path = \"../nc-log\" }\n",
                true,
            ),
            // The house style: every internal dependency in this tree is dotted.
            (
                "crates/dotted",
                "[dependencies]\nnc-log.workspace = true\n",
                true,
            ),
            (
                "crates/tabled",
                "[dependencies.nc-log]\nworkspace = true\n",
                true,
            ),
            (
                "crates/dev",
                "[dev-dependencies.nc-log]\nworkspace = true\n",
                true,
            ),
            (
                "crates/targeted",
                "[target.'cfg(unix)'.dependencies.nc-log]\nworkspace = true\n",
                true,
            ),
            (
                "crates/quoted",
                "[dependencies]\n\"nc-log\".workspace = true\n",
                true,
            ),
            // **The rename.** Nothing in this manifest spells `nc-log`.
            (
                "crates/renamed",
                "[dependencies]\nmylog.workspace = true\n",
                true,
            ),
            // A longer name beginning the same way is a different crate.
            (
                "crates/near-miss",
                "[dependencies]\nnc-logger = { path = \"../nc-logger\" }\n",
                false,
            ),
            ("crates/quiet", "", false),
            // **An optional dependency behind a feature that is off.** `cargo
            // metadata` resolves *default* features, so without `--all-features`
            // this edge is absent from the graph and the crate's whole source
            // sits outside the type wall's scope — measured with a
            // `format!(…).leak()` at a logging call site in it and
            // `check passed`.
            (
                "crates/optional",
                "[dependencies]\nnc-log = { path = \"../nc-log\", optional = true }\n\
                 [features]\nnoisy = [\"dep:nc-log\"]\n",
                true,
            ),
            // **The inferred member.** Its directory is not in `members`, and cargo
            // makes it a member anyway because a member depends on it by path.
            //
            // It reaches the log *through* that member, which is the second
            // thing this row is for: a crate that `pub use nc_log;` hands the
            // log to everything depending on it, and the consumer's own
            // manifest never spells it.
            (
                "crates/reaches-out",
                "[dependencies]\ninferred = { path = \"../../tools/inferred\" }\n",
                true,
            ),
        ];
        for (path, body, _) in spellings {
            let name = path.rsplit('/').next().expect("a directory name");
            crate_at(path, name, body);
        }
        crate_at(
            "tools/inferred",
            "inferred",
            "[dependencies]\nnc-log = { path = \"../../crates/nc-log\" }\n",
        );

        let found = members(&scratch).expect("cargo reads the scratch workspace");
        let answer = |path: &str| {
            found
                .iter()
                .find(|member| member.path == path)
                .unwrap_or_else(|| panic!("cargo did not list {path}: {found:#?}"))
                .can_log
        };
        for (path, body, reaches) in spellings {
            assert_eq!(
                answer(path),
                reaches,
                "cargo and this gate disagree about {path}:\n{body}"
            );
        }
        // The log itself takes no dependency on itself, and the inferred member
        // — which `members` never listed — is both present and reported as
        // reaching the log.
        assert!(!answer("crates/nc-log"));
        assert!(
            answer("tools/inferred"),
            "an inferred member was not resolved: {found:#?}"
        );
        assert_eq!(found.len(), 14, "{found:#?}");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A rename is resolved, and a member cargo infers is listed.
    ///
    /// Both of these switch the type-wall scope off under a manifest reader, with
    /// `check passed`, and neither is visible in the manifest such a reader
    /// opens: the rename is declared in the workspace table two directories
    /// up, and the inferred member is named in a *sibling's* dependency list.
    #[test]
    fn a_renamed_dependency_and_an_inferred_member_are_both_resolved() {
        let answer = serde_json::json!({
            "workspace_root": "/w",
            "packages": [
                {"name": "nc-log", "id": "log", "manifest_path": "/w/crates/nc-log/Cargo.toml"},
                {"name": "loud", "id": "loud", "manifest_path": "/w/crates/loud/Cargo.toml"},
                {"name": "quiet", "id": "quiet", "manifest_path": "/w/tools/quiet/Cargo.toml"},
            ],
            "resolve": {"nodes": [
                {"id": "log", "deps": []},
                {"id": "loud", "deps": [{"pkg": "log"}]},
                {"id": "quiet", "deps": []},
            ]},
        });
        let found = from_metadata(&answer).expect("a readable answer");
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].path, "crates/loud");
        assert!(found[0].can_log, "the rename hid the log");
        assert_eq!(found[1].path, "crates/nc-log");
        assert!(!found[1].can_log, "the log was counted as reaching itself");
        assert_eq!(found[2].path, "tools/quiet");
        assert!(!found[2].can_log);
    }

    /// An answer this cannot read is an error, never an empty scope.
    ///
    /// An empty list would read as "no crate may log", which is precisely how
    /// both misses above present — a gate reporting `ok` about nothing.
    #[test]
    fn an_unreadable_answer_is_said_rather_than_returned_as_no_members() {
        for unreadable in [
            serde_json::json!({"packages": []}),
            serde_json::json!({"workspace_root": "/w"}),
            serde_json::json!({"workspace_root": "/w", "packages": []}),
        ] {
            assert!(
                from_metadata(&unreadable).is_err(),
                "{unreadable} was read as a workspace"
            );
        }
    }

    /// A graph that failed for a package not on this machine names the fetch,
    /// and one that failed for anything else does not.
    ///
    /// The first case is cargo's answer on a clone whose cargo home holds
    /// nothing, measured with `CARGO_HOME` pointed at an empty directory.
    #[test]
    fn a_package_missing_offline_is_answered_with_the_fetch() {
        let missing = "error: no matching package named `cedar-policy` found\nlocation \
                       searched: crates.io index\nnote: offline mode (via `--offline`) can \
                       sometimes cause surprising resolution failures\n";
        let said = super::what_a_failed_graph_says(missing);
        assert!(
            said.contains("no matching package named `cedar-policy`")
                && said.contains("`cargo fetch --locked`"),
            "a graph that failed for a package not downloaded did not name the fetch: {said}"
        );
        let broken = "error: failed to parse manifest at `/w/Cargo.toml`\n";
        let said = super::what_a_failed_graph_says(broken);
        assert!(
            said.contains("failed to parse manifest") && !said.contains("cargo fetch"),
            "a manifest cargo could not parse was answered with a download: {said}"
        );
    }

    /// A sibling of the root is not a member of it.
    ///
    /// The same measurement as `a_sibling_whose_name_begins_with_the_roots_is_outside`,
    /// against the function beside it: `/w-vendor` starts with `/w`, and this is
    /// the reader whose answer is the scope of every member-keyed gate. Read as a
    /// textual prefix, it admits the package with the relative path `-vendor/nc-vendored`, and `crates/nc-log`'s own source list would take a `src_path` from there too. <!-- allow-reference: `-vendor/nc-vendored` is the nonsense path the defect produced -->
    #[test]
    fn a_sibling_of_the_root_is_not_a_member() {
        let answer = serde_json::json!({
            "workspace_root": "/w",
            "packages": [
                {"name": "nc-log", "id": "log", "manifest_path": "/w/crates/nc-log/Cargo.toml"},
                {"name": "nc-vendored", "id": "vendored",
                 "manifest_path": "/w-vendor/nc-vendored/Cargo.toml",
                 "targets": [{"src_path": "/w-vendor/nc-vendored/src/lib.rs"}]},
            ],
        });
        let found = from_metadata(&answer).expect("a readable answer");
        let paths: Vec<&str> = found.iter().map(|member| member.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["crates/nc-log"],
            "`/w-vendor` is a sibling of `/w`, not a directory inside it"
        );
    }

    /// A member cargo names with Windows' separator is a member, written with `/`.
    ///
    /// The answer on a Windows runner, in the shape it came: read with `/` as
    /// the only separator, no package in it is inside the root and the walk
    /// answers that cargo lists no member at all. A sibling is still outside
    /// with either separator, because what decides inside is still a separator
    /// after the root.
    #[test]
    fn a_member_cargo_names_with_backslashes_is_a_member() {
        let answer = serde_json::json!({
            "workspace_root": "D:\\w",
            "packages": [
                {"name": "nc-log", "id": "log",
                 "manifest_path": "D:\\w\\crates\\nc-log\\Cargo.toml",
                 "targets": [{"src_path": "D:\\w\\crates\\nc-log\\src\\lib.rs", "kind": ["lib"]}]},
                {"name": "nc-vendored", "id": "vendored",
                 "manifest_path": "D:\\w-vendor\\nc-vendored\\Cargo.toml"},
            ],
        });
        let found = from_metadata(&answer).expect("a readable answer");
        let read: Vec<(&str, &[String])> = found
            .iter()
            .map(|member| (member.path.as_str(), member.sources.as_slice()))
            .collect();
        assert_eq!(
            read,
            vec![(
                "crates/nc-log",
                &["crates/nc-log/src/lib.rs".to_owned()][..]
            )],
            "a Windows answer is read as the members it names, with `/`, and its sibling \
             is not one of them"
        );
    }

    /// Cargo is asked for the resolved graph in exactly one place, with these
    /// five arguments.
    ///
    /// **Two copies of the argument list drift.** `--all-features` decides
    /// whether an optional dependency is in the graph at all — so a flag added
    /// to one copy and not the other silently scopes half the gates
    /// differently.
    ///
    /// **And counting one spelling is wrong in both directions at once.** A rule
    /// that counts `"metadata",` and `"--all-features",` adjacent, at twelve
    /// spaces of indentation, lets a second reader asking cargo a different
    /// question add no match and pass, which is the exact drift such a rule
    /// names; while reordering the same two flags, which cargo cannot tell
    /// apart, takes the count to zero and fails saying the graph is asked for in
    /// **0** places.
    ///
    /// So it is two questions about the thing itself. How many times this file
    /// starts cargo — once — and what that one invocation asks for, as a
    /// **set**, because the order of five flags is not a property of anything.
    /// The needles are assembled, because a test that scans for a string is
    /// another copy of it.
    #[test]
    fn cargo_is_asked_for_the_graph_in_one_place() {
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/members.rs"))
                .expect("this file is readable");
        // **Every comment, not the one spelling.** `line_without_a_comment`
        // knows `//` and nothing else, so a `/* … */` naming this needle pays
        // for the invocation it names. `without_comments` is the whole-file
        // state machine, and it keeps the string literals the needle is made
        // of.
        let code = crate::gates::without_comments(&source, "//");
        let starts = format!("Command::new(\"{}\")", "cargo");
        let asked = code.matches(&starts).count();
        assert_eq!(
            asked, 1,
            "cargo is started {asked} time(s) in this file. One invocation, in \
             `metadata`, or the readers can be scoped by different flags with both \
             counts still printed"
        );
        let at = code.find(&starts).expect("the one invocation is here");
        let after = &code[at..];
        let opens = after.find(".args([").expect("its argument list follows");
        let closes = after[opens..].find("])").expect("and the list closes") + opens;
        let mut flags: Vec<&str> = after[opens + ".args([".len()..closes]
            .split(',')
            .map(|word| word.trim().trim_matches('"'))
            .filter(|word| !word.is_empty())
            .collect();
        flags.sort_unstable();
        assert_eq!(
            flags,
            [
                "--all-features",
                "--format-version",
                "--offline",
                "1",
                "metadata"
            ],
            "the one invocation asks cargo for something other than the fully-featured \
             offline graph. `--all-features` is what decides whether an optional \
             dependency is in the answer at all, and a crate missing from the answer is \
             a crate outside every member-keyed gate"
        );
    }
}
