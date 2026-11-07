//! Regenerates the self-documentation surfaces (`NOCTUA.md §15.3`).
//!
//! Generated, committed and drift-checked Markdown under `docs/`: every error
//! code, notification, email, limit, command, log event and capability, in both
//! locales, from the same source the code consumes — so the documentation cannot drift
//! from the behaviour. `§15.3` asks for documentation that is "as automated as
//! possible", and the catalog is the instrument.
//!
//! **They are emitted through the same list as the generated Rust**, which is
//! what makes them drift-checked, stale-checked and deterministic without any of
//! it being written twice. This verb exists only to write that subset: somebody
//! editing a sentence should be able to see the page without rebuilding the
//! workspace, and a verb that wrote everything would make `build` and `docs` two
//! names for one thing.
//!
//! The **in-app** developer pages `§15.3` also requires are a different artifact
//! with a different host: they are the dev pages of the desktop interface
//! (`NOCTUA.md §12.16`), which arrives in Stage 16. This surface is what a developer can read
//! before then, and it is the source those pages will render.

use std::path::Path;

use nc_catalog_compiler::DOCUMENTATION;

use crate::ui;

/// The pages this verb writes, or why there are none.
///
/// **Named so it can be driven, because the call site cannot be.** `run`
/// compiles the whole catalog first, so a test through `run` reaches neither of
/// this verb's two decisions; `a_run_that_would_write_no_page_is_a_refusal`
/// hands this one no page, and `if pages.is_empty()` replaced by `if false`
/// fails it. The refusal exists because a verb that wrote nothing would
/// otherwise report success for it, and a verb that reports success for
/// nothing is `§19.3`'s first shape.
fn the_pages_of(
    files: &[nc_catalog_compiler::EmittedFile],
) -> Result<Vec<&nc_catalog_compiler::EmittedFile>, String> {
    let prefix = format!("{DOCUMENTATION}/");
    let pages: Vec<_> = files
        .iter()
        .filter(|file| file.path.starts_with(&prefix))
        .collect();
    if pages.is_empty() {
        return Err(format!(
            "the catalog produced no page under {DOCUMENTATION}/, so this verb wrote \
             nothing and would have reported success for it"
        ));
    }
    Ok(pages)
}

/// Whether the page on disk is already this page.
///
/// An identical rewrite bumps the modification time, which is enough to wake a
/// file watcher and start a loop that never settles.
///
/// **The other decision a test through `run` cannot reach.** Replaced by
/// `true` — the verb writing nothing, ever — it fails
/// `a_page_is_rewritten_when_it_differs_and_not_otherwise`, and driven for real
/// against a stale page, `cargo xtask docs` reports `0 rewritten`, exits 0, and
/// leaves the stale line on disk.
fn already_there(path: &Path, contents: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|on_disk| on_disk == contents)
}

/// Which of `pages` a run would write, by workspace-relative path.
///
/// **The polarity of the skip, which a test of the predicate cannot reach.**
/// Asked inline in the loop as `if already_there(…) { continue; }`, the skip is
/// a condition whose inversion makes `cargo xtask docs` write **nothing at
/// all** — every page it is about to write skipped, every page it has nothing
/// to do with attempted — while a test of `already_there` passes either way.
/// `already_there` itself has a unit test and one mutation operator; its *use*
/// is a second decision, and needs a reader of its own.
///
/// Lifted so a test can hand it a directory holding one of two pages and read
/// back which one is left to write.
fn what_a_run_would_write<'a>(
    root: &Path,
    pages: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    pages
        .into_iter()
        .filter(|(path, contents)| !already_there(&root.join(path), contents))
        .map(|(path, _)| (*path).to_owned())
        .collect()
}

/// Writes the documentation surfaces, and nothing else.
///
/// # Errors
///
/// Returns why the catalog could not be compiled, or a page could not be
/// written.
pub(crate) fn run(root: &Path) -> Result<(), String> {
    ui::heading("docs");
    // Never `Allow`: reserving an error code is `build`'s to do, and a verb that
    // could do it by accident is a bypass waiting to be found.
    let compiled = crate::build::artifacts(root, nc_catalog_compiler::ledger::Bootstrap::Refuse)?;

    let pages = the_pages_of(&compiled.files)?;

    let mut written = 0_usize;
    for page in &pages {
        let path = root.join(&page.path);
        // See [`what_a_run_would_write`]: this decision is the one that says
        // whether the verb writes anything, and inverted it writes nothing; what
        // decides it is lifted into that function so that a test can reach it.
        if what_a_run_would_write(root, [(page.path.as_str(), page.contents.as_str())]).is_empty() {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        // The same sentence `build` writes, for the same reason: a run that
        // stopped half-way has left `docs/` part old and part new, and the
        // person reading this is deciding whether it is safe to stop.
        std::fs::write(&path, &page.contents).map_err(|error| {
            let part_way = if written > 0 {
                format!(
                    " — {written} page(s) were rewritten before it, so `docs/` is \
                     part-way through"
                )
            } else {
                " — nothing was rewritten before it, so `docs/` is as it was".to_owned()
            };
            format!(
                "could not write {}: {error}{part_way}. Run `cargo xtask docs` again once \
                 that path is writable",
                page.path
            )
        })?;
        written += 1;
    }

    // **And whether a page now documents a code the ledger has never heard of.**
    // `docs` compiles the catalog and writes the pages; it does not reserve, so
    // a new code goes straight onto `docs/errors.md`, exit 0, `1 rewritten`,
    // while `catalog/errors.reserved.toml` holds nothing about it — and without
    // this warning nothing in the run says so. `check` catches the
    // drift afterwards, which makes this a confusing intermediate state rather
    // than a lasting one; saying it here costs one line and removes the
    // confusion.
    let unreserved = codes_no_ledger_holds(root, &compiled);
    ui::ok(&format!("{} page(s), {written} rewritten", pages.len()));
    if !unreserved.is_empty() {
        ui::warn(&format!(
            "{} code(s) on these pages are not in catalog/errors.reserved.toml: {}",
            unreserved.len(),
            unreserved.join(", ")
        ));
        ui::detail(
            "`docs` writes pages and reserves nothing — `cargo xtask build` is what makes \
             a code permanent, and until it runs these pages document a number that is \
             not yet anybody's",
        );
    }
    for page in &pages {
        ui::detail(&page.path);
    }
    Ok(())
}

/// Every error code the catalog declares that the ledger on disk does not hold.
///
/// Read from the file rather than asked of the compiler, because what matters
/// here is what a reader of this repository would find: the pages are about to
/// say a code exists, and the ledger is where "exists" is recorded.
fn codes_no_ledger_holds(root: &Path, compiled: &nc_catalog_compiler::Compiled) -> Vec<String> {
    let ledger = std::fs::read_to_string(root.join(&compiled.ledger.path)).unwrap_or_default();
    compiled
        .catalog
        .errors
        .iter()
        .map(|entry| entry.code.clone())
        .filter(|code| !ledger.contains(&format!("\"{code}\"")))
        .collect()
}

#[cfg(test)]
mod tests {
    /// A run writes the pages that are not already there, and only those.
    ///
    /// **The decision, not the predicate.** `already_there` has a unit test and
    /// a mutation operator; the skip it guards is a decision of its own, and
    /// inverting that one condition, which makes `cargo xtask docs` write
    /// nothing at all, fails this test.
    /// Both directions, because inverted it is wrong both ways: it skips what
    /// it should write and attempts what it should skip.
    #[test]
    fn a_run_writes_the_pages_that_are_not_already_there() {
        let held = crate::check::LeavesNothingBehind::new("docs-would-write");
        let root = held.root.clone();
        std::fs::create_dir_all(root.join("docs")).expect("writable");
        std::fs::write(root.join("docs/same.md"), "identical\n").expect("writable");
        std::fs::write(root.join("docs/stale.md"), "what was there before\n").expect("writable");
        assert_eq!(
            super::what_a_run_would_write(
                &root,
                [
                    ("docs/same.md", "identical\n"),
                    ("docs/stale.md", "what the compiler says now\n"),
                    ("docs/absent.md", "never written\n"),
                ]
            ),
            ["docs/stale.md", "docs/absent.md"],
            "a run would write the wrong set: an identical page rewritten bumps its \
             modification time and wakes a watcher, and a stale or absent one left \
             unwritten is documentation that does not say what the catalog says"
        );
        drop(held);
    }

    use nc_catalog_compiler::EmittedFile;

    /// This verb refuses to report success for writing nothing.
    ///
    /// **Neither of its two decisions is reached through `run`**, because `run`
    /// compiles the whole catalog before it reaches them. With
    /// `if pages.is_empty()` replaced by `if false`, this test fails, and the
    /// verb prints `ok 0 page(s), 0 rewritten` and exits 0 for a catalog that
    /// produced no page at all — `§19.3`'s first shape, in one of the five verbs
    /// `NOCTUA.md §10.2` fixes.
    #[test]
    fn a_run_that_would_write_no_page_is_a_refusal() {
        let none: Vec<EmittedFile> = Vec::new();
        let refused = super::the_pages_of(&none).expect_err("no page is not a success");
        assert!(
            refused.contains(super::DOCUMENTATION),
            "the refusal does not say where it looked: {refused}"
        );
        // And a catalog that produced files, none of them a page: the filter is
        // what decides, not the emptiness of the whole list.
        let elsewhere = vec![EmittedFile::new(
            "crates/nc-catalog/src/errors.rs",
            "// code\n",
        )];
        super::the_pages_of(&elsewhere).expect_err("a generated crate is not a page");
        // A page, and only the pages.
        let mixed = vec![
            EmittedFile::new(format!("{}/errors.md", super::DOCUMENTATION), "# Errors\n"),
            EmittedFile::new("crates/nc-catalog/src/errors.rs", "// code\n"),
        ];
        let pages = super::the_pages_of(&mixed).expect("one page is enough");
        assert_eq!(
            pages.len(),
            1,
            "the filter took something that is not a page"
        );
        assert!(pages[0].path.ends_with("errors.md"));
    }

    /// A page already on disk is left alone, and a stale one is not.
    ///
    /// **The other decision a test through `run` cannot reach.** Replaced by
    /// `true` — the verb writing nothing, ever — `already_there` fails this
    /// test, and driven for real against a page with a line appended,
    /// `cargo xtask docs` reports `0 rewritten`, exits 0, and leaves the stale
    /// line there.
    /// An identical rewrite is what this rule exists to avoid, because it bumps
    /// the modification time and wakes a file watcher that never settles.
    #[test]
    fn a_page_is_rewritten_when_it_differs_and_not_otherwise() {
        let held = crate::check::LeavesNothingBehind::new("scratch");
        let scratch = held.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        let page = scratch.join("errors.md");
        // Missing: there is nothing there to be already right.
        assert!(!super::already_there(&page, "# Errors\n"));
        std::fs::write(&page, "# Errors\n").expect("writable");
        assert!(super::already_there(&page, "# Errors\n"));
        // Stale by one line, which is the shape a page takes when the catalog
        // moves under it.
        std::fs::write(&page, "# Errors\n\n<!-- stale -->\n").expect("writable");
        assert!(
            !super::already_there(&page, "# Errors\n"),
            "a stale page reads as already written, so `docs` reports pages it did not write"
        );
        drop(held);
    }
}
