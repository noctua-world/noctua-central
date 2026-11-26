//! The document the webview is handed, built in Rust before the webview
//! exists (`NOCTUA.md §12.10`, §9.6): the interface's markup, written as
//! `rsx!`, rendered once into it, with its theme already stamped on its root
//! and every stylesheet it is painted with linked in its head.
//!
//! **The theme is the document's before anything paints** ([`crate::theme`]):
//! its root carries the mode and the palette, and its head links the design
//! system's sheets in the design system's order, the palette's own after
//! them, and the interface's sheet of layout utilities last — each a
//! `<link>` in the head, which a page does not paint before it has loaded.
//! No script stamps or swaps anything, so nothing is painted that a script
//! would replace (§9.6, lessons 1 and 4).
//!
//! **What the first run shows is the footer alone** (§12.2, §12.3): the
//! workspace area above it holds nothing, and the footer is the design
//! system's `Footer`, its three lines — the modeline, the info line and the
//! prompt line — and a corner at each end. What each line says is drawn by a
//! later step of this stage; the window shows this document as it is built.
//!
//! **Everything it links is the custom protocol's** ([`crate::assets`]), by a
//! path relative to the document, so no address in it names a host or a
//! scheme, and every path is one the protocol answers. Its one script says,
//! through the webview's message channel, the page's scale — how many device
//! pixels one CSS pixel is — and then a fixed word once the document and
//! every sheet it links have loaded, and the scale again whenever the page
//! is resized, and nothing else; what the window does with each is
//! [`crate::host`]'s.
//!
//! **Every class written here is a utility the sheet holds a rule for**, and
//! one written outside the closed set `cargo xtask check` admits — an
//! arbitrary value, a colour written out, a modifier — fails the gate
//! (`xtask/src/tailwind.rs`), which also rebuilds the sheet from the classes
//! written here and refuses one that differs from `crate::sheet`.

use dioxus::prelude::*;
use noctua_design::{
    Footer, FooterCorner, FooterCornerSide, FooterLine, FooterLineKind, FooterVisibility,
};

use crate::theme::Theme;

/// The word the document's script says once the document and every sheet it
/// links have loaded, through the webview's message channel.
pub const LOADED: &str = "loaded";

/// The word the document's script says the page's scale with: how many
/// device pixels one CSS pixel is, in thousandths, written after it.
pub const SCALE: &str = "scale";

/// The script that says the page's scale ([`SCALE`]) and then [`LOADED`]
/// once the document has loaded — its markup, and every stylesheet its head
/// links — and the scale again whenever the page is resized, so the window
/// can keep the least size in CSS pixels whatever the system scales a page
/// by.
const SAYS_LOADED: &str = "(() => {\
    const scale = () => window.ipc.postMessage('scale ' + Math.round(window.devicePixelRatio * 1000));\
    window.addEventListener('load', () => { scale(); window.ipc.postMessage('loaded'); });\
    window.addEventListener('resize', scale);\
})();";

/// The name the footer is given for a screen reader: the product's, which
/// no locale translates, until the interface's own texts arrive in both
/// locales.
const FOOTER_NAME: &str = crate::host::TITLE;

/// The document, in `locale` and `theme`, as the webview is handed it.
///
/// **Its root element is written here and its content as `rsx!`**, which
/// renders the elements inside a document and not the document's own; the
/// root's attributes are the theme's, each a word of the design system's
/// closed set, so none needs escaping.
#[must_use]
pub fn document(locale: nc_catalog::Locale, theme: Theme) -> String {
    let rendered = dioxus_ssr::render_element(rsx! {
        Document { sheets: theme.sheets() }
    });
    let [(mode, mode_value), (palette, palette_value)] = theme.attributes();
    format!(
        "<!doctype html><html lang=\"{}\" {mode}=\"{mode_value}\" \
         {palette}=\"{palette_value}\">{rendered}</html>",
        locale.tag()
    )
}

/// The document's markup: its head, linking `sheets` of the design system
/// and then the interface's sheet, and the interface's root, filling the
/// window — the workspace area, which holds nothing yet, and the footer.
#[component]
fn Document(sheets: Vec<&'static str>) -> Element {
    let linked = sheets
        .iter()
        .map(|sheet| format!("{}{sheet}", crate::assets::DIST));
    rsx! {
        head {
            meta { charset: "utf-8" }
            for href in linked {
                link { rel: "stylesheet", href }
            }
            link { rel: "stylesheet", href: crate::assets::SHEET }
        }
        body { class: "h-screen overflow-hidden",
            main { id: "noctua", class: "flex h-full w-full flex-col",
                div { id: "noctua-workspace", class: "flex-1" }
                Shell {}
            }
            script { dangerous_inner_html: SAYS_LOADED }
        }
    }
}

/// The footer, edge to edge at the bottom of the window: a corner at each
/// end and its three lines between them, top to bottom (`NOCTUA.md §12.3`).
#[component]
fn Shell() -> Element {
    rsx! {
        Footer { label: FOOTER_NAME, visibility: FooterVisibility::Always,
            FooterCorner { side: FooterCornerSide::Start }
            FooterLine { kind: FooterLineKind::Modeline }
            FooterLine { kind: FooterLineKind::Info }
            FooterLine { kind: FooterLineKind::Prompt }
            FooterCorner { side: FooterCornerSide::End }
        }
    }
}

#[cfg(test)]
mod tests {
    use noctua_design::model::{Mode, Palette};

    use super::{LOADED, SAYS_LOADED, SCALE, document};
    use crate::assets::{Answered, answer};
    use crate::theme::Theme;

    /// A theme of each mode.
    fn themes() -> [Theme; 2] {
        [
            Theme::chosen(Mode::Light, Palette::DEFAULT, true),
            Theme::chosen(Mode::Dark, Palette::JadeVivid, false),
        ]
    }

    /// The value of every `name="…"` the document writes, in order.
    fn values<'a>(written: &'a str, name: &str) -> Vec<&'a str> {
        written
            .split(&format!(" {name}=\""))
            .skip(1)
            .filter_map(|rest| rest.split_once('"').map(|(value, _)| value))
            .collect()
    }

    /// The document is one, opening with its type, in the locale it was
    /// asked in, its root carrying the theme's mode and palette, and its
    /// script last in the body.
    #[test]
    fn the_document_is_one_in_its_locale_and_its_theme() {
        for locale in nc_catalog::Locale::ALL {
            for theme in themes() {
                let written = document(*locale, theme);
                let [(_, mode), (_, palette)] = theme.attributes();
                assert!(
                    written.starts_with(&format!(
                        "<!doctype html><html lang=\"{}\" data-theme=\"{mode}\" \
                         data-palette=\"{palette}\"><head>",
                        locale.tag()
                    )),
                    "{written}"
                );
                assert_eq!(written.matches("<html").count(), 1, "{written}");
                let script = written.find(SAYS_LOADED).expect("the script is in it");
                let main = written.find("<main").expect("the root is in it");
                assert!(main < script, "the script follows the root: {written}");
                assert!(written.ends_with("</body></html>"), "{written}");
            }
        }
    }

    /// The head links the theme's sheets in their order, then the
    /// interface's, and links nothing else: no other stylesheet, no script
    /// and no sheet written inline.
    #[test]
    fn the_head_links_the_themes_sheets_then_the_interfaces() {
        for theme in themes() {
            let written = document(nc_catalog::Locale::En, theme);
            let head = written
                .split_once("</head>")
                .map(|(head, _)| head)
                .expect("a head");
            let mut expected: Vec<String> = theme
                .sheets()
                .iter()
                .map(|sheet| format!("{}{sheet}", crate::assets::DIST))
                .collect();
            expected.push(crate::assets::SHEET.to_owned());
            assert_eq!(values(head, "href"), expected, "{head}");
            assert_eq!(
                head.matches("<link rel=\"stylesheet\"").count(),
                expected.len(),
                "{head}"
            );
            for absent in ["<script", "<style", "src="] {
                assert!(!head.contains(absent), "the head writes {absent}: {head}");
            }
        }
    }

    /// Every address the document writes is a path the custom protocol
    /// answers, relative to the document: none names a scheme or a host, and
    /// none is a path the protocol does not hold.
    #[test]
    fn every_address_is_one_the_protocol_answers() {
        for theme in themes() {
            let written = document(nc_catalog::Locale::En, theme);
            let addresses: Vec<&str> = ["href", "src", "action", "srcset"]
                .iter()
                .flat_map(|name| values(&written, name))
                .collect();
            assert!(!addresses.is_empty(), "{written}");
            for address in addresses {
                assert!(
                    !address.contains(':') && !address.starts_with('/'),
                    "{address} is not relative to the document"
                );
                assert!(
                    matches!(
                        answer("GET", &format!("/{address}"), &written),
                        Answered::Found { .. }
                    ),
                    "the protocol does not answer {address}"
                );
            }
            for needle in ["http:", "https:", "url(", "@import", "noctua://"] {
                assert!(
                    !written.contains(needle),
                    "the document writes `{needle}`: {written}"
                );
            }
        }
    }

    /// The first run shows the footer alone: the body holds the workspace
    /// area, empty, and one footer — its corner at the start, its modeline,
    /// info line and prompt line in that order, and its corner at the end —
    /// always shown, and nothing else but the script.
    #[test]
    fn the_first_run_shows_the_footer_alone() {
        let written = document(nc_catalog::Locale::En, themes()[0]);
        let body = written
            .split_once("<body")
            .and_then(|(_, body)| body.split_once("<script"))
            .map(|(body, _)| body)
            .expect("a body");
        assert!(
            body.contains("<div id=\"noctua-workspace\" class=\"flex-1\"></div>"),
            "the workspace area is empty: {body}"
        );
        assert_eq!(body.matches("<footer").count(), 1, "{body}");
        let footer = body
            .split_once("<footer")
            .and_then(|(_, footer)| footer.split_once("</footer>"))
            .map(|(footer, _)| footer)
            .expect("a footer");
        assert!(
            footer.contains("role=\"group\"")
                && footer.contains(&format!("aria-label=\"{}\"", super::FOOTER_NAME))
                && footer.contains("data-nd-visibility=\"always\""),
            "{footer}"
        );
        assert_eq!(
            values(footer, "data-nd-kind"),
            ["modeline", "info", "prompt"],
            "{footer}"
        );
        assert_eq!(values(footer, "data-nd-side"), ["start", "end"], "{footer}");
        let outside = body.replace(footer, "");
        for element in [
            "<button",
            "<input",
            "<textarea",
            "<a ",
            "<img",
            "<p",
            "<span",
        ] {
            assert!(
                !outside.contains(element) && !footer.contains(element),
                "the first run draws {element}: {body}"
            );
        }
    }

    /// The script says the words the window reads: the scale, then that the
    /// document loaded, once the document and every sheet it links have
    /// loaded, and the scale again at each resize.
    #[test]
    fn the_script_says_the_words_the_window_reads() {
        assert!(
            SAYS_LOADED.contains(&format!(
                "window.addEventListener('load', () => {{ scale(); \
                 window.ipc.postMessage('{LOADED}'); }});"
            )),
            "{SAYS_LOADED}"
        );
        assert!(
            SAYS_LOADED.contains(&format!(
                "window.ipc.postMessage('{SCALE} ' + Math.round(window.devicePixelRatio * \
                 1000))"
            )),
            "{SAYS_LOADED}"
        );
        assert!(
            SAYS_LOADED.contains("window.addEventListener('resize', scale);"),
            "{SAYS_LOADED}"
        );
    }
}
