//! The pages an install is to carry: the generated developer surfaces and
//! the changelog, embedded as written (`NOCTUA.md §12.16`, `§15.3`).
//!
//! **The generated Markdown is the source, and the interface is its reader.**
//! `cargo xtask docs` writes every page under `docs/` from the catalog and the
//! tables beside it, and `cargo xtask release` writes `CHANGELOG.md`; an
//! install has neither directory, so this crate compiles each in as the text
//! the build wrote ([`PAGES`]) and hands it out as the tree the one reader of
//! Markdown answers ([`Page::document`]). Nothing here writes a page or adds
//! to one: a page that repeats generated content by hand would be a second
//! account of it (`§20.5`). **No binary links this crate yet**: the
//! desktop's help and dev pages, which draw these trees, are what will carry
//! it into an install.
//!
//! **Read before anything carries it, three times.** The catalog compiler
//! reads every page it emits through `nc_markdown` and writes nothing while
//! one is outside the subset, so `cargo xtask build` fails naming the page and
//! its line; `cargo xtask release` reads the changelog it computed through
//! the same reader, its commits' own words written as text, and writes
//! nothing while it is outside the subset; and this crate's own suite reads
//! every page it embeds — the changelog, which the compiler does not emit,
//! among them — so a page the interface could not draw fails the gate.
//!
//! **The prose is English, and the columns are both locales'.** A generated
//! page says what the code does in the language every artifact of this tree
//! is written in (`§2.1`), and shows every text a person is told — a
//! command's names, an error's message, a notification — in a column for each
//! locale, side by side; it is the same page in every language the interface
//! runs in. What a person reads in their own language is the registries'
//! texts themselves: a command's entry, an entry's wiki text
//! (`nc_catalog::WIKI`), an error's message.
//!
//! # Where each page is listed
//!
//! | List | Pages |
//! |---|---|
//! | help (Leader `h`) | the commands, the CLI, the MCP tools, the keybindings, the error codes and what is new ([`Page::help`]) |
//! | dev (Leader `d`) | every page generated from a registry — all of them but what is new ([`Page::dev`]) |
//!
//! # The security answer `NOCTUA.md §2.4` asks for
//!
//! A page is bytes compiled into the binary, found by a name of a closed
//! list, so no path a page or a person names reaches a file; and what it is
//! drawn from is `nc_markdown`'s tree, which holds no markup of the page's
//! own. **What it does not answer:** what a page says, which is the catalog's
//! and its compiler's, and whether an install's pages are the newest, which
//! is the release's.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

use nc_markdown::{Block, Document, Inline, Refused};

/// One page an install carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    /// The name it is opened by: its file's, without the extension, and
    /// `whats-new` for the changelog.
    pub name: &'static str,
    /// The page, as the build wrote it.
    pub text: &'static str,
    /// Whether the help pages list it.
    pub help: bool,
    /// Whether the dev pages list it.
    pub dev: bool,
}

/// The name the changelog is opened by.
pub const WHATS_NEW: &str = "whats-new";

/// Every page, in the order of their names.
pub const PAGES: [Page; 14] = [
    generated(
        "capabilities",
        include_str!("../../../docs/capabilities.md"),
        false,
    ),
    generated(
        "categories",
        include_str!("../../../docs/categories.md"),
        false,
    ),
    generated("cli", include_str!("../../../docs/cli.md"), true),
    generated("commands", include_str!("../../../docs/commands.md"), true),
    generated("emails", include_str!("../../../docs/emails.md"), false),
    generated("errors", include_str!("../../../docs/errors.md"), true),
    generated(
        "external-identifiers",
        include_str!("../../../docs/external-identifiers.md"),
        false,
    ),
    generated(
        "interface",
        include_str!("../../../docs/interface.md"),
        false,
    ),
    generated(
        "keybindings",
        include_str!("../../../docs/keybindings.md"),
        true,
    ),
    generated("limits", include_str!("../../../docs/limits.md"), false),
    generated(
        "log-events",
        include_str!("../../../docs/log-events.md"),
        false,
    ),
    generated(
        "mcp-tools",
        include_str!("../../../docs/mcp-tools.md"),
        true,
    ),
    generated(
        "notifications",
        include_str!("../../../docs/notifications.md"),
        false,
    ),
    Page {
        name: WHATS_NEW,
        text: include_str!("../../../CHANGELOG.md"),
        help: true,
        dev: false,
    },
];

/// A page generated from a registry, which the dev pages always list.
const fn generated(name: &'static str, text: &'static str, help: bool) -> Page {
    Page {
        name,
        text,
        help,
        dev: true,
    }
}

/// The page opened by `name`, if an install carries one.
#[must_use]
pub fn page(name: &str) -> Option<&'static Page> {
    PAGES.iter().find(|page| page.name == name)
}

impl Page {
    /// The page as the tree its reader draws.
    ///
    /// # Errors
    ///
    /// [`Refused`] for a page outside the subset, which the build that
    /// embedded it refuses first: a page that fails here is a build made past
    /// its own gate.
    pub fn document(&self) -> Result<Document, Refused> {
        nc_markdown::parse(self.text)
    }

    /// The page's title: the text of its first heading, as plain text.
    ///
    /// # Errors
    ///
    /// What [`Page::document`] refuses.
    pub fn title(&self) -> Result<String, Refused> {
        let document = self.document()?;
        Ok(document
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Heading { text, .. } => Some(plain(text)),
                _ => None,
            })
            .unwrap_or_else(|| self.name.to_owned()))
    }
}

/// What `inlines` say with every mark taken away.
fn plain(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(text) | Inline::Code(text) => text.clone(),
            Inline::Emphasis(held) | Inline::Strong(held) | Inline::Link { text: held, .. } => {
                plain(held)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use nc_markdown::Block;

    use super::{PAGES, Page, WHATS_NEW, page, plain};

    /// The repository's root, from this crate's directory.
    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    /// Every page is of the subset its reader draws, opens with one heading
    /// of the first level and holds no other of that level.
    #[test]
    fn every_page_is_read_by_the_one_reader_and_opens_with_its_title() {
        for page in &PAGES {
            let document = page
                .document()
                .unwrap_or_else(|refused| panic!("{}: {refused}", page.name));
            let firsts = document
                .blocks
                .iter()
                .filter(|block| matches!(block, Block::Heading { level: 1, .. }))
                .count();
            assert_eq!(firsts, 1, "{}", page.name);
            assert!(
                matches!(
                    document.blocks.first(),
                    Some(Block::Heading { level: 1, .. })
                ),
                "{}",
                page.name
            );
            let title = page.title().expect("read above");
            assert!(!title.is_empty() && title != page.name, "{}", page.name);
        }
        assert_eq!(
            page("errors").map(Page::title),
            Some(Ok("Error codes".to_owned()))
        );
        assert_eq!(
            page(WHATS_NEW).map(Page::title),
            Some(Ok("Changelog".to_owned()))
        );
    }

    /// A page outside the subset is refused when it is asked for, and a page
    /// with no heading is titled by its name.
    #[test]
    fn a_page_outside_the_subset_is_refused_and_one_untitled_takes_its_name() {
        let raw = Page {
            name: "raw",
            text: "# Title\n\n<script>alert(1)</script>\n",
            help: false,
            dev: false,
        };
        let refused = raw.document().expect_err("raw HTML");
        assert_eq!((refused.line, refused.column), (3, 1));
        assert_eq!(raw.title(), Err(refused));
        let untitled = Page {
            name: "untitled",
            text: "words\n",
            ..raw
        };
        assert_eq!(untitled.title(), Ok("untitled".to_owned()));
        let marked = nc_markdown::parse("# The `code` and **strong *both* [link](README.md)**")
            .expect("of the subset");
        let Some(Block::Heading { text, .. }) = marked.blocks.first() else {
            panic!("{marked:?}");
        };
        assert_eq!(plain(text), "The code and strong both link");
    }

    /// The pages are every generated page under `docs/` and the changelog,
    /// each embedded as the file holds it, both ways.
    #[test]
    fn the_pages_are_the_generated_pages_on_disk_and_the_changelog() {
        let mut on_disk: BTreeSet<String> = std::fs::read_dir(root().join("docs"))
            .expect("docs/ is there")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
            .filter_map(|path| Some(path.file_stem()?.to_str()?.to_owned()))
            // The one hand-written file there: this repository's manual.
            .filter(|name| name != "README")
            .collect();
        on_disk.insert(WHATS_NEW.to_owned());
        let embedded: BTreeSet<String> = PAGES.iter().map(|page| page.name.to_owned()).collect();
        assert_eq!(embedded, on_disk);
        assert_eq!(embedded.len(), PAGES.len(), "a name twice");
        assert!(PAGES.is_sorted_by(|a, b| a.name < b.name));
        for page in &PAGES {
            let file = if page.name == WHATS_NEW {
                root().join("CHANGELOG.md")
            } else {
                root().join("docs").join(format!("{}.md", page.name))
            };
            let written = std::fs::read_to_string(&file).expect("readable");
            assert_eq!(page.text, written, "{}", page.name);
            assert_eq!(super::page(page.name), Some(page));
        }
        assert_eq!(page("README"), None);
        assert_eq!(page(""), None);
        assert_eq!(page("error"), None);
        assert_eq!(page("../Cargo"), None);
    }

    /// The help pages are the six §12.16 names that are pages, and the dev
    /// pages every generated one.
    #[test]
    fn each_list_holds_the_pages_its_section_names() {
        let help: Vec<&str> = PAGES
            .iter()
            .filter(|page| page.help)
            .map(|page| page.name)
            .collect();
        assert_eq!(
            help,
            [
                "cli",
                "commands",
                "errors",
                "keybindings",
                "mcp-tools",
                WHATS_NEW
            ]
        );
        let not_dev: Vec<&str> = PAGES
            .iter()
            .filter(|page| !page.dev)
            .map(|page| page.name)
            .collect();
        assert_eq!(not_dev, [WHATS_NEW]);
    }
}
