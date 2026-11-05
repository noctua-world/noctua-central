//! The plugins, built for `wasm32` and encoded as components (`NOCTUA.md §11.3`).
//!
//! **Built by this tool, because `§19.4` says the command surface does not
//! grow.** A plugin is a member of the workspace, so the host target lints,
//! documents and tests it with everything else; what the host cannot do is
//! build the thing the kernel loads. For each member under `plugins/`, this:
//!
//! 1. builds its library for `wasm32-unknown-unknown`, `--release` and
//!    `--locked` — and `--offline` when the run is — into `target/plugins`, a <!-- allow-reference: `target/plugins` is what this build writes, and the reference step reads this line before it runs, so a fresh checkout has none yet -->
//!    target directory of its own so a plugin build never invalidates the
//!    host's;
//! 2. encodes the core module as a component with `wit-component`'s
//!    `ComponentEncoder`, validated and with **no WASI adapter**, then decodes
//!    the component and refuses it if it imports anything the world `plugin`
//!    in `crates/nc-kernel/wit` does not declare ([`outside_the_world`]).
//!    The encoder alone does not: it encodes against **every** world the module
//!    carries, and a dependency can carry its own — measured, `wasip2` added to
//!    the reference plugin and called once encodes without a word, and a kernel
//!    handed that component would find `get-random-u64`, from `wasi:random`,
//!    among its imports. What the encoder does refuse is an import no carried world
//!    describes, such as a hand-written `extern` block;
//! 3. writes the component to `target/plugins/components/<dir>.wasm`, and
//!    beside it `<dir>.inputs` — every file the component was built from, with
//!    its BLAKE3 — each through a temporary file and a rename, so a reader never
//!    sees half of either;
//! 4. lints the member with clippy at `-D warnings`, and documents it with
//!    rustdoc at `-D warnings`, **for `wasm32`**: a plugin's adapter to the world
//!    is compiled for that target only, so the host's lint and documentation
//!    steps never read it and this is the only run that does.
//!
//! `crates/nc-kernel/src/built.rs` reads the inputs list back before a kernel
//! suite loads a component, and panics on one that no longer matches its
//! sources rather than running against it. The list's format is that file's
//! header, and [`listing`] is the one writer of it.
//!
//! What this answers (`NOCTUA.md §2.4`): a component the kernel's suites load
//! is the one its sources build today, and a component this builds imports
//! nothing the world does not declare. What it does not answer: a component the
//! kernel is handed from anywhere else, whose imports are the kernel's to judge;
//! and what a component does at run time, which is the kernel's to bound.

use std::path::Path;

use crate::ui;

/// The one target a plugin is built for.
///
/// `rust-toolchain.toml` and every job in `ci.yml` that runs the gate ask for
/// it, and a test in `workflow.rs` holds each of them to that.
pub(crate) const TARGET: &str = "wasm32-unknown-unknown";

/// Where plugin builds go, relative to the repository.
const OUT: &str = "target/plugins";

/// The world the guest bindings are generated from.
///
/// The one input of a component no manifest names, because `nc-guest` reads it
/// by path.
const WORLD: &str = "crates/nc-kernel/wit";

/// What `cargo xtask build --plugins` runs: the same build `check` runs.
///
/// # Errors
///
/// Returns every plugin that could not be built, encoded, linted or
/// documented.
pub(crate) fn run(root: &Path) -> Result<(), String> {
    ui::heading("plugins");
    let built = build_every_plugin(root, false)?;
    ui::ok(&what_was_built(built));
    Ok(())
}

/// The `plugins` step of `check`.
pub(crate) fn check(root: &Path, offline: bool, failures: &mut Vec<String>) {
    ui::heading("plugins");
    match build_every_plugin(root, offline) {
        Ok(built) => ui::ok(&what_was_built(built)),
        Err(problem) => {
            ui::failure(&problem);
            failures.push("the plugins".to_owned());
        }
    }
}

/// The line a run that built every plugin prints.
fn what_was_built(built: usize) -> String {
    format!("{built} component(s) built for wasm32, componentized, linted and documented")
}

/// Every plugin member, built and judged; the count, floored by equality.
///
/// **Against the members cargo resolves, not against the directory.** A plugin
/// the walk skipped and one it never found print the same smaller number, so the
/// count is compared with `cargo metadata`'s list of members under `plugins/` —
/// and a tree with none is a failure, because a step that built nothing has
/// shown nothing.
fn build_every_plugin(root: &Path, offline: bool) -> Result<usize, String> {
    let plugins: Vec<crate::members::Member> = crate::members::members(root)?
        .into_iter()
        .filter(|member| member.path.starts_with("plugins/"))
        .collect();
    if plugins.is_empty() {
        return Err(
            "cargo resolves no member under `plugins/`, so no component was built and \
             nothing the kernel's suites load was shown to be current. The members glob \
             in `Cargo.toml` names `plugins/*`"
                .to_owned(),
        );
    }
    let mut problems = Vec::new();
    let mut built = 0_usize;
    for plugin in &plugins {
        match build_one(root, plugin, offline) {
            Ok(()) => built += 1,
            Err(problem) => problems.push(problem),
        }
    }
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    if built != plugins.len() {
        return Err(format!(
            "{built} component(s) were built and cargo resolves {} plugin member(s): \
             something between the two skipped a plugin, and a component nobody rebuilt \
             is one a suite would load stale",
            plugins.len()
        ));
    }
    Ok(built)
}

/// The directory a plugin sits in, once its package is shown to be named for it.
///
/// **One name, read two ways.** The component is written under the
/// directory's name, and the mutation runner's meta-gate reads a plugin's
/// package from its path — so `plugins/<dir>` is the package `nc-plugin-<dir>`,
/// and a plugin named anything else is refused here rather than built under a
/// name those two would not find.
fn directory_of(plugin: &crate::members::Member) -> Result<&str, String> {
    let directory = plugin
        .path
        .strip_prefix("plugins/")
        .ok_or_else(|| format!("`{}` is not under `plugins/`", plugin.path))?;
    if plugin.name == format!("nc-plugin-{directory}") {
        Ok(directory)
    } else {
        Err(format!(
            "`{}` is the package `{}`, and a plugin under `plugins/<dir>` is the package \
             `nc-plugin-<dir>`: its component is written under the directory's name and \
             the mutation runner reads the package from the path. Rename the package",
            plugin.path, plugin.name
        ))
    }
}

/// The three cargo commands one plugin `name` is made with, into `out`: its
/// build, its lint and its documentation, each as the arguments cargo is
/// handed — the verb first, which is what `crate::check::cargo` hands
/// rustdoc its deny by.
///
/// **Every one for `wasm32`, and the lint and the documentation are the only
/// runs that read the adapter**, which compiles for that target alone. So the
/// arguments are a value a test holds: with the lint's `-D warnings` weakened
/// to `-W` and the documentation pointed at the host, each one token, the step
/// would go on printing `linted and documented` over a clippy violation and a
/// broken link in the adapter.
fn plugin_commands<'a>(name: &'a str, out: &'a str, offline: bool) -> [Vec<&'a str>; 3] {
    let mut common = vec![
        "-p",
        name,
        "--lib",
        "--locked",
        "--target",
        TARGET,
        "--target-dir",
        out,
    ];
    if offline {
        common.push("--offline");
    }
    [
        [&["build", "--release"], &common[..]].concat(),
        [&["clippy"], &common[..], &["--", "-D", "warnings"]].concat(),
        [
            &["doc", "--no-deps", "--document-private-items"],
            &common[..],
        ]
        .concat(),
    ]
}

/// One plugin: built, encoded, written with its inputs, linted and documented.
fn build_one(root: &Path, plugin: &crate::members::Member, offline: bool) -> Result<(), String> {
    let directory = directory_of(plugin)?;
    let from = crate::members::built_from(root, &plugin.name)?;
    let inputs = inputs_of(root, &from)?;
    let out = root.join(OUT);
    let out = out.to_string_lossy();
    let [build, lint, document] = plugin_commands(&plugin.name, &out, offline);
    run_cargo(root, "build", &build, &plugin.path, offline)?;
    let library = plugin.name.replace('-', "_");
    let core_path = root
        .join(OUT)
        .join(TARGET)
        .join("release")
        .join(format!("{library}.wasm"));
    let core = std::fs::read(&core_path).map_err(|error| {
        format!(
            "`{}` built and `{}` is not there to encode ({error}): a plugin's library \
             keeps the name cargo gives its package",
            plugin.path,
            core_path.display()
        )
    })?;
    let world = declared(&root.join(WORLD))?;
    let component = encode(&core, &world).map_err(|error| {
        format!(
            "`{}` built and is not a component of the world: {error}. A component that \
             imports anything the world `plugin` in `{WORLD}` does not declare — a WASI \
             import a dependency brought included — is refused here, before any kernel \
             loads it",
            plugin.path
        )
    })?;
    let components = root.join(OUT).join("components");
    std::fs::create_dir_all(&components)
        .map_err(|error| format!("`{}` could not be made: {error}", components.display()))?;
    let written = format!("{OUT}/components/{directory}.wasm");
    write_atomically(&root.join(&written), &component)?;
    let listing = listing((&blake3::hash(&component).to_hex(), &written), &inputs);
    write_atomically(
        &components.join(format!("{directory}.inputs")),
        listing.as_bytes(),
    )?;
    run_cargo(root, "clippy", &lint, &plugin.path, offline)?;
    run_cargo(root, "doc", &document, &plugin.path, offline)?;
    Ok(())
}

/// One cargo command for one plugin, or what cargo said when it refused.
fn run_cargo(
    root: &Path,
    verb: &str,
    args: &[&str],
    plugin: &str,
    offline: bool,
) -> Result<(), String> {
    crate::check::cargo(root, args, offline)
        .map(drop)
        .map_err(|output| {
            format!(
                "`cargo {verb}` refused `{plugin}` for {TARGET}:\n{}",
                crate::check::what_cargo_said(&output).join("\n")
            )
        })
}

/// A core module, encoded as a component, once it is shown to reach nothing but `world`.
fn encode(core: &[u8], world: &Declared) -> Result<Vec<u8>, String> {
    let component = wit_component::ComponentEncoder::default()
        .module(core)
        .map_err(|error| format!("{error:#}"))?
        .validate(true)
        .encode()
        .map_err(|error| format!("{error:#}"))?;
    let outside = outside_the_world(&component, world)?;
    if outside.is_empty() {
        Ok(component)
    } else {
        Err(format!(
            "it reaches outside the world — {} — so the module carries a world beside \
             the plugin's own, the way a dependency with bindings of its own brings one",
            outside.join(", ")
        ))
    }
}

/// What the world `plugin` imports, each by the name a component imports it under.
///
/// That is the interface's package, name and version, and the interfaces whose
/// types those use are among them.
#[derive(Debug)]
struct Declared(std::collections::BTreeSet<String>);

/// The world `plugin` as the WIT in `directory` declares it.
///
/// Read from the text, not from a module: what a module carries is every world
/// its dependencies embedded, merged, which is the very thing being judged.
fn declared(directory: &Path) -> Result<Declared, String> {
    let mut resolve = wit_parser::Resolve::default();
    let (package, _) = resolve
        .push_path(directory)
        .map_err(|error| format!("`{}` does not parse: {error:#}", directory.display()))?;
    let world = resolve
        .select_world(&[package], Some("plugin"))
        .map_err(|error| format!("`{}` has no world `plugin`: {error:#}", directory.display()))?;
    Ok(Declared(
        resolve.worlds[world]
            .imports
            .keys()
            .map(|key| resolve.name_world_key(key))
            .collect(),
    ))
}

/// Every import of `component` that `world` does not declare.
///
/// **Read from the component, after encoding**, because that is where a second
/// world shows: the encoder merges every `component-type` section the module
/// carries — the plugin's own and any a dependency's bindings embedded — and
/// encodes against the union, so a stray import is one the component really
/// has. An interface a component uses only the types of is imported by name
/// like any other, and is judged the same way.
///
/// Imports only: an export is something the kernel may call, and reaches
/// nothing of the host; the kernel calls `guest` and nothing else.
fn outside_the_world(component: &[u8], world: &Declared) -> Result<Vec<String>, String> {
    let decoded = wit_component::decode(component)
        .map_err(|error| format!("the encoded component does not decode: {error:#}"))?;
    let wit_component::DecodedWasm::Component(resolve, id) = decoded else {
        return Err("what was encoded decodes as a WIT package, not a component".to_owned());
    };
    Ok(resolve.worlds[id]
        .imports
        .keys()
        .map(|key| resolve.name_world_key(key))
        .filter(|name| !world.0.contains(name))
        .map(|name| format!("an import of `{name}`"))
        .collect())
}

/// Writes `bytes` to a temporary file beside `path`, then renames it into place.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".partial");
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("`{}` could not be written: {error}", path.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("`{}` could not be put in place: {error}", path.display()))
}

/// What one component is built from.
#[derive(Debug)]
struct Inputs {
    /// Directories whose every file is listed, relative to the repository.
    roots: Vec<String>,
    /// Each file, as its BLAKE3 in hex beside its path, sorted by path.
    files: Vec<(String, String)>,
}

/// The directories a component's list names, and every file in it with its digest.
///
/// Each member directory the component is built from, **whole** — its manifest,
/// its `plugin.toml`, its `src/` and anything else in it — is a root, so a file
/// that later appears anywhere in the member makes the list stale too. Then the
/// world, whole, and the workspace's own manifest and lockfile, which decide
/// the profile and every version.
///
/// **The member, not its `src/`.** With each member's `src/` as the root, and
/// `Cargo.toml`, `build.rs` and `plugin.toml` listed beside it only where each
/// is there at build time, a file cargo acts on without any manifest naming it
/// would arrive after the build unseen. Measured with those roots: a `build.rs`
/// that panics, written into `plugins/reference/` after a clean build, leaves
/// the kernel's suite loading the old component as current while `cargo xtask
/// build --plugins` fails on that very file. Listing more than the build reads — a
/// member's own tests — costs a rebuild the next `check` does anyway; listing
/// less is a suite running against a plugin its sources no longer build.
fn inputs_of(root: &Path, directories: &[String]) -> Result<Inputs, String> {
    let mut roots = vec![WORLD.to_owned()];
    let mut paths = vec!["Cargo.lock".to_owned(), "Cargo.toml".to_owned()];
    for directory in directories {
        roots.push(directory.clone());
    }
    for directory in &roots {
        files_under(root, &root.join(directory), &mut paths)?;
    }
    roots.sort();
    paths.sort();
    paths.dedup();
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(root.join(&path))
            .map_err(|error| format!("`{path}` could not be read: {error}"))?;
        files.push((blake3::hash(&bytes).to_hex().to_string(), path));
    }
    Ok(Inputs { roots, files })
}

/// Every file under `directory`, relative to `root`, with `/` separators.
fn files_under(root: &Path, directory: &Path, into: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("`{}` could not be read: {error}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(root, &path, into)?;
        } else {
            into.push(crate::relative_to(root, &path));
        }
    }
    Ok(())
}

/// The inputs list, as `crates/nc-kernel/src/built.rs` reads it.
fn listing(component: (&str, &str), inputs: &Inputs) -> String {
    let (digest, path) = component;
    std::iter::once(format!("component {digest} {path}\n"))
        .chain(
            inputs
                .roots
                .iter()
                .map(|directory| format!("root {directory}\n")),
        )
        .chain(
            inputs
                .files
                .iter()
                .map(|(digest, path)| format!("file {digest} {path}\n")),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        Inputs, TARGET, WORLD, declared, directory_of, encode, inputs_of, listing, plugin_commands,
    };

    /// A plugin is built, linted with every warning denied and documented
    /// with its private items, each for `wasm32` and each under the verb that
    /// names it, offline when the gate is.
    ///
    /// **These two are the only runs that read the plugin's adapter**, so this
    /// test holds their arguments: weakening the lint's deny or pointing the
    /// documentation at the host fails it, where the step itself would still
    /// print `linted and documented`.
    #[test]
    fn a_plugin_is_built_linted_and_documented_for_wasm32_with_warnings_denied() {
        assert_eq!(TARGET, "wasm32-unknown-unknown");
        for offline in [false, true] {
            let [build, lint, document] = plugin_commands("nc-plugin-x", "target/plugins", offline);
            for (verb, args) in [("build", &build), ("clippy", &lint), ("doc", &document)] {
                assert_eq!(args.first(), Some(&verb), "{args:?}");
                for pair in [
                    ["-p", "nc-plugin-x"],
                    ["--target", TARGET],
                    ["--target-dir", "target/plugins"],
                ] {
                    assert!(
                        args.windows(2).any(|two| two == pair),
                        "`cargo {verb}` is not handed {pair:?}: {args:?}"
                    );
                }
                assert!(
                    args.contains(&"--lib") && args.contains(&"--locked"),
                    "{args:?}"
                );
                assert_eq!(args.contains(&"--offline"), offline, "{args:?}");
            }
            assert!(build.contains(&"--release"), "{build:?}");
            assert!(lint.ends_with(&["--", "-D", "warnings"]), "{lint:?}");
            assert!(
                document.contains(&"--document-private-items") && document.contains(&"--no-deps"),
                "{document:?}"
            );
        }
    }

    /// A stand-in for the WASI interface `wasip2` embeds, and two worlds: the
    /// kernel's own `log`, which a plugin may import, and that one.
    const PROBES: [(&str, &str); 2] = [
        (
            "random.wit",
            "package wasi:random@0.2.4;\n\ninterface random {\n  get-random-u64: func() -> u64;\n}\n",
        ),
        (
            "probes.wit",
            "package nc:probe;\n\nworld probe {\n  import noctua:kernel/log@0.2.0;\n}\n\n\
             world stray {\n  import wasi:random/random@0.2.4;\n}\n",
        ),
    ];

    /// `module`, carrying each of `worlds` the way a crate's bindings embed one:
    /// a `component-type` section apiece.
    fn carrying(mut module: Vec<u8>, worlds: &[&str]) -> Vec<u8> {
        let mut resolve = wit_parser::Resolve::default();
        resolve
            .push_path(root().join(WORLD))
            .expect("the world parses");
        // The last package pushed is the one the worlds are in.
        let mut probes = None;
        for (name, source) in PROBES {
            probes = Some(resolve.push_str(name, source).expect("a probe parses"));
        }
        let probes = probes.expect("there are probes");
        for world in worlds {
            let id = resolve
                .select_world(&[probes], Some(world))
                .expect("the probe's world is there");
            wit_component::embed_component_metadata(
                &mut module,
                &resolve,
                id,
                wit_component::StringEncoding::UTF8,
            )
            .expect("the world embeds in the module");
        }
        module
    }

    /// A core module, byte for byte, importing each `(module, field, parameters,
    /// result)` — `i32` parameters, and a result of the value type given — with
    /// the one page of memory a lowering reads a string from, as `memory`.
    fn a_core_module_importing(imports: &[(&str, &str, u8, Option<u8>)]) -> Vec<u8> {
        let sized = |bytes: &[u8]| {
            let length = u8::try_from(bytes.len())
                .ok()
                .filter(|length| *length < 0x80)
                .expect("every length here fits in one LEB128 byte");
            [&[length][..], bytes].concat()
        };
        let section = |id: u8, content: &[u8]| [&[id][..], &sized(content)].concat();
        let count = u8::try_from(imports.len()).expect("a handful of imports");
        let types: Vec<u8> = imports
            .iter()
            .flat_map(|(_, _, parameters, result)| {
                [
                    &[0x60, *parameters][..],
                    &vec![0x7f; usize::from(*parameters)],
                    &result.map_or(vec![0], |result| vec![1, result]),
                ]
                .concat()
            })
            .collect();
        let entries: Vec<u8> = imports
            .iter()
            .zip(0_u8..)
            .flat_map(|((module, field, _, _), index)| {
                [
                    sized(module.as_bytes()),
                    sized(field.as_bytes()),
                    vec![0, index],
                ]
                .concat()
            })
            .collect();
        [
            b"\0asm\x01\0\0\0".to_vec(),
            section(1, &[&[count][..], &types].concat()),
            section(2, &[&[count][..], &entries].concat()),
            section(5, &[1, 0, 1]),
            section(7, &[&[1][..], &sized(b"memory"), &[2, 0]].concat()),
        ]
        .concat()
    }

    /// A module carrying a second world is refused, and one carrying the
    /// world's own import is not.
    ///
    /// **The encoder merges every world a module carries.** `wasip2`, added to
    /// the reference plugin and called once, embeds a world of its own beside
    /// `nc-guest`'s, and the encoder merges the two without a word: a build that
    /// trusted it would say the component is built, and the kernel would find
    /// `get-random-u64`, from `wasi:random`, among its imports. This is that
    /// shape, built by hand: one module, two embedded worlds, one import from
    /// each.
    #[test]
    fn a_module_carrying_a_second_world_is_refused_and_the_worlds_own_import_is_not() {
        let world = declared(&root().join(WORLD)).expect("the world parses");
        let log = ("noctua:kernel/log@0.2.0", "write", 3, None);
        // `get-random-u64` takes nothing and returns an `i64`.
        let random = ("wasi:random/random@0.2.4", "get-random-u64", 0, Some(0x7e));
        let own = carrying(a_core_module_importing(&[log]), &["probe"]);
        assert!(
            encode(&own, &world).is_ok(),
            "the world's own `log` was refused: {:?}",
            encode(&own, &world).err()
        );
        let second = carrying(a_core_module_importing(&[log, random]), &["probe", "stray"]);
        let refused = encode(&second, &world).expect_err("the second world's import encoded");
        assert!(
            refused.contains("an import of `wasi:random/random@0.2.4")
                && !refused.contains("noctua:kernel/log"),
            "the refusal names the stray import and only that: {refused}"
        );
    }

    #[test]
    fn a_plugin_is_the_package_named_for_its_directory() {
        let member = |path: &str, name: &str| crate::members::Member {
            path: path.to_owned(),
            name: name.to_owned(),
            can_log: false,
            sources: Vec::new(),
            documented: Vec::new(),
        };
        assert_eq!(
            directory_of(&member("plugins/reference", "nc-plugin-reference")),
            Ok("reference")
        );
        let renamed = directory_of(&member("plugins/reference", "reference"))
            .expect_err("a plugin named for something else");
        assert!(renamed.contains("`nc-plugin-<dir>`"), "{renamed}");
        let outside = directory_of(&member("crates/nc-guest", "nc-guest"))
            .expect_err("a crate is not a plugin");
        assert!(outside.contains("is not under `plugins/`"), "{outside}");
    }

    fn root() -> &'static std::path::Path {
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    #[test]
    fn a_listing_is_one_line_per_entry_in_the_shape_the_reader_parses() {
        let text = listing(
            ("c0ffee", "target/plugins/components/a.wasm"),
            &Inputs {
                roots: vec!["plugins/a/src".to_owned()],
                files: vec![("beef".to_owned(), "plugins/a/src/lib.rs".to_owned())],
            },
        );
        assert_eq!(
            text,
            "component c0ffee target/plugins/components/a.wasm\n\
             root plugins/a/src\n\
             file beef plugins/a/src/lib.rs\n"
        );
    }

    #[test]
    fn the_reference_plugin_is_built_from_its_sources_its_manifest_and_the_world() {
        let Inputs { roots, files } = inputs_of(
            root(),
            &["crates/nc-guest".to_owned(), "plugins/reference".to_owned()],
        )
        .expect("the tree is readable");
        assert_eq!(
            roots,
            ["crates/nc-guest", WORLD, "plugins/reference"],
            "the world is a root, and so is every member's whole directory"
        );
        let paths: Vec<&str> = files.iter().map(|(_, path)| path.as_str()).collect();
        for expected in [
            "Cargo.lock",
            "Cargo.toml",
            "crates/nc-guest/Cargo.toml",
            "crates/nc-guest/src/lib.rs",
            "crates/nc-kernel/wit/noctua-kernel.wit",
            "plugins/reference/Cargo.toml",
            "plugins/reference/plugin.toml",
            "plugins/reference/src/adapter.rs",
            "plugins/reference/src/behaviour.rs",
        ] {
            assert!(
                paths.contains(&expected),
                "`{expected}` is missing: {paths:?}"
            );
        }
        assert!(
            files
                .iter()
                .all(|(digest, _)| digest.len() == 64
                    && digest.chars().all(|c| c.is_ascii_hexdigit())),
            "every file carries its BLAKE3: {files:?}"
        );
    }
}
