//! The document the webview is handed, built in Rust before the webview
//! exists (`NOCTUA.md §12.10`): the interface's markup, written as `rsx!`,
//! rendered once into it, with the interface's sheet of layout utilities
//! and the one script that tells the window the document has loaded.
//!
//! **The document links nothing and loads nothing**: the sheet is inlined
//! (`crate::sheet`), and the script is this module's own text, which says one
//! fixed word through the webview's message channel and nothing else. What
//! the window does with that word is [`crate::host`]'s, which reads that word
//! alone.
//!
//! **Every class written here is a utility the sheet holds a rule for**, and
//! one written outside the closed set `cargo xtask check` admits — an
//! arbitrary value, a colour written out, a modifier — fails the gate
//! (`xtask/src/tailwind.rs`), which also rebuilds the sheet from the classes
//! written here and refuses one that differs from `crate::sheet`.

use dioxus::prelude::*;

/// The word the document's script says once it has loaded, through the
/// webview's message channel.
pub const LOADED: &str = "loaded";

/// The script that says [`LOADED`] once the document's markup is all read.
const SAYS_LOADED: &str =
    "window.addEventListener('DOMContentLoaded', () => window.ipc.postMessage('loaded'));";

/// The document, in `locale`, as the webview is handed it.
///
/// **Its root element is written here and its content as `rsx!`**, which
/// renders the elements inside a document and not the document's own.
#[must_use]
pub fn document(locale: nc_catalog::Locale) -> String {
    let rendered = dioxus_ssr::render_element(rsx! { Document {} });
    format!(
        "<!doctype html><html lang=\"{}\">{rendered}</html>",
        locale.tag()
    )
}

/// The document's markup: the interface's root, filling the window, and
/// nothing in it yet.
#[component]
fn Document() -> Element {
    rsx! {
        head {
            meta { charset: "utf-8" }
            style { dangerous_inner_html: crate::sheet::SHEET }
        }
        body { class: "h-screen overflow-hidden",
            main { id: "noctua", class: "flex h-full w-full flex-col" }
            script { dangerous_inner_html: SAYS_LOADED }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LOADED, SAYS_LOADED, document};

    /// The document is one, opening with its type, in the locale it was
    /// asked in, its sheet inlined and its script last in the body.
    #[test]
    fn the_document_is_one_in_its_locale_with_its_sheet_and_its_script() {
        for locale in nc_catalog::Locale::ALL {
            let written = document(*locale);
            assert!(
                written.starts_with(&format!(
                    "<!doctype html><html lang=\"{}\"><head>",
                    locale.tag()
                )),
                "{written}"
            );
            assert_eq!(written.matches("<html").count(), 1, "{written}");
            assert!(written.contains(crate::sheet::SHEET), "{written}");
            let script = written.find(SAYS_LOADED).expect("the script is in it");
            let main = written.find("<main").expect("the root is in it");
            assert!(main < script, "the script follows the root: {written}");
            assert!(written.ends_with("</body></html>"), "{written}");
        }
    }

    /// The script says the one word the window reads, and only once the
    /// document is read.
    #[test]
    fn the_script_says_the_word_the_window_reads() {
        assert!(
            SAYS_LOADED.contains(&format!("postMessage('{LOADED}')")),
            "{SAYS_LOADED}"
        );
        assert!(SAYS_LOADED.contains("'DOMContentLoaded'"), "{SAYS_LOADED}");
    }

    /// The document links nothing and loads nothing: no address is written
    /// in its markup. The sheet's own licence line names its maker's site,
    /// which is text in a comment, so the markup is read without the sheet,
    /// and the sheet is held to loading nothing where it is built.
    #[test]
    fn the_document_links_nothing() {
        let written = document(nc_catalog::Locale::En).replace(crate::sheet::SHEET, "");
        assert!(written.contains("<main"), "the markup is read: {written}");
        for needle in ["href", "src=", "http:", "https:", "url(", "@import"] {
            assert!(
                !written.contains(needle),
                "the document writes `{needle}`: {written}"
            );
        }
    }
}
