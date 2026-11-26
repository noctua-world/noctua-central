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
//! workspace area above it holds nothing, and the footer is
//! [`crate::footer`]'s, drawn from the view of the state the window holds
//! when the document is built — none, before the daemon has answered.
//!
//! **Everything it links is the custom protocol's** ([`crate::assets`]), by a
//! path relative to the document, so no address in it names a host or a
//! scheme, and every path is one the protocol answers.
//!
//! **Its one script of its own** ([`SCRIPT`]) is the page's half of the
//! window: what the page says through the webview's message channel, and the
//! two calls the window brings the page to the state through. It says the
//! page's scale — how many device pixels one CSS pixel is — and then
//! [`LOADED`] once the document and every sheet it links have loaded, and
//! the scale again whenever the page is resized; the command a control runs
//! when it is activated, by the `data-nd-action` the design system writes on
//! it ([`ACTION`]); and the prompt's text whenever an edit changes it
//! ([`DRAFT`]). `window.noctua.draw` brings the footer to the markup it is
//! handed by changing only what that markup changed since the last: an
//! attribute set, removed or changed, a text changed, an element whose name
//! changed replaced and one added or taken away — so an element that stays
//! keeps its focus, and what the design system's scripts added to the page,
//! the prompt's block among it, stays as they left it; the text area's
//! content is never touched, since it is what the person typed.
//! `window.noctua.draft` writes a draft into the prompt's text area, unless
//! it holds it already or an input method is composing in it. What the
//! window does with each word is [`crate::host`]'s, and what it sends
//! [`crate::footer::Drawing`]'s.
//!
//! **The design system's behaviour is its own scripts'**: the document loads
//! the design system's `prompt.js` as a module and enhances the prompt with
//! it, so the block cursor, its blinking and the text area's selection,
//! composition and clipboard are the design system's and the engine's
//! (§12.7).
//!
//! **Every class written here is a utility the sheet holds a rule for**, and
//! one written outside the closed set `cargo xtask check` admits — an
//! arbitrary value, a colour written out, a modifier — fails the gate
//! (`xtask/src/tailwind.rs`), which also rebuilds the sheet from the classes
//! written here and refuses one that differs from `crate::sheet`.

use dioxus::prelude::*;

use crate::footer::{TheFooter, View};
use crate::theme::Theme;

/// The word the document's script says once the document and every sheet it
/// links have loaded, through the webview's message channel.
pub const LOADED: &str = "loaded";

/// The word the document's script says the page's scale with: how many
/// device pixels one CSS pixel is, in thousandths, written after it.
pub const SCALE: &str = "scale";

/// The word the document's script says a control was activated with: the
/// command it runs, its `data-nd-action`, written after it.
pub const ACTION: &str = "action";

/// The word the document's script says the prompt's text with, written whole
/// after it and one space.
pub const DRAFT: &str = "draft";

/// The document's own script: what the page says ([`LOADED`], [`SCALE`],
/// [`ACTION`] and [`DRAFT`]), and `window.noctua`'s two calls, `draw` and
/// `draft`, which bring the page to the state.
pub const SCRIPT: &str = r"(() => {
  const say = (message) => window.ipc.postMessage(message);
  const scale = () => say('scale ' + Math.round(window.devicePixelRatio * 1000));
  window.addEventListener('load', () => { scale(); say('loaded'); });
  window.addEventListener('resize', scale);
  document.addEventListener('click', (event) => {
    const control = event.target instanceof Element ? event.target.closest('[data-nd-action]') : null;
    if (control) say('action ' + control.getAttribute('data-nd-action'));
  });
  document.addEventListener('input', (event) => {
    const area = event.target;
    if (area instanceof HTMLTextAreaElement && area.matches('.nd-prompt-input')) say('draft ' + area.value);
  });
  const footer = document.querySelector('footer');
  let drawn = footer.cloneNode(true);
  const alike = (one, other) => one.nodeType === other.nodeType && one.nodeName === other.nodeName;
  const bring = (live, was, next) => {
    for (const { name, value } of Array.from(next.attributes)) {
      if (was.getAttribute(name) !== value) live.setAttribute(name, value);
    }
    for (const { name } of Array.from(was.attributes)) {
      if (!next.hasAttribute(name)) live.removeAttribute(name);
    }
    if (next.localName === 'textarea') return;
    const lives = Array.from(live.childNodes);
    const before = was.childNodes;
    const after = next.childNodes;
    for (let at = 0; at < after.length; at += 1) {
      const one = after[at];
      const old = before[at];
      const here = lives[at];
      if (old && here && alike(old, one) && alike(here, one)) {
        if (one.nodeType === Node.ELEMENT_NODE) bring(here, old, one);
        else if (old.nodeValue !== one.nodeValue) here.nodeValue = one.nodeValue;
      } else if (old && here) {
        live.replaceChild(one.cloneNode(true), here);
      } else {
        live.insertBefore(one.cloneNode(true), lives[before.length] || null);
      }
    }
    for (let at = after.length; at < before.length; at += 1) {
      if (lives[at]) lives[at].remove();
    }
  };
  window.noctua = {
    draw(markup) {
      const template = document.createElement('template');
      template.innerHTML = markup;
      const next = template.content.firstElementChild;
      if (!next) return;
      bring(footer, drawn, next);
      drawn = next;
      if (window.noctua.enhance) window.noctua.enhance(footer);
    },
    draft(text) {
      const area = footer.querySelector('.nd-prompt-input');
      if (!area || area.value === text || area.closest('[data-nd-composing]')) return;
      area.value = text;
    },
  };
})();";

/// The module that hands the prompt to the design system's script, which
/// draws its block cursor, and hands `window.noctua` the same call for a
/// prompt a draw put in the page.
pub const ENHANCES: &str = r#"import { enhancePrompts } from "./dist/prompt.js";
enhancePrompts();
window.noctua.enhance = enhancePrompts;"#;

/// The document, in `locale` and `theme`, its footer `footer`, as the
/// webview is handed it.
///
/// **Its root element is written here and its content as `rsx!`**, which
/// renders the elements inside a document and not the document's own; the
/// root's attributes are the theme's, each a word of the design system's
/// closed set, so none needs escaping.
#[must_use]
pub fn document(locale: nc_catalog::Locale, theme: Theme, footer: &View) -> String {
    let rendered = dioxus_ssr::render_element(rsx! {
        Document { sheets: theme.sheets(), footer: footer.clone() }
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
/// window — the workspace area, which holds nothing yet, and the footer —
/// then the page's script and the module that enhances the prompt.
#[component]
fn Document(sheets: Vec<&'static str>, footer: View) -> Element {
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
                TheFooter { view: footer }
            }
            script { dangerous_inner_html: SCRIPT }
            script { r#type: "module", dangerous_inner_html: ENHANCES }
        }
    }
}

#[cfg(test)]
mod tests {
    use nc_catalog::Locale;
    use nc_channel::Channel;
    use noctua_design::model::{Mode, Palette};

    use super::{ACTION, DRAFT, ENHANCES, LOADED, SCALE, SCRIPT, document};
    use crate::assets::{Answered, answer};
    use crate::footer::{Context, View, markup, view};
    use crate::theme::Theme;

    /// A theme of each mode.
    fn themes() -> [Theme; 2] {
        [
            Theme::chosen(Mode::Light, Palette::DEFAULT, true),
            Theme::chosen(Mode::Dark, Palette::JadeVivid, false),
        ]
    }

    /// The footer of the first run, before the daemon has answered, in
    /// `locale`.
    fn footer(locale: Locale) -> View {
        view(
            None,
            Context {
                locale,
                channel: Channel::Production,
            },
        )
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
    /// scripts last in the body.
    #[test]
    fn the_document_is_one_in_its_locale_and_its_theme() {
        for locale in Locale::ALL {
            for theme in themes() {
                let written = document(*locale, theme, &footer(*locale));
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
                let script = written.find(SCRIPT).expect("the script is in it");
                let module = written.find(ENHANCES).expect("the module is in it");
                let main = written.find("<main").expect("the root is in it");
                assert!(
                    main < script && script < module,
                    "the scripts follow the root: {written}"
                );
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
            let written = document(Locale::En, theme, &footer(Locale::En));
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

    /// Every address the document writes — an attribute's, and a module's
    /// it imports — is a path the custom protocol answers, relative to the
    /// document: none names a scheme or a host, and none is a path the
    /// protocol does not hold.
    #[test]
    fn every_address_is_one_the_protocol_answers() {
        for theme in themes() {
            let written = document(Locale::En, theme, &footer(Locale::En));
            let imported: Vec<&str> = written
                .split(" from \"")
                .skip(1)
                .filter_map(|rest| rest.split_once('"').map(|(address, _)| address))
                .collect();
            assert_eq!(imported, ["./dist/prompt.js"], "{written}");
            let addresses: Vec<&str> = ["href", "src", "action", "srcset"]
                .iter()
                .flat_map(|name| values(&written, name))
                .chain(
                    imported
                        .iter()
                        .map(|address| address.trim_start_matches("./")),
                )
                .collect();
            assert!(addresses.len() > imported.len(), "{written}");
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
    /// area, empty, then the footer — the first run's, as
    /// [`crate::footer::markup`] writes it — and nothing else but the
    /// scripts.
    #[test]
    fn the_first_run_shows_the_footer_alone() {
        for locale in Locale::ALL {
            let first = footer(*locale);
            let written = document(*locale, themes()[0], &first);
            let body = written
                .split_once("<body class=\"h-screen overflow-hidden\">")
                .and_then(|(_, body)| body.split_once("<script"))
                .map(|(body, _)| body)
                .expect("a body");
            assert_eq!(
                body,
                format!(
                    "<main id=\"noctua\" class=\"flex h-full w-full flex-col\">\
                     <div id=\"noctua-workspace\" class=\"flex-1\"></div>{}</main>",
                    markup(&first)
                )
            );
        }
    }

    /// The script says the words the window reads — the scale, that the
    /// document loaded once it and every sheet it links have loaded, the
    /// command a control runs, and the prompt's text whole — and the module
    /// enhances the prompt with the design system's script.
    #[test]
    fn the_script_says_the_words_the_window_reads() {
        for said in [
            format!("window.addEventListener('load', () => {{ scale(); say('{LOADED}'); }});"),
            format!("say('{SCALE} ' + Math.round(window.devicePixelRatio * 1000))"),
            "window.addEventListener('resize', scale);".to_owned(),
            format!("say('{ACTION} ' + control.getAttribute('data-nd-action'))"),
            format!("say('{DRAFT} ' + area.value)"),
            "const say = (message) => window.ipc.postMessage(message);".to_owned(),
        ] {
            assert!(SCRIPT.contains(&said), "{said} is not in {SCRIPT}");
        }
        assert!(
            ENHANCES.contains(
                "import { enhancePrompts } from \"./dist/prompt.js\";\nenhancePrompts();"
            ),
            "{ENHANCES}"
        );
        assert!(
            noctua_design_dist::FILES
                .iter()
                .any(|(name, contents)| *name == "prompt.js"
                    && contents.contains("export function enhancePrompts(")),
            "the design system ships the script the module imports"
        );
    }
}
