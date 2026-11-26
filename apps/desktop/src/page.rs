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
//! when the document is built — none, before the daemon has answered — with
//! the sidebar's region beside the workspace area and the drawer's below the
//! footer ([`crate::sidebar`], [`crate::drawer`]), each empty until a state
//! draws it, and neither a box of its own, so the design system's drawers
//! stand in the window's own row and column.
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
//! it ([`ACTION`]), and the page a tab of the drawer's tabs rail names when it
//! is pressed; and the prompt's text whenever an edit changes it
//! ([`DRAFT`]). `window.noctua.draw` brings the footer, the drawer or the
//! sidebar to the markup it is handed by changing only what that markup
//! changed since the last: an
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

use crate::drawer::TheDrawer;
use crate::footer::{TheFooter, View};
use crate::sidebar::TheSidebar;
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
/// [`ACTION`] and [`DRAFT`], and the keys, the lists and the focus), and
/// `window.noctua`'s calls, `draw`, `draft`, `keys`, `held` and `focus`,
/// which bring the page to the state and tell it the keys in force.
///
/// **The keys are heard before anything else hears them**, on the window in
/// its capture phase: the Leader's chord going down, which is said and kept
/// from the page; while its modifiers stay down, every other key, said with
/// whether the chord modifier is held and kept from the page; once one of them
/// is let go, the Leader going up; a key of the menu's table pressed alone on
/// the leader menu; and Esc on the drawer or on a prompt that filters a list.
/// Which modifiers are down is read off every key event, so a hold the window
/// is told of after its modifiers were let go ends at once. Everything else is
/// the page's: typing, the arrows and Enter of `option-list.js`, and the send
/// of `prompt.js`, which the script says with the option the keyboard is on.
pub const SCRIPT: &str = r"(() => {
  const say = (message) => window.ipc.postMessage(message);
  const scale = () => say('scale ' + Math.round(window.devicePixelRatio * 1000));
  window.addEventListener('load', () => { scale(); say('loaded'); });
  window.addEventListener('resize', scale);
  document.addEventListener('click', (event) => {
    const control = event.target instanceof Element ? event.target.closest('[data-nd-action]') : null;
    if (control) say('action ' + control.getAttribute('data-nd-action'));
    const tab = event.target instanceof Element ? event.target.closest('#noctua-drawer .nd-tab') : null;
    if (tab) say('choose pages ' + tab.getAttribute('data-nd-value'));
  });
  document.addEventListener('input', (event) => {
    const area = event.target;
    if (area instanceof HTMLTextAreaElement && area.matches('.nd-prompt-input')) say('draft ' + area.value);
  });
  const activeOf = (named) => {
    const list = named ? document.getElementById(named) : null;
    const option = list ? list.querySelector('[data-nd-active=true]') : null;
    return option ? option.getAttribute('data-nd-value') : null;
  };
  document.addEventListener('nd-prompt-send', (event) => {
    const area = event.target instanceof Element ? event.target.querySelector('.nd-prompt-input') : null;
    const active = activeOf(area ? area.getAttribute('aria-controls') : null);
    say(active === null ? 'send' : 'send ' + active);
  });
  document.addEventListener('nd-option-choose', (event) => {
    if (event.detail.value !== null) say('choose ' + event.detail.name + ' ' + event.detail.value);
  });
  document.addEventListener('nd-option-active', (event) => {
    if (event.detail.value !== null) say('active ' + event.detail.name + ' ' + event.detail.value);
  });
  document.addEventListener('focusin', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (target && target.matches('.nd-prompt-input')) say('focus prompt');
    else if (target && target.closest('#noctua-drawer')) say('focus drawer');
    else if (target && target.closest('#noctua-sidebar')) say('focus sidebar');
  });
  let keys = null;
  let held = false;
  let downs = [false, false, false, false];
  const flags = (event) => [event.ctrlKey, event.altKey, event.shiftKey, event.metaKey];
  const modifier = (code) => ['Control', 'Alt', 'Shift', 'Meta', 'OS'].some((name) => code.startsWith(name));
  const holds = () => keys.mods.every((on, at) => !on || downs[at]);
  const release = () => { if (held) { held = false; say('leader up'); } };
  window.addEventListener('keydown', (event) => {
    downs = flags(event);
    if (!keys || event.isComposing || event.keyCode === 229 || modifier(event.code)) return;
    if (event.code === keys.code && holds()) {
      event.preventDefault();
      if (!held && !event.repeat && downs.every((on, at) => on === keys.mods[at])) { held = true; say('leader down'); }
      return;
    }
    if (held && holds()) {
      event.preventDefault();
      event.stopPropagation();
      say('key held ' + event.code + ' ' + (downs[keys.chord] ? '1' : '0'));
      return;
    }
    release();
    const target = event.target instanceof Element ? event.target : null;
    const plain = !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey;
    if (!plain || !target) return;
    if (event.code === 'Escape' && (target.closest('#noctua-drawer') || target.hasAttribute('aria-controls'))) {
      event.preventDefault();
      say('escape');
    } else if (keys.menu.includes(event.code) && target.closest('[data-nd-option-list=leader]')) {
      event.preventDefault();
      event.stopPropagation();
      say('key menu ' + event.code);
    }
  }, true);
  window.addEventListener('keyup', (event) => {
    downs = flags(event);
    if (held && !holds()) release();
  }, true);
  window.addEventListener('blur', () => { downs = [false, false, false, false]; held = false; });
  const drawn = new Map();
  const regions = () => [document.querySelector('footer'), document.getElementById('noctua-drawer'), document.getElementById('noctua-sidebar')];
  for (const region of regions()) if (region) drawn.set(region.id || region.localName, region.cloneNode(true));
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
      const key = next.id || next.localName;
      const region = next.id ? document.getElementById(next.id) : document.querySelector(next.localName);
      if (!region || !drawn.has(key)) return;
      if (window.noctua.release) window.noctua.release(region);
      bring(region, drawn.get(key), next);
      drawn.set(key, next);
      if (window.noctua.enhance) window.noctua.enhance(region);
    },
    draft(text) {
      const area = document.querySelector('footer .nd-prompt-input');
      if (!area || area.value === text || area.closest('[data-nd-composing]')) return;
      area.value = text;
    },
    keys(told) { keys = told; },
    held() {
      if (!keys) return;
      if (holds()) held = true;
      else say('leader up');
    },
    focus(where) {
      const element = where === 'prompt'
        ? document.querySelector('footer .nd-prompt-input')
        : document.querySelector('#noctua-' + where + ' [role=listbox][tabindex]')
          || document.querySelector('#noctua-' + where + ' [role=toolbar] button');
      if (element && document.activeElement !== element) element.focus();
    },
  };
})();";

/// The module that hands the prompt, the lists, the drawers, the tabs rail
/// and the controls rail to the design system's scripts — the prompt's block
/// cursor, a list's walk, a drawer's `inert` and its focus given back when it
/// shuts around it, a strip's roles and its arrows, and a toolbar's one tab
/// stop — and hands `window.noctua` the same calls for a part a draw put in
/// the page, and the call that takes a list's and a strip's behaviour back
/// before a draw changes them. A drawer's is never taken back, so where the
/// focus came into it from is still known when a draw collapses it.
pub const ENHANCES: &str = r#"
import { enhancePrompts } from "./dist/prompt.js";
enhancePrompts();
import { enhanceOptionLists, releaseOptionLists } from "./dist/option-list.js";
enhanceOptionLists();
import { enhanceDrawers } from "./dist/drawer.js";
import { enhanceTabs, releaseTabs } from "./dist/tabs.js";
import { enhanceRoving } from "./dist/roving.js";
window.noctua.enhance = (root) => { enhancePrompts(root); enhanceOptionLists(root); enhanceDrawers(root); enhanceTabs(root); enhanceRoving(root); };
window.noctua.release = (root) => { releaseOptionLists(root); releaseTabs(root); };"#;

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
/// window in one column — a row holding the sidebar at its start and the
/// workspace area, which holds nothing yet, then the footer and the drawer
/// below it — then the page's script and the module that enhances the
/// prompt. **The row and the workspace area take what the footer and the
/// drawer leave**, and the row hides what overflows it, so a sidebar's list
/// scrolls inside the sidebar rather than pushing the footer out of the
/// window.
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
                div { class: "flex flex-1 overflow-hidden",
                    TheSidebar { view: crate::sidebar::View::hidden() }
                    div { id: "noctua-workspace", class: "flex-1" }
                }
                TheFooter { view: footer }
                TheDrawer { view: crate::drawer::View::hidden() }
            }
            script { dangerous_inner_html: SCRIPT }
            script { r#type: "module", dangerous_inner_html: ENHANCES }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
            assert_eq!(
                imported,
                [
                    "./dist/prompt.js",
                    "./dist/option-list.js",
                    "./dist/drawer.js",
                    "./dist/tabs.js",
                    "./dist/roving.js"
                ],
                "{written}"
            );
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

    /// The first run shows the footer alone: the body holds a row of the
    /// sidebar's region, empty, and the workspace area, empty, then the
    /// footer — the first run's, as [`crate::footer::markup`] writes it —
    /// then the drawer's region, empty, and nothing else but the scripts.
    #[test]
    pub(crate) fn the_first_run_shows_the_footer_alone() {
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
                     <div class=\"flex flex-1 overflow-hidden\">\
                     <div id=\"noctua-sidebar\" class=\"contents\"></div>\
                     <div id=\"noctua-workspace\" class=\"flex-1\"></div></div>{}\
                     <div id=\"noctua-drawer\" class=\"contents\"></div></main>",
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
            "held = true; say('leader down');".to_owned(),
            "const release = () => { if (held) { held = false; say('leader up'); } };".to_owned(),
            "say('key held ' + event.code + ' ' + (downs[keys.chord] ? '1' : '0'));".to_owned(),
            "say('key menu ' + event.code);".to_owned(),
            "say('escape');".to_owned(),
            "say('choose ' + event.detail.name + ' ' + event.detail.value);".to_owned(),
            "say('active ' + event.detail.name + ' ' + event.detail.value);".to_owned(),
            "say(active === null ? 'send' : 'send ' + active);".to_owned(),
            "say('focus prompt');".to_owned(),
            "say('focus drawer');".to_owned(),
            "say('focus sidebar');".to_owned(),
            "say('choose pages ' + tab.getAttribute('data-nd-value'));".to_owned(),
            "window.addEventListener('keydown', (event) => {".to_owned(),
            "  }, true);\n  window.addEventListener('keyup'".to_owned(),
        ] {
            assert!(SCRIPT.contains(&said), "{said} is not in {SCRIPT}");
        }
        assert!(
            ENHANCES.contains(
                "import { enhancePrompts } from \"./dist/prompt.js\";\nenhancePrompts();"
            ),
            "{ENHANCES}"
        );
        for handed in [
            "enhanceOptionLists();",
            "enhancePrompts(root); enhanceOptionLists(root); enhanceDrawers(root); \
             enhanceTabs(root); enhanceRoving(root);",
            "window.noctua.release = (root) => { releaseOptionLists(root); releaseTabs(root); };",
        ] {
            assert!(ENHANCES.contains(handed), "{handed} is not in {ENHANCES}");
        }
        for (shipped, exported) in [
            ("drawer.js", "export function enhanceDrawers("),
            ("tabs.js", "export function enhanceTabs("),
            ("tabs.js", "export function releaseTabs("),
            ("roving.js", "export function enhanceRoving("),
        ] {
            assert!(
                noctua_design_dist::FILES
                    .iter()
                    .any(|(name, contents)| *name == shipped && contents.contains(exported)),
                "the design system ships {exported} in {shipped}, which the module imports"
            );
        }
        assert!(
            noctua_design_dist::FILES
                .iter()
                .any(|(name, contents)| *name == "option-list.js"
                    && contents.contains("export function enhanceOptionLists(")
                    && contents.contains("export function releaseOptionLists(")),
            "the design system ships the list script the module imports"
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
