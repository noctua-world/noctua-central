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
//! scheme, and every path is one the protocol answers — **and nothing else
//! is loaded at all** ([`POLICY`]): the first thing its head says is its
//! content security policy, which lets the page load a script, a sheet, an
//! image or a font from its own origin alone — an image written inline as
//! well — and connect, frame, submit or set a base nowhere, so no script in
//! the page, its own or one an injection put there, can fetch an address or
//! send anything off the machine. Its two scripts are files the protocol
//! serves, not text inside it, so the policy runs no inline script at all.
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
//! keeps its focus, a keyboard on one taken away, or on one left unable to
//! take it, goes onto its part's own element where the part still holds
//! one, and what the design system's
//! scripts added to the page,
//! the prompt's block among it, stays as they left it; the text area's
//! content is never touched, since it is what the person typed.
//! `window.noctua.draft` writes a draft into the prompt's text area, unless
//! it holds it already or an input method is composing in it. It says what
//! the system's own pointer is over, `window.noctua.seats` writes every other
//! seat the window names onto the page through the design system's
//! `seats.js` (`crate::seat`, §12.18), and `window.noctua.witness` says what
//! the page shows, which the window asks for a test's install alone. What the
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

/// The document's content security policy: a script, a sheet and a font
/// from its own origin alone, an image from it or written inline, and
/// nothing to connect to, to frame, to submit to or to take as a base — every
/// other kind of load refused by `default-src`.
pub const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     img-src 'self' data:; font-src 'self'; connect-src 'none'; frame-src 'none'; \
     base-uri 'none'; form-action 'none'";

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
/// [`ACTION`] and [`DRAFT`], and the keys, the lists, the focus and what the
/// system's own pointer is over), and `window.noctua`'s calls, `draw`,
/// `draft`, `keys`, `held`, `focus`, `seats` and `witness`, which bring the
/// page to the state, tell it the keys in force and the seats that are not
/// the system's own, and ask it what it shows.
///
/// **The system's own pointer is said by what it is over**: whenever it
/// moves onto another element, the target the nearest control or option
/// around it names — its `data-nd-action` or its `data-nd-value`, a word of
/// the closed shape `shell.hover` takes — or, where there is none and once it
/// leaves the page, nothing; a word said once is not said again until it
/// changes. **Every other seat is written where the window says**
/// (`crate::seat`): `seats` takes every seat drawn that the window no longer
/// names off the page, then writes each seat named through the design
/// system's `seats.js` — its hover and its drag on the element whose
/// `data-nd-action` or `data-nd-value` is the word named, compared as text,
/// and its keyboard on the element its selection names inside the part its
/// keyboard is on, else on that part's own element, the one `focus` gives the
/// system's own keyboard.
///
/// **`witness` answers what the page shows**, asked only for a test's install
/// (`crate::host`), once every animation running that ends has ended — a
/// drawer sliding open — or a second has passed: its size in CSS pixels;
/// whether every element drawn stands inside the window, or inside something
/// that clips it, each way; whether every line of the footer stands wholly
/// inside the window and inside every box that clips it; whether every
/// control drawn is at least `--noctua-target-min` each way, and every box
/// that scrolls what it holds past what it shows is a stop of the keyboard or
/// holds one, so a keyboard alone reaches all it holds; whether
/// every seat's attribute changes how the element it is on is drawn, judged
/// where the system's own seat is not in that state there; which part the
/// system's own keyboard is in — none where the element it is on can no
/// longer take it, since the system's webview drops it from there onto
/// nothing a moment later; which parts draw anything; and every seat's
/// state, by the element it is on.
///
/// **The keys are heard before anything else hears them**, on the window in
/// its capture phase: the Leader's chord going down, which is said and kept
/// from the page; while its modifiers stay down, every other key, said with
/// whether the chord modifier is held and kept from the page; once one of them
/// is let go, the Leader going up; a key of the menu's table pressed alone on
/// the leader menu; and Esc on the drawer or on a prompt that filters a list.
/// Which modifiers are down is read off every key event, so a hold the window
/// is told of after its modifiers were let go ends at once — **and Super from
/// its own key as well**, held from its going down to its going up, since
/// `WebKitGTK` sets no Meta flag on any key while it is held. Everything else
/// is the page's: typing, the arrows and Enter of `option-list.js`, and the
/// send of `prompt.js`, which the script says with the option the keyboard is
/// on.
///
/// **The keyboard is said wherever it goes**: onto the prompt line, the
/// drawer, the sidebar, the info line or the modeline, as that part, and onto
/// anything in the workspace area or given up to nothing in the page — a press
/// on nothing, outside a draw — as the workspace area; a window that loses the
/// keyboard to another keeps its place and says nothing. `focus` puts it on a
/// part's own element, and gives it up to nothing for the workspace area.
///
/// **A draw never leaves the keyboard on nothing behind the state's back**:
/// one that takes away the element the keyboard is on — the leader menu's
/// list, closed in a drawer that still shows a page — or leaves it unable to
/// take it — the expanded info line's box, no stop once the line collapses,
/// or the leader menu's list become the command list, which the prompt
/// walks — puts it on the own element of the part it was in, where that part
/// still holds one that can take it, so the page stays where the state says.
/// An element that can take it is one the engine gives it to, a button or a
/// field enabled, a link or one with a tab index, drawn and neither hidden
/// nor inert; the system's webview leaves the keyboard on one that can no
/// longer take it only until a later pass of its own drops it onto nothing.
/// Where the part is gone
/// whole, the state has moved the keyboard already — the kernel keeps none on
/// a drawer that shows nothing open — and the window puts it there. **And a
/// part `focus` names that cannot take the keyboard** — not drawn, or inside
/// something hidden or inert, as a collapsed sidebar is — is answered with
/// where the keyboard is, said as above, so the state follows the page rather
/// than name a part the page has no keyboard in.
///
/// **A draw leaves nothing pointing at an option that is gone**: once a part
/// is brought to its markup and enhanced again, an `aria-activedescendant`
/// naming no element of the page is taken off whatever holds it — the prompt
/// that named a list a draw closed.
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
  const regionOf = (element) => element.closest('#noctua-drawer') ? 'drawer'
    : element.closest('#noctua-sidebar') ? 'sidebar'
      : element.closest('footer .nd-footer-line[data-nd-kind=prompt]') ? 'prompt'
        : element.closest('footer .nd-footer-line[data-nd-kind=info]') ? 'info'
          : element.closest('footer .nd-footer-line[data-nd-kind=modeline]') ? 'modeline'
            : element.closest('#noctua-workspace') ? 'workspace' : null;
  let drawing = false;
  document.addEventListener('focusin', (event) => {
    const region = event.target instanceof Element ? regionOf(event.target) : null;
    if (region) say('focus ' + region);
  });
  document.addEventListener('focusout', () => {
    if (drawing) return;
    setTimeout(() => {
      const active = document.activeElement;
      if (!active || active === document.body) say('focus workspace');
    }, 0);
  });
  const TARGET = /^[A-Za-z0-9._:\/-]{1,64}$/;
  const targetOf = (element) => {
    const word = element.getAttribute('data-nd-action') || element.getAttribute('data-nd-value');
    return word && TARGET.test(word) ? word : null;
  };
  let hovered = null;
  const hover = (target) => {
    if (target === hovered) return;
    hovered = target;
    say(target === null ? 'hover' : 'hover ' + target);
  };
  document.addEventListener('pointerover', (event) => {
    const element = event.target instanceof Element ? event.target.closest('[data-nd-action], [data-nd-value]') : null;
    hover(element ? targetOf(element) : null);
  });
  document.documentElement.addEventListener('pointerleave', () => hover(null));
  const partOf = (where) => where === 'prompt'
    ? document.querySelector('footer .nd-prompt-input')
    : where === 'drawer' || where === 'sidebar'
      ? document.querySelector('#noctua-' + where + ' [role=listbox][tabindex]')
        || document.querySelector('#noctua-' + where + ' [role=toolbar] button')
      : where === 'modeline' ? document.querySelector('footer .nd-footer-line[data-nd-kind=modeline] button')
        : where === 'info' ? document.querySelector('footer .nd-footer-line[data-nd-kind=info] .nd-footer-line-toggle')
          : null;
  const takes = (element) => element !== null && !element.closest('[inert], [hidden]');
  const TAKERS = '[tabindex], a[href], button:enabled, input:enabled, select:enabled, textarea:enabled';
  const keeps = (element) => takes(element) && element.matches(TAKERS) && element.getClientRects().length > 0;
  const keyboardAt = () => {
    const active = document.activeElement;
    return active && active !== document.body ? regionOf(active) || 'workspace' : 'workspace';
  };
  const within = (where) => where === 'prompt'
    ? document.querySelector('footer')
    : where === 'drawer' || where === 'sidebar' ? document.getElementById('noctua-' + where) : null;
  const named = (word, inside) => {
    const root = inside || document.getElementById('noctua');
    if (word === null || !root) return null;
    for (const element of root.querySelectorAll('[data-nd-action], [data-nd-value]')) {
      if (element.getAttribute('data-nd-action') === word || element.getAttribute('data-nd-value') === word) return element;
    }
    return null;
  };
  const STAMPS = [['hover', 'data-nd-seat-hover'], ['focus', 'data-nd-seat-focus'], ['active', 'data-nd-seat-active'], ['drag', 'data-nd-seat-drag']];
  const NATIVE = { hover: ':hover', focus: ':focus-within', active: ':active', drag: null };
  const LOOKS = ['background-color', 'color', 'outline-style', 'outline-color', 'outline-width', 'box-shadow', 'border-top-color', 'border-left-color', 'border-right-color', 'text-decoration-line', 'opacity'];
  let keys = null;
  let held = false;
  let downs = [false, false, false, false];
  const SUPER = ['MetaLeft', 'MetaRight', 'OSLeft', 'OSRight'];
  let superDown = false;
  const flags = (event) => [event.ctrlKey, event.altKey, event.shiftKey, event.metaKey || superDown];
  const modifier = (code) => ['Control', 'Alt', 'Shift', 'Meta', 'OS'].some((name) => code.startsWith(name));
  const holds = () => keys.mods.every((on, at) => !on || downs[at]);
  const release = () => { if (held) { held = false; say('leader up'); } };
  window.addEventListener('keydown', (event) => {
    if (SUPER.includes(event.code)) superDown = true;
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
    if (SUPER.includes(event.code)) superDown = false;
    downs = flags(event);
    if (held && !holds()) release();
  }, true);
  window.addEventListener('blur', () => { downs = [false, false, false, false]; superDown = false; held = false; });
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
      const before = document.activeElement;
      const held = before && before !== document.body && region.contains(before) ? regionOf(before) : null;
      drawing = true;
      if (window.noctua.release) window.noctua.release(region);
      bring(region, drawn.get(key), next);
      drawn.set(key, next);
      if (window.noctua.enhance) window.noctua.enhance(region);
      for (const holder of document.querySelectorAll('[aria-activedescendant]')) {
        if (!document.getElementById(holder.getAttribute('aria-activedescendant'))) holder.removeAttribute('aria-activedescendant');
      }
      drawing = false;
      const after = document.activeElement;
      if (held && (!after || !keeps(after))) {
        const part = partOf(held);
        if (keeps(part)) part.focus();
      }
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
      const active = document.activeElement;
      if (where === 'workspace') {
        if (active && active !== document.body && !active.closest('#noctua-workspace')) active.blur();
        return;
      }
      const element = partOf(where);
      if (!takes(element)) {
        say('focus ' + keyboardAt());
        return;
      }
      if (active !== element) element.focus();
    },
    seats(told) {
      const seat = window.noctua.seat;
      if (!seat) return;
      const kept = new Set(told.map((one) => one.seat));
      for (const [, name] of STAMPS) {
        for (const element of document.querySelectorAll('[' + name + ']')) {
          for (const id of element.getAttribute(name).split(' ')) if (id && !kept.has(id)) seat.clear(id);
        }
      }
      for (const one of told) {
        seat.hover(one.seat, named(one.hover));
        seat.focus(one.seat, named(one.selection, within(one.part)) || partOf(one.part));
        seat.drag(one.seat, named(one.drag));
      }
    },
    witness() {
      const root = document.getElementById('noctua');
      if (!root) return;
      root.getBoundingClientRect();
      const ending = document.getAnimations()
        .filter((one) => one.effect && Number.isFinite(one.effect.getComputedTiming().endTime))
        .map((one) => one.finished.catch(() => null));
      Promise.race([Promise.all(ending), new Promise((done) => setTimeout(done, 1000))])
        .then(() => window.noctua.witnessed(root));
    },
    witnessed(root) {
      const bit = (held) => (held ? '1' : '0');
      const rendered = (element) => {
        const box = element.getBoundingClientRect();
        return box.width > 0 && box.height > 0
          && !element.closest('[hidden], [inert], [aria-hidden=true]')
          && getComputedStyle(element).visibility !== 'hidden';
      };
      const clipped = (element, across) => {
        for (let at = element.parentElement; at && at !== root.parentElement; at = at.parentElement) {
          const style = getComputedStyle(at);
          if ((across ? style.overflowX : style.overflowY) !== 'visible') return true;
        }
        return false;
      };
      const viewport = window.visualViewport;
      const width = viewport ? viewport.width : window.innerWidth;
      const height = viewport ? viewport.height : window.innerHeight;
      let fits = true;
      for (const element of [root, ...root.querySelectorAll('*')]) {
        if (!rendered(element)) continue;
        const box = element.getBoundingClientRect();
        if ((box.left < -0.5 || box.right > width + 0.5) && !clipped(element, true)) fits = false;
        if ((box.top < -0.5 || box.bottom > height + 0.5) && !clipped(element, false)) fits = false;
      }
      const inside = (box, outer) => box.left >= outer.left - 0.5 && box.top >= outer.top - 0.5
        && box.right <= outer.right + 0.5 && box.bottom <= outer.bottom + 0.5;
      const whole = (element) => {
        const box = element.getBoundingClientRect();
        if (!inside(box, { left: 0, top: 0, right: width, bottom: height })) return false;
        for (let at = element.parentElement; at && at !== document.documentElement; at = at.parentElement) {
          const style = getComputedStyle(at);
          if (style.overflowX === 'visible' && style.overflowY === 'visible') continue;
          if (!inside(box, at.getBoundingClientRect())) return false;
        }
        return true;
      };
      const lines = Array.from(root.querySelectorAll('footer .nd-footer-line')).filter(rendered).every(whole);
      const least = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--noctua-target-min'));
      let reachable = least > 0;
      for (const control of root.querySelectorAll('button, textarea, input, select, a[href], [role=option], [role=tab], [role=button]')) {
        if (!rendered(control)) continue;
        const box = control.getBoundingClientRect();
        if (box.width < least - 0.5 || box.height < least - 0.5) reachable = false;
      }
      const SCROLLS = ['auto', 'scroll'];
      const stops = (element) => [element, ...element.querySelectorAll('*')].some((one) => one.tabIndex >= 0 && rendered(one));
      for (const element of [root, ...root.querySelectorAll('*')]) {
        if (!rendered(element)) continue;
        const style = getComputedStyle(element);
        const scrolls = (SCROLLS.includes(style.overflowY) && element.scrollHeight - element.clientHeight > 1)
          || (SCROLLS.includes(style.overflowX) && element.scrollWidth - element.clientWidth > 1);
        if (scrolls && !stops(element)) reachable = false;
      }
      const placeOf = (element) => regionOf(element) || 'elsewhere';
      const active = document.activeElement;
      const keyboard = !active || !keeps(active) ? '-' : placeOf(active);
      const holds = (part) => part !== null && Array.from(part.querySelectorAll('*')).some(rendered);
      const shows = [['footer', document.querySelector('footer')], ['drawer', document.getElementById('noctua-drawer')],
        ['sidebar', document.getElementById('noctua-sidebar')], ['workspace', document.getElementById('noctua-workspace')]]
        .filter(([, part]) => holds(part)).map(([word]) => word);
      const look = (element) => [element, element.nextElementSibling, element.parentElement].map((one) => {
        if (!one) return '';
        const style = getComputedStyle(one);
        return LOOKS.map((name) => style.getPropertyValue(name)).join('|');
      }).join('/');
      const seen = [];
      let styled = true;
      for (const [state, name] of STAMPS) {
        for (const element of root.querySelectorAll('[' + name + ']')) {
          for (const id of element.getAttribute(name).split(' ').filter(Boolean)) {
            const deeper = Array.from(element.querySelectorAll('[' + name + ']'))
              .some((inner) => inner.getAttribute(name).split(' ').includes(id));
            if (deeper) continue;
            seen.push(id + ':' + state + ':' + (targetOf(element) || placeOf(element)));
            if (NATIVE[state] === null || element.matches(NATIVE[state])) continue;
            const value = element.getAttribute(name);
            const still = [element, element.nextElementSibling, element.parentElement].filter(Boolean);
            const moving = still.map((one) => one.style.transition);
            for (const one of still) one.style.transition = 'none';
            const stamped = look(element);
            element.removeAttribute(name);
            const bare = look(element);
            element.setAttribute(name, value);
            look(element);
            still.forEach((one, at) => { one.style.transition = moving[at]; });
            if (stamped === bare) styled = false;
          }
        }
      }
      say(['seen', window.innerWidth, window.innerHeight, bit(fits), bit(lines), bit(reachable), bit(styled), keyboard,
        shows.length ? shows.join(',') : '-', ...seen].join(' '));
    },
  };
})();";

/// The module that hands the prompt, the lists, the drawers, the tabs rail
/// and the controls rail to the design system's scripts — the prompt's block
/// cursor, a list's walk, a drawer's `inert` and its focus given back when it
/// shuts around it, a strip's roles and its arrows, and a toolbar's one tab
/// stop — and hands `window.noctua` the same calls for a part a draw put in
/// the page, the call that takes a list's and a strip's behaviour back
/// before a draw changes them, and the design system's four calls that write
/// a seat's hover, focus and drag and take a seat off the page (`seats.js`).
///
/// A drawer's behaviour is never taken back, so where the focus came into it
/// from is still known when a draw collapses it.
pub const ENHANCES: &str = r#"
import { enhancePrompts } from "./dist/prompt.js";
enhancePrompts();
import { enhanceOptionLists, releaseOptionLists } from "./dist/option-list.js";
enhanceOptionLists();
import { enhanceDrawers } from "./dist/drawer.js";
import { enhanceTabs, releaseTabs } from "./dist/tabs.js";
import { enhanceRoving } from "./dist/roving.js";
import { seatHover, seatFocus, seatDrag, clearSeat } from "./dist/seats.js";
window.noctua.enhance = (root) => { enhancePrompts(root); enhanceOptionLists(root); enhanceDrawers(root); enhanceTabs(root); enhanceRoving(root); };
window.noctua.release = (root) => { releaseOptionLists(root); releaseTabs(root); };
window.noctua.seat = { hover: seatHover, focus: seatFocus, drag: seatDrag, clear: clearSeat };"#;

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

/// The document's markup: its head, saying its policy and linking `sheets`
/// of the design system and then the interface's sheet, and the interface's
/// root, filling the window in one column — a row holding the sidebar at its
/// start and the workspace area, which holds nothing yet, then the footer and
/// the drawer below it — then the page's script and the module that enhances
/// the prompt. **The row and the workspace area take what the footer and the
/// drawer leave**, and the row hides what overflows it, so a sidebar's list
/// scrolls inside the sidebar rather than pushing the footer out of the
/// window. **The body is the window, edge to edge**: its auto margins are
/// used as none, since a block whose width is auto takes none (CSS 2,
/// sections 10.3.3 and 10.6.3) — the one way the utilities alone take back the
/// engine's own margin on the body, Tailwind's preflight not being imported
/// and its spacing scale emitting nothing here.
#[component]
fn Document(sheets: Vec<&'static str>, footer: View) -> Element {
    let linked = sheets
        .iter()
        .map(|sheet| format!("{}{sheet}", crate::assets::DIST));
    rsx! {
        head {
            meta { charset: "utf-8" }
            meta { "http-equiv": "Content-Security-Policy", content: POLICY }
            for href in linked {
                link { rel: "stylesheet", href }
            }
            link { rel: "stylesheet", href: crate::assets::SHEET }
        }
        body { class: "m-auto h-screen overflow-hidden",
            main { id: "noctua", class: "flex h-full w-full flex-col",
                div { class: "flex flex-1 overflow-hidden",
                    TheSidebar { view: crate::sidebar::View::hidden() }
                    div { id: "noctua-workspace", class: "flex-1" }
                }
                TheFooter { view: footer }
                TheDrawer { view: crate::drawer::View::hidden() }
            }
            script { src: crate::assets::SCRIPT }
            script { r#type: "module", src: crate::assets::ENHANCES }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use nc_catalog::Locale;
    use nc_channel::Channel;
    use noctua_design::model::{Mode, Palette};

    use super::{ACTION, DRAFT, ENHANCES, LOADED, POLICY, SCALE, SCRIPT, document};
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
                let script = written
                    .find("<script src=\"noctua.js\"></script>")
                    .expect("the script is in it");
                let module = written
                    .find("<script type=\"module\" src=\"enhances.js\"></script>")
                    .expect("the module is in it");
                let main = written.find("<main").expect("the root is in it");
                assert!(
                    main < script && script < module,
                    "the scripts follow the root: {written}"
                );
                assert_eq!(
                    written.matches("<script").count(),
                    2,
                    "no script is written inline: {written}"
                );
                assert!(written.ends_with("</body></html>"), "{written}");
            }
        }
    }

    /// The head says its policy first and links the theme's sheets in their
    /// order, then the interface's, and links nothing else: no other
    /// stylesheet, no script and no sheet written inline.
    #[test]
    fn the_head_links_the_themes_sheets_then_the_interfaces() {
        for theme in themes() {
            let written = document(Locale::En, theme, &footer(Locale::En));
            let head = written
                .split_once("</head>")
                .map(|(head, _)| head)
                .expect("a head");
            // The page reads `&#39;` as the quote it stands for.
            assert!(
                head.contains(&format!(
                    "<head><meta charset=\"utf-8\"/><meta http-equiv=\"Content-Security-Policy\" \
                     content=\"{}\"/><link ",
                    POLICY.replace('\'', "&#39;")
                )),
                "the policy is said before anything is linked: {head}"
            );
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
    /// the module it loads imports — is a path the custom protocol answers,
    /// relative to the document: none names a scheme or a host, and none is a
    /// path the protocol does not hold.
    #[test]
    fn every_address_is_one_the_protocol_answers() {
        for theme in themes() {
            let written = document(Locale::En, theme, &footer(Locale::En));
            let imported: Vec<&str> = ENHANCES
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
                    "./dist/roving.js",
                    "./dist/seats.js"
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
            for address in [crate::assets::SCRIPT, crate::assets::ENHANCES] {
                assert!(
                    addresses.contains(&address),
                    "{address} is not loaded: {written}"
                );
            }
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

    /// The policy lets the page load a script, a sheet and a font from its
    /// own origin alone, an image from it or written inline, and nothing
    /// anywhere else; it connects, frames, submits and takes a base nowhere,
    /// and refuses every other kind of load by its default — so no inline
    /// script runs, no `eval`, and no address off the protocol is reached.
    #[test]
    fn the_policy_refuses_every_load_off_the_protocol() {
        let directives: Vec<(&str, Vec<&str>)> = POLICY
            .split(';')
            .map(|directive| {
                let mut words = directive.split_whitespace();
                (words.next().unwrap_or(""), words.collect())
            })
            .collect();
        assert_eq!(
            directives,
            [
                ("default-src", vec!["'none'"]),
                ("script-src", vec!["'self'"]),
                ("style-src", vec!["'self'"]),
                ("img-src", vec!["'self'", "data:"]),
                ("font-src", vec!["'self'"]),
                ("connect-src", vec!["'none'"]),
                ("frame-src", vec!["'none'"]),
                ("base-uri", vec!["'none'"]),
                ("form-action", vec!["'none'"]),
            ]
        );
        for loose in ["unsafe", "*", "http", "blob:", "filesystem:"] {
            assert!(!POLICY.contains(loose), "the policy admits {loose}");
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
                .split_once("<body class=\"m-auto h-screen overflow-hidden\">")
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
            "if (region) say('focus ' + region);".to_owned(),
            "if (!active || active === document.body) say('focus workspace');".to_owned(),
            "if (drawing) return;".to_owned(),
            "const regionOf = (element) => element.closest('#noctua-drawer') ? 'drawer'\n    \
             : element.closest('#noctua-sidebar') ? 'sidebar'\n      \
             : element.closest('footer .nd-footer-line[data-nd-kind=prompt]') ? 'prompt'\n        \
             : element.closest('footer .nd-footer-line[data-nd-kind=info]') ? 'info'\n          \
             : element.closest('footer .nd-footer-line[data-nd-kind=modeline]') ? 'modeline'\n            \
             : element.closest('#noctua-workspace') ? 'workspace' : null;"
                .to_owned(),
            "if (where === 'workspace') {\n        \
             if (active && active !== document.body && !active.closest('#noctua-workspace')) \
             active.blur();"
                .to_owned(),
            "const flags = (event) => [event.ctrlKey, event.altKey, event.shiftKey, \
             event.metaKey || superDown];"
                .to_owned(),
            "const SUPER = ['MetaLeft', 'MetaRight', 'OSLeft', 'OSRight'];".to_owned(),
            "if (SUPER.includes(event.code)) superDown = true;\n    downs = flags(event);".to_owned(),
            "if (SUPER.includes(event.code)) superDown = false;\n    downs = flags(event);".to_owned(),
            "if (!document.getElementById(holder.getAttribute('aria-activedescendant'))) \
             holder.removeAttribute('aria-activedescendant');"
                .to_owned(),
            "say('choose pages ' + tab.getAttribute('data-nd-value'));".to_owned(),
            "window.addEventListener('keydown', (event) => {".to_owned(),
            "  }, true);\n  window.addEventListener('keyup'".to_owned(),
            "say(target === null ? 'hover' : 'hover ' + target);".to_owned(),
            "document.addEventListener('pointerover', (event) => {".to_owned(),
            "document.documentElement.addEventListener('pointerleave', () => hover(null));"
                .to_owned(),
            "if (target === hovered) return;".to_owned(),
            "say(['seen', window.innerWidth, window.innerHeight, bit(fits), bit(lines), \
             bit(reachable), bit(styled), keyboard,"
                .to_owned(),
            "if ((box.top < -0.5 || box.bottom > height + 0.5) && !clipped(element, false)) \
             fits = false;"
                .to_owned(),
            "const lines = Array.from(root.querySelectorAll('footer .nd-footer-line'))\
             .filter(rendered).every(whole);"
                .to_owned(),
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
            ("option-list.js", "export function enhanceOptionLists("),
            ("option-list.js", "export function releaseOptionLists("),
            ("prompt.js", "export function enhancePrompts("),
        ] {
            assert!(
                noctua_design_dist::FILES
                    .iter()
                    .any(|(name, contents)| *name == shipped && contents.contains(exported)),
                "the design system ships {exported} in {shipped}, which the module imports"
            );
        }
    }

    /// The script keeps the keyboard where the state says.
    ///
    /// A draw that takes away the element the keyboard is on, or leaves it
    /// unable to take it, puts it on the own element of the part it was in,
    /// where that part still holds one that can take it; a part `focus`
    /// names that cannot take it is answered with where the keyboard is; and
    /// the witness says a keyboard on an element that cannot take it as on
    /// nothing, and holds every box that scrolls to being a stop of the
    /// keyboard or holding one. What each does in the system's webview is
    /// held by `apps/desktop/tests/the_window_follows_its_daemon.rs`, the
    /// window suite.
    #[test]
    fn the_script_keeps_the_keyboard_where_the_state_says() {
        for written in [
            "const held = before && before !== document.body && region.contains(before) ? \
             regionOf(before) : null;",
            "if (held && (!after || !keeps(after))) {\n        \
             const part = partOf(held);\n        \
             if (keeps(part)) part.focus();",
            "const takes = (element) => element !== null && !element.closest('[inert], [hidden]');",
            "const TAKERS = '[tabindex], a[href], button:enabled, input:enabled, \
             select:enabled, textarea:enabled';",
            "const keeps = (element) => takes(element) && element.matches(TAKERS) && \
             element.getClientRects().length > 0;",
            "const keyboard = !active || !keeps(active) ? '-' : placeOf(active);",
            "if (!takes(element)) {\n        say('focus ' + keyboardAt());\n        return;\n      }",
            "return active && active !== document.body ? regionOf(active) || 'workspace' : \
             'workspace';",
            "if (scrolls && !stops(element)) reachable = false;",
            "one.tabIndex >= 0 && rendered(one)",
        ] {
            assert!(SCRIPT.contains(written), "{written} is not in {SCRIPT}");
        }
    }

    /// The script writes every seat the window names through the design
    /// system's `seats.js`, which the module imports and hands it — a seat's
    /// hover and drag on the element whose action or value is the word
    /// named, its keyboard on what its selection names inside its part, or
    /// on that part's own element — and takes every other seat off the page.
    #[test]
    fn the_script_writes_the_seats_the_window_names() {
        for written in [
            "seat.hover(one.seat, named(one.hover));",
            "seat.focus(one.seat, named(one.selection, within(one.part)) || partOf(one.part));",
            "seat.drag(one.seat, named(one.drag));",
            "if (id && !kept.has(id)) seat.clear(id);",
            "if (element.getAttribute('data-nd-action') === word || \
             element.getAttribute('data-nd-value') === word) return element;",
            "const element = partOf(where);",
        ] {
            assert!(SCRIPT.contains(written), "{written} is not in {SCRIPT}");
        }
        assert!(
            ENHANCES.contains(
                "import { seatHover, seatFocus, seatDrag, clearSeat } from \"./dist/seats.js\";"
            ) && ENHANCES.contains(
                "window.noctua.seat = { hover: seatHover, focus: seatFocus, drag: seatDrag, \
                 clear: clearSeat };"
            ),
            "{ENHANCES}"
        );
        for exported in [
            "export function seatHover(",
            "export function seatFocus(",
            "export function seatDrag(",
            "export function clearSeat(",
        ] {
            assert!(
                noctua_design_dist::FILES
                    .iter()
                    .any(|(name, contents)| *name == "seats.js" && contents.contains(exported)),
                "the design system ships {exported} in seats.js, which the module imports"
            );
        }
    }
}
