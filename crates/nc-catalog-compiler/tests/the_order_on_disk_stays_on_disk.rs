//! The order the filesystem hands back files does not reach the output.
//!
//! **Criterion 4's blind spot.** `emit_all(x) == emit_all(x)` runs both
//! generations in one process against one filesystem, so `read_dir` returns the
//! same order twice and the comparison is blind to a dependency on it. The
//! drift check compares against a disk written by a build on this machine, so it
//! agrees for the same reason. The `deterministic` job in CI builds twice in one
//! checkout on one runner, so it shares the directory order too — and by that
//! file's own opening comment, it has never run.
//!
//! What sits behind all three: `catalog/i18n/` is read with `read_dir`, and
//! locale *order* is load-bearing output. `Locale::ALL`, the enum's variant
//! order, `Locale::index()` and therefore every column of `MESSAGES` in
//! `crates/nc-catalog/src/i18n.rs` come from it, along with four `docs/*.md`
//! pages. One `sort_by` in `locale_files` is what makes that order independent
//! of the filesystem, and the `allow-nondeterminism:` marker beside the
//! `read_dir` call asserts that sort exists — while nothing checked the
//! assertion. Deleting the sort and leaving the marker as committed:
//!
//! ```text
//! cargo xtask build   ->  ok  15 generated file(s), 5 rewritten
//! cargo xtask check   ->  check passed
//!
//! crates/nc-catalog/src/i18n.rs:
//!   /// Every locale, source locale first.
//!   pub const ALL: &'static [Self] = &[Self::PtBR, Self::En];   <- and it is not
//! ```
//!
//! Directory order really does differ between machines — tmpfs returns creation
//! order, ZFS returns hash order — so this is not hypothetical for a second
//! developer or for CI.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
}

/// Copies `catalog/` into `into`, creating the locale files in `first` order.
///
/// Everything else is copied in whatever order the source walk gives; only
/// `catalog/i18n/` is laid down deliberately, because that is the directory
/// whose order reaches the output.
fn lay_out(into: &Path, first: &str) {
    let from = root().join("catalog");
    copy_tree(&from, &into.join("catalog"));
    // **The locale files last, and in the order this layout is named for.** The
    // copy above skips them, so the directory is empty when they are written and
    // a filesystem that returns creation order returns this one.
    let i18n = into.join("catalog/i18n");
    std::fs::create_dir_all(&i18n).expect("writable");
    let mut names: Vec<PathBuf> = std::fs::read_dir(from.join("i18n"))
        .expect("the locales are there")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    names.sort_by_key(|path| {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        (usize::from(!name.starts_with(first)), name)
    });
    for path in names {
        std::fs::copy(&path, i18n.join(path.file_name().expect("a named entry")))
            .expect("writable");
    }
}

/// Everything under `catalog/` except the locale directory.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("writable");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(from)
        .expect("the catalog is there")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().expect("a named entry");
        if name == "i18n" {
            continue;
        }
        let target = to.join(name);
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("writable");
        }
    }
}

/// The names `read_dir` gives back for one directory, in its own order.
fn as_the_filesystem_gives_them(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("readable")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn two_directory_orders_produce_the_same_bytes() {
    // tmpfs returns entries in creation order, which is what makes the two
    // layouts below actually differ. A filesystem that sorts or hashes would
    // make this test vacuous, so the orders are compared before they are relied
    // on and a run that cannot produce two of them fails rather than passes.
    let base = Path::new("/dev/shm").join(format!("nc-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let en_first = base.join("en-first");
    let pt_first = base.join("pt-first");
    lay_out(&en_first, "en");
    lay_out(&pt_first, "pt-BR");

    let one = as_the_filesystem_gives_them(&en_first.join("catalog/i18n"));
    let other = as_the_filesystem_gives_them(&pt_first.join("catalog/i18n"));
    assert_ne!(
        one, other,
        "both layouts came back in the same order, so this test would pass \
         whatever the emitter does with it — it needs a filesystem that returns \
         creation order, which /dev/shm is"
    );

    let first =
        nc_catalog_compiler::compile(&en_first, nc_catalog_compiler::ledger::Bootstrap::Allow)
            .expect("the catalog compiles");
    let second =
        nc_catalog_compiler::compile(&pt_first, nc_catalog_compiler::ledger::Bootstrap::Allow)
            .expect("the catalog compiles");

    let differing: Vec<&str> = first
        .files
        .iter()
        .zip(second.files.iter())
        .filter(|(a, b)| a.contents != b.contents)
        .map(|(a, _)| a.path.as_str())
        .collect();
    assert!(
        differing.is_empty(),
        "two directory orders produced different bytes in: {differing:?}"
    );
    assert_eq!(
        first.files.len(),
        second.files.len(),
        "the two runs did not even emit the same files"
    );
    // The floor: a compile that emitted nothing would satisfy the comparison
    // above by having nothing to compare.
    assert!(
        first.files.len() > 10,
        "{} file(s) emitted",
        first.files.len()
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Every page's sections are in the order the generated crate uses.
///
/// **Four pages walked their registry in file order** while `emit.rs` sorts
/// before it writes the Rust, so `docs/log-events.md` opened at
/// `journal.appended` and `crates/nc-catalog/src/events.rs` opened at
/// `db.migrated` — and `docs.rs`'s own comment, one table over, says "every
/// other table on every page sorts". Measured on the committed tree, no
/// mutation:
///
/// ```text
/// docs/log-events.md   ## `journal.appended`  ## `journal.verified`  ## `journal.broken` …
/// events.rs            name: "db.migrated",   "ids.clock_backwards",  …
/// ```
///
/// Small while the registries are small; it bites on the next insertion, which
/// is one line in the sorted file and a rewrite from that point down in the
/// unsorted one — in a repository whose stated reason for deterministic output
/// is a diff a person can read.
#[test]
fn every_page_lists_its_entries_in_order() {
    let compiled =
        nc_catalog_compiler::compile(&root(), nc_catalog_compiler::ledger::Bootstrap::Refuse)
            .expect("the catalog compiles");
    let mut checked = 0_usize;
    let mut pages = 0_usize;
    for page in &compiled.files {
        if std::path::Path::new(&page.path)
            .extension()
            .is_none_or(|kind| kind != "md")
        {
            continue;
        }
        // Backticked headings only: those are the registry's entries, one
        // section each. `docs/limits.md`'s "Tables", "Key derivation" and
        // "Rate limits" are structure the emitter writes in a deliberate order,
        // and sorting *those* would be a different page, not a better one.
        let headings: Vec<&str> = page
            .contents
            .lines()
            .filter_map(|line| line.strip_prefix("## "))
            .filter(|heading| heading.starts_with('`'))
            .collect();
        let mut sorted = headings.clone();
        sorted.sort_unstable();
        assert_eq!(
            headings, sorted,
            "`{}` lists its sections in an order a reader cannot predict",
            page.path
        );
        checked += headings.len();
        pages += 1;
    }
    // **The pages first.** Most sections live on `docs/log-events.md`, so a
    // floor of ten survives every other page leaving the walk. See
    // [`the_pages_this_catalog_has`].
    assert_eq!(
        pages,
        the_pages_this_catalog_has(),
        "{pages} page(s) were read where this catalog has {}, so whatever is not \
         alphabetical on the pages that left the walk is ordered by nothing",
        the_pages_this_catalog_has()
    );
    // And then the floor on what is in them: a walk that found no headings would
    // satisfy the loop by never entering it, which is this file's own subject
    // one test up. A section is a log event, a command, a notification or an
    // email. A floor of ten is a floor on the *walk*, not a restatement of the
    // registries — which would move with them and prove nothing.
    assert!(checked >= 10, "only {checked} section(s) were read");
}

/// Every value a registry supplies to a table row is escaped for one.
///
/// **It was applied to the translated sentences and to nothing else.**
/// `commands.toml`'s `action` and `resource_selector` and `limits.toml`'s
/// `order_by` were written into their rows raw, and only their non-emptiness is
/// checked — so a value holding a `|` wrote a row with an extra cell in it:
///
/// ```text
/// | Resource selector | `realm | anything` |
/// ```
///
/// and the compiler said nothing. The page's own test caught the wreckage and
/// reported it against the page, which sends a reader to the generated file
/// rather than to the registry line that caused it.
///
/// Driven through `compile` rather than through `cell`, because what was wrong
/// was the call site and not the function.
#[test]
fn a_bar_in_a_registry_value_does_not_end_its_cell() {
    let base = Path::new("/dev/shm").join(format!("nc-cell-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    lay_out(&base, "en");
    // **A hand list of three, and `docs.rs` wrote five values raw.** Two of the
    // other three were reachable: the message-key column of every registry's
    // sentence table, and `limit.surface` in the rate-limit table. Adding either
    // to this list turned it red, which is the definition of an exclusion list
    // that had switched a gate off:
    //
    // ```text
    // `docs/limits.md` has a row of 10 cells under a header of 9
    // `docs/notifications.md` has a row of 6 cells under a header of 5
    // ```
    //
    // **And the reason the remaining three were excused was false of two of
    // them.** It said all three *"pass through `load::identifier()`, which
    // refuses anything outside `[A-Za-z0-9_]`, so a bar cannot reach them"* —
    // and nothing calls `identifier()` on a `limits.tables` key or a
    // `limits.stores` key. `check_tables` never touches the name at all, and
    // `docs.rs` wrote both raw while every neighbouring value on the same two
    // rows went through `cell()`. Adding either to this list turned it red at
    // once:
    //
    // ```text
    // `docs/limits.md` has a row of 8 cells under a header of 7:
    // | `access | records` | `oldest-first` | `recorded` | 500000 | 50000 | — |
    // ```
    //
    // Both are escaped now and both are in the sweep below. What is left
    // excused is a *parameter name*, which really is identifier-checked, and the
    // resource selector, which is held to its grammar — `realm` or
    // `argument:<name>` — by the judge the kernel shares, so the catalog is
    // refused before a page is rendered. That is why this is a list and not a
    // sweep, and each entry says which surface it stands for.
    //
    // **The selector's excuse is asserted rather than assumed**, on this same
    // copy and before anything else is changed in it.
    a_bar_never_reaches_the_selector_column(&base);
    for (file, from, to) in [
        (
            "catalog/commands.toml",
            "action = \"Read\"",
            "action = \"Read | Write\"",
        ),
        (
            "catalog/limits.toml",
            "order_by = \"sequence\"",
            "order_by = \"sequence | id\"",
        ),
        // The rate-limit table's first column.
        (
            "catalog/limits.toml",
            "surface = \"login\"",
            "surface = \"log | in\"",
        ),
        // The first column of the table limits, and of the store limits: the two
        // the comment above excused on a reason that was not true of them.
        (
            "catalog/limits.toml",
            "[tables.access_records]",
            "[tables.\"access | records\"]",
        ),
        (
            "catalog/limits.toml",
            "[stores.central_log]",
            "[stores.\"central | log\"]",
        ),
        // The message-key column, which every registry's sentence table writes.
        // The key has to move in all three files at once, or the cross-registry
        // check refuses the catalog before the page is ever rendered — which is
        // itself the reason a bar cannot get here by accident.
        (
            "catalog/notifications.toml",
            "notification.catalog.mechanism.title",
            "notification.catalog.mech|anism.title",
        ),
        (
            "catalog/i18n/en.toml",
            "notification.catalog.mechanism.title",
            "notification.catalog.mech|anism.title",
        ),
        (
            "catalog/i18n/pt-BR.toml",
            "notification.catalog.mechanism.title",
            "notification.catalog.mech|anism.title",
        ),
    ] {
        let path = base.join(file);
        let text = std::fs::read_to_string(&path).expect("readable");
        assert!(text.contains(from), "`{file}` no longer holds `{from}`");
        std::fs::write(&path, text.replace(from, to)).expect("writable");
    }

    let compiled =
        nc_catalog_compiler::compile(&base, nc_catalog_compiler::ledger::Bootstrap::Allow)
            .expect("a bar in a value is not a reason to refuse the catalog");

    // Every row of one table has the column count its header does. A value that
    // ends its own cell adds one, which is exactly the breakage.
    let mut rows = 0_usize;
    let mut pages = 0_usize;
    for page in &compiled.files {
        if std::path::Path::new(&page.path)
            .extension()
            .is_none_or(|kind| kind != "md")
        {
            continue;
        }
        let mut header: Option<usize> = None;
        for line in page.contents.lines() {
            if !line.starts_with('|') {
                header = None;
                continue;
            }
            let columns = boundaries(line);
            rows += 1;
            match header {
                None => header = Some(columns),
                Some(expected) => assert_eq!(
                    columns, expected,
                    "`{}` has a row of {columns} cells under a header of {expected}, so a \
                     value ended its own cell: {line}",
                    page.path
                ),
            }
        }
        pages += 1;
    }
    // **The pages first.** `docs/errors.md` alone carries more rows than the
    // floor, so `rows > 20` is satisfied with every other page gone. See
    // [`the_pages_this_catalog_has`].
    assert_eq!(
        pages,
        the_pages_this_catalog_has(),
        "{pages} page(s) were read where this catalog has {}, so a column on a page \
         that left the walk could stop being escaped and this would still pass",
        the_pages_this_catalog_has()
    );
    // And then the floor on what is in them: a page set with no tables would
    // satisfy the loop by never entering it.
    assert!(rows > 20, "only {rows} table row(s) were read");

    let _ = std::fs::remove_dir_all(&base);
}

/// A selector holding a bar is refused by name, so its row is never written.
///
/// The half of [`a_bar_in_a_registry_value_does_not_end_its_cell`] that
/// excuses the selector column, run on that test's own copy of the catalog
/// and leaving it as it found it. Its own function because the caller crossed
/// clippy's line limit with it inline.
fn a_bar_never_reaches_the_selector_column(base: &Path) {
    let commands = base.join("catalog/commands.toml");
    let shipped = std::fs::read_to_string(&commands).expect("readable");
    assert!(
        shipped.contains("resource_selector = \"realm\""),
        "`catalog/commands.toml` no longer selects the realm, which this breaks on purpose"
    );
    std::fs::write(
        &commands,
        shipped.replace(
            "resource_selector = \"realm\"",
            "resource_selector = \"realm | anything\"",
        ),
    )
    .expect("writable");
    let refused = nc_catalog_compiler::compile(base, nc_catalog_compiler::ledger::Bootstrap::Allow)
        .expect_err("a selector holding a bar is outside the selector grammar");
    assert!(
        refused
            .iter()
            .any(|problem| problem.check == "selector-malformed"),
        "a bar in a selector reached the page's column unrefused: {refused:?}"
    );
    std::fs::write(&commands, shipped).expect("writable");
}

/// How many pages the emitter writes, counted from the tree.
///
/// **A floor on what is inside the pages is not a floor on the pages.** Both
/// walks below iterate `compiled.files`, filter to Markdown, and then floor on
/// what they found *within* the pages they reached — sections in one, table rows
/// in the other. `docs/errors.md` alone carries more rows than the floor, so
/// `rows > 20` is satisfied with every other page gone from the walk
/// entirely, and the escaper could stop being applied to a whole column of a
/// page nobody read. The section floor has the same shape: most sections live
/// on `docs/log-events.md`, and a floor of ten survives losing every other
/// page.
///
/// So the walks are floored on their *subject* first. The count comes off the
/// committed `docs/` directory, which is a different thing from the compiler's
/// answer — the comparison relates two things, and one of them has to be free
/// of the emitter.
fn the_pages_this_catalog_has() -> usize {
    std::fs::read_dir(root().join("docs"))
        .expect("the generated pages are committed")
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|kind| kind == "md"))
        .count()
}

/// The unescaped `|` characters of a line, which are its cell boundaries.
///
/// `\|` is a bar *inside* a cell — what the emitter's `cell` writes — and does
/// not end one. A counter that did not know the difference would call an
/// escaped value broken and a broken one fine.
fn boundaries(line: &str) -> usize {
    let mut found = 0_usize;
    let mut escaped = false;
    for character in line.chars() {
        match character {
            '\\' => escaped = !escaped,
            '|' if !escaped => found += 1,
            _ => escaped = false,
        }
    }
    found
}
