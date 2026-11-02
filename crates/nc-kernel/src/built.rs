//! The components `cargo xtask build --plugins` wrote, and the proof that each
//! is the one its sources build today. **Compiled for tests only.**
//!
//! **A component is never built here, and a missing or stale one is never
//! skipped.** Building one is a `wasm32` build and an encoding step, which is
//! `xtask`'s — `xtask/src/plugins.rs` writes the component and, beside it, the
//! list of every file it was built from with its BLAKE3. What this module does
//! is read that list back: each file must still hash the same, no file may have
//! appeared under a directory the list names, and the component must be the
//! bytes the list was written for. Anything else panics with the command that
//! fixes it, because a suite that skipped a stale component would pass against
//! a plugin nobody wrote, and a skipped test reads as a passed one.
//!
//! The list's format is written by `xtask/src/plugins.rs` and read here, one
//! line per entry:
//!
//! ```text
//! component <blake3> <path>    the component, relative to the repository
//! root <path>                  a directory whose every file is listed below
//! file <blake3> <path>         one file the component was built from
//! ```
//!
//! and a line of any other shape is refused rather than skipped.
//!
//! **Each item carries the condition, and the module declaration none.** A
//! condition on `mod built;` would read, to the gate that refuses a hidden
//! test, as every test in this file hidden behind it — so the file is always a
//! module, and what is compiled out without `test-support` is its items.

#[cfg(any(test, feature = "test-support"))]
use std::path::{Path, PathBuf};

/// What every refusal here tells a reader to run.
#[cfg(any(test, feature = "test-support"))]
const REBUILD: &str =
    "run `cargo xtask build --plugins` (cargo xtask check does it before the tests)";

/// The component a list describes, if the tree still matches it.
///
/// `digest_of` answers a path's BLAKE3, or nothing for a file that is not
/// there, and `files_under` answers every file under a directory. They are
/// handed in rather than read here so that every refusal can be driven without
/// touching the tree — which is the only way to show a staleness check refusing
/// anything.
#[cfg(any(test, feature = "test-support"))]
fn current(
    listing: &str,
    digest_of: &dyn Fn(&str) -> Option<String>,
    files_under: &dyn Fn(&str) -> Vec<String>,
) -> Result<String, String> {
    let mut component: Option<(&str, &str)> = None;
    let mut roots: Vec<&str> = Vec::new();
    let mut listed: Vec<&str> = Vec::new();
    for line in listing.lines() {
        match line.splitn(3, ' ').collect::<Vec<_>>().as_slice() {
            ["component", hash, path] if component.is_none() => component = Some((hash, path)),
            ["root", path] => roots.push(path),
            ["file", hash, path] => {
                if digest_of(path).as_deref() != Some(*hash) {
                    return Err(format!(
                        "`{path}` changed or went after the component was built from it"
                    ));
                }
                listed.push(path);
            }
            _ => return Err(format!("the list holds a line this cannot read: `{line}`")),
        }
    }
    let (hash, path) = component.ok_or_else(|| "the list names no component".to_owned())?;
    if roots.is_empty() || listed.is_empty() {
        return Err(format!(
            "the list names {} director(ies) and {} file(s), and a component is built \
             from both",
            roots.len(),
            listed.len()
        ));
    }
    for root in &roots {
        if let Some(arrived) = files_under(root)
            .into_iter()
            .find(|file| !listed.contains(&file.as_str()))
        {
            return Err(format!(
                "`{arrived}` is in `{root}` and was not there when the component was built"
            ));
        }
    }
    if digest_of(path).as_deref() != Some(hash) {
        return Err(format!(
            "`{path}` is not the component the list was written for"
        ));
    }
    Ok(path.to_owned())
}

/// The repository this crate sits in.
#[cfg(any(test, feature = "test-support"))]
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `directory`, relative to `repository`, with `/` separators.
#[cfg(any(test, feature = "test-support"))]
fn files_under(repository: &Path, directory: &Path, into: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(repository, &path, into);
        } else if let Ok(relative) = path.strip_prefix(repository) {
            into.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The component built from `plugins/<name>`, once it is shown to be current.
///
/// # Panics
///
/// When the component's list of inputs is missing, or when [`current`] finds
/// the tree no longer matches it.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn component(name: &str) -> Vec<u8> {
    let repository = repository();
    let listing_path = repository.join(format!("target/plugins/components/{name}.inputs"));
    let listing = std::fs::read_to_string(&listing_path).unwrap_or_else(|error| {
        panic!(
            "no component of `plugins/{name}` has been built — `{}`: {error}. {REBUILD}",
            listing_path.display()
        )
    });
    let digest_of = |path: &str| {
        std::fs::read(repository.join(path))
            .ok()
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
    };
    let under = |directory: &str| {
        let mut found = Vec::new();
        files_under(&repository, &repository.join(directory), &mut found);
        found
    };
    let path = current(&listing, &digest_of, &under).unwrap_or_else(|why| {
        panic!("the component of `plugins/{name}` is stale: {why}. {REBUILD}")
    });
    std::fs::read(repository.join(&path))
        .unwrap_or_else(|error| panic!("`{path}` cannot be read ({error}): {REBUILD}"))
}

#[cfg(test)]
mod tests {
    use super::current;

    /// A tree as the staleness check sees it: each path's digest and each directory's files.
    #[expect(
        clippy::type_complexity,
        reason = "two closures standing in for a filesystem are the whole of what this \
                  returns, and a type alias for them would be a name read once"
    )]
    fn tree(
        files: &[(&str, &str)],
    ) -> (
        Box<dyn Fn(&str) -> Option<String>>,
        Box<dyn Fn(&str) -> Vec<String>>,
    ) {
        let digests: Vec<(String, String)> = files
            .iter()
            .map(|(path, digest)| ((*path).to_owned(), (*digest).to_owned()))
            .collect();
        let paths: Vec<String> = digests.iter().map(|(path, _)| path.clone()).collect();
        (
            Box::new(move |path: &str| {
                digests
                    .iter()
                    .find(|(listed, _)| listed == path)
                    .map(|(_, digest)| digest.clone())
            }),
            Box::new(move |directory: &str| {
                paths
                    .iter()
                    .filter(|path| path.starts_with(&format!("{directory}/")))
                    .cloned()
                    .collect()
            }),
        )
    }

    /// What a build of a plugin in `a` writes, over the tree [`tree`] is handed.
    const LISTING: &str = "component c0 target/plugins/components/a.wasm\n\
                           root plugins/a/src\n\
                           file f1 plugins/a/src/lib.rs\n";

    #[test]
    fn a_component_whose_inputs_are_unchanged_is_current() {
        let (digest_of, under) = tree(&[
            ("plugins/a/src/lib.rs", "f1"),
            ("target/plugins/components/a.wasm", "c0"),
        ]);
        assert_eq!(
            current(LISTING, &digest_of, &under),
            Ok("target/plugins/components/a.wasm".to_owned())
        );
    }

    #[test]
    fn a_changed_or_missing_input_makes_the_component_stale() {
        for files in [
            &[
                ("plugins/a/src/lib.rs", "f2"),
                ("target/plugins/components/a.wasm", "c0"),
            ][..],
            &[("target/plugins/components/a.wasm", "c0")][..],
        ] {
            let (digest_of, under) = tree(files);
            let refused = current(LISTING, &digest_of, &under).expect_err("stale");
            assert!(
                refused.contains("plugins/a/src/lib.rs") && refused.contains("changed or went"),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_file_added_under_a_listed_directory_makes_the_component_stale() {
        let (digest_of, under) = tree(&[
            ("plugins/a/src/lib.rs", "f1"),
            ("plugins/a/src/new.rs", "f3"),
            ("target/plugins/components/a.wasm", "c0"),
        ]);
        let refused = current(LISTING, &digest_of, &under).expect_err("stale");
        assert!(
            refused.contains("plugins/a/src/new.rs") && refused.contains("was not there"),
            "{refused}"
        );
    }

    #[test]
    fn a_build_script_that_appears_in_the_member_after_the_build_makes_it_stale() {
        // **The member's directory is the root, not its `src/`.** Cargo runs a
        // `build.rs` that no manifest names, so one written after the build is
        // a plugin the component was not built from — measured, a panicking one
        // left the kernel's suite green while the plugin build failed on it,
        // when the roots were each member's `src/`. `xtask/src/plugins.rs`
        // writes the member whole now, and this is that list read back.
        let listing = "component c0 target/plugins/components/a.wasm\n\
                       root plugins/a\n\
                       file m1 plugins/a/Cargo.toml\n\
                       file f1 plugins/a/src/lib.rs\n";
        let (digest_of, under) = tree(&[
            ("plugins/a/Cargo.toml", "m1"),
            ("plugins/a/build.rs", "b1"),
            ("plugins/a/src/lib.rs", "f1"),
            ("target/plugins/components/a.wasm", "c0"),
        ]);
        let refused = current(listing, &digest_of, &under).expect_err("stale");
        assert!(
            refused.contains("plugins/a/build.rs") && refused.contains("was not there"),
            "{refused}"
        );
    }

    #[test]
    fn a_component_that_is_not_the_one_listed_is_stale() {
        let (digest_of, under) = tree(&[
            ("plugins/a/src/lib.rs", "f1"),
            ("target/plugins/components/a.wasm", "c9"),
        ]);
        let refused = current(LISTING, &digest_of, &under).expect_err("stale");
        assert!(
            refused.contains("is not the component the list was written for"),
            "{refused}"
        );
    }

    #[test]
    fn a_list_that_cannot_describe_a_build_is_refused() {
        let (digest_of, under) = tree(&[
            ("plugins/a/src/lib.rs", "f1"),
            ("target/plugins/components/a.wasm", "c0"),
        ]);
        for (listing, said) in [
            (
                "root plugins/a/src\nfile f1 plugins/a/src/lib.rs\n",
                "names no component",
            ),
            (
                "component c0 target/plugins/components/a.wasm\nfile f1 plugins/a/src/lib.rs\n",
                "0 director(ies) and 1 file(s)",
            ),
            (
                "component c0 target/plugins/components/a.wasm\nroot plugins/a/src\n",
                "1 director(ies) and 0 file(s)",
            ),
            (
                "component c0 target/plugins/components/a.wasm\nroot plugins/a/src\nfile f1\n",
                "a line this cannot read",
            ),
            (
                "component c0 target/plugins/components/a.wasm\n\
                 component c0 target/plugins/components/a.wasm\n",
                "a line this cannot read",
            ),
        ] {
            let refused = current(listing, &digest_of, &under).expect_err("refused");
            assert!(refused.contains(said), "{listing:?}: {refused}");
        }
    }
}
