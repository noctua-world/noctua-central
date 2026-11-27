//! The theme the interface is painted in, decided before its first paint
//! (`NOCTUA.md §9.6`, §8.4, §12.10).
//!
//! **A theme is a mode and a palette**, the two halves of the colour
//! system's compound selector (`[data-palette="…"][data-theme="…"]`), so both
//! are always stamped on the document's root, together: the palette —
//! `<accent>-<saturation>` (§8.4) — and the mode, light or dark. A person's
//! choice of either is the appearance setting's, which this build does not
//! hold yet (§12.15), so the design system's defaults are taken: its default
//! palette, and the mode the system prefers ([`Theme::chosen`]).
//!
//! **The system's mode is resolved here, and stamped as the one it resolved
//! to**, never left to the stylesheet's `prefers-color-scheme`: the window
//! and the webview are told the same mode the document is stamped with, so
//! what the system draws around and beneath the page — before the page has
//! painted, and where it has not — is the page's own mode, and no two parts
//! of the window can read the system's preference differently.
//!
//! **What the document links is the design system's own list**, in its
//! order (the `base.html` of `noctua-design-dist`, whose order is
//! load-bearing), with the chosen palette's sheet after it where the list
//! does not already hold it — as the design system's own bootstrap appends
//! it — so the document is complete when its head is read and no script
//! changes its colours afterwards (§9.6, lessons 1, 2 and 4).
//!
//! **What the webview paints where the document has not** is the page's own
//! background where the design system gives it as data ([`Theme::backdrop`]):
//! in the dark, the palette's dark surface, read from the palette's own file
//! of values. In the light the design system's page is white paper, which it
//! writes as a literal in its sheet and as no value a consumer can read, and
//! which is the webview's own background, so nothing is set.

use noctua_design::model::{Mode, Palette};

/// The theme a document is built in: a palette, and a mode that is light or
/// dark — the system's having been resolved to one of the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Light or dark, never the system's.
    mode: Mode,
    /// The palette.
    palette: Palette,
}

/// The token the design system's dark page is painted with, through
/// `--noctua-backdrop` and `--nc-color-surface`, as its palettes' values name
/// it.
const DARK_SURFACE: &str = "--nc-neutral-bg-app";

impl Theme {
    /// The theme `mode` and `palette` ask for, the system's mode resolved to
    /// dark when `system_is_dark` and to light otherwise.
    #[must_use]
    pub const fn chosen(mode: Mode, palette: Palette, system_is_dark: bool) -> Self {
        let mode = match mode {
            Mode::System if system_is_dark => Mode::Dark,
            Mode::System => Mode::Light,
            chosen => chosen,
        };
        Self { mode, palette }
    }

    /// Light or dark.
    #[must_use]
    pub const fn mode(self) -> Mode {
        self.mode
    }

    /// The palette.
    #[must_use]
    pub const fn palette(self) -> Palette {
        self.palette
    }

    /// Whether the mode is dark.
    #[must_use]
    pub const fn is_dark(self) -> bool {
        matches!(self.mode, Mode::Dark)
    }

    /// The attributes the document's root carries, as `(name, value)`: the
    /// mode and the palette, both always.
    #[must_use]
    pub const fn attributes(self) -> [(&'static str, &'static str); 2] {
        [
            (Mode::ATTRIBUTE, self.mode.as_str()),
            (Palette::ATTRIBUTE, self.palette.as_str()),
        ]
    }

    /// Every stylesheet of the design system the document links, by its name
    /// in `noctua-design-dist`, in the order it links them: the design
    /// system's list, then the palette's own sheet where the list does not
    /// hold it.
    #[must_use]
    // allow-logging: `sheets` hands back names of files the design system ships, compiled into this binary
    pub fn sheets(self) -> Vec<&'static str> {
        let mut sheets = base_sheets();
        let own = noctua_design::model::stylesheet_href(self.palette);
        if !sheets.contains(&own) {
            sheets.push(own);
        }
        sheets
    }

    /// What the webview paints before the document has, as red, green and
    /// blue: the palette's dark surface in the dark, and nothing in the
    /// light, whose page is the webview's own white. Nothing as well for a
    /// palette whose values the design system does not ship.
    #[must_use]
    pub fn backdrop(self) -> Option<[u8; 3]> {
        if !self.is_dark() {
            return None;
        }
        let name = format!("palettes/{}.json", self.palette.as_str());
        let (_, values) = noctua_design_dist::FILES
            .iter()
            .find(|(shipped, _)| *shipped == name)?;
        dark_value(values, DARK_SURFACE)
    }
}

/// The value `token` holds in the dark, of a palette's `values` as the design
/// system writes them: `{"dark": [[<token>, "#rrggbb"], …], …}`.
fn dark_value(values: &str, token: &str) -> Option<[u8; 3]> {
    let mut fields = nc_canon::parse(values).ok()?.into_object()?;
    let pairs = fields.remove("dark")?.into_array()?;
    pairs.into_iter().find_map(|pair| {
        let [name, value] = <[nc_canon::Value; 2]>::try_from(pair.into_array()?).ok()?;
        if name.into_text()? != token {
            return None;
        }
        rgb(&value.into_text()?)
    })
}

/// `#rrggbb`, in lower case as the design system writes it, as red, green
/// and blue; nothing for any other text.
fn rgb(hex: &str) -> Option<[u8; 3]> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let bytes = nc_canon::from_hex(digits).ok()?;
    <[u8; 3]>::try_from(bytes).ok()
}

/// The stylesheets the design system's own list links, in its order: every
/// `href` of a `<link rel="stylesheet">` of its `base.html`.
// allow-logging: `base_sheets` hands back names read from a file compiled into this binary
fn base_sheets() -> Vec<&'static str> {
    noctua_design_dist::FILES
        .iter()
        .find(|(name, _)| *name == "base.html")
        .map(|(_, list)| linked(list))
        .unwrap_or_default()
}

/// Every `href` a line of `list` opening `<link rel="stylesheet" href="…"`
/// names, in order.
fn linked(list: &str) -> Vec<&str> {
    list.lines()
        .filter_map(|line| line.trim().strip_prefix("<link rel=\"stylesheet\" href=\""))
        .filter_map(|rest| rest.split_once('"').map(|(href, _)| href))
        .collect()
}

#[cfg(test)]
mod tests {
    use noctua_design::model::{Mode, Palette};

    use super::{Theme, base_sheets, dark_value, linked, rgb};

    /// The system's mode is resolved to the one the system prefers, and a
    /// mode chosen is kept whatever the system prefers.
    #[test]
    fn the_systems_mode_is_resolved_and_a_chosen_one_is_kept() {
        for (mode, dark, resolved) in [
            (Mode::System, true, Mode::Dark),
            (Mode::System, false, Mode::Light),
            (Mode::Light, true, Mode::Light),
            (Mode::Light, false, Mode::Light),
            (Mode::Dark, true, Mode::Dark),
            (Mode::Dark, false, Mode::Dark),
        ] {
            let theme = Theme::chosen(mode, Palette::DEFAULT, dark);
            assert_eq!(theme.mode(), resolved, "{mode:?} {dark}");
            assert_eq!(theme.is_dark(), resolved == Mode::Dark, "{mode:?} {dark}");
            assert_eq!(theme.palette(), Palette::DEFAULT);
        }
    }

    /// The root carries the mode and the palette, both always, under the
    /// design system's own attribute names.
    #[test]
    fn the_root_carries_the_mode_and_the_palette() {
        let theme = Theme::chosen(Mode::Dark, Palette::JadeVivid, false);
        assert_eq!(
            theme.attributes(),
            [("data-theme", "dark"), ("data-palette", "jade-vivid")]
        );
        let theme = Theme::chosen(Mode::System, Palette::DEFAULT, false);
        assert_eq!(
            theme.attributes(),
            [("data-theme", "light"), ("data-palette", "blue-balanced")]
        );
    }

    /// The design system's list is read whole and in its order, and every
    /// sheet in it is one the design system ships.
    #[test]
    fn the_design_systems_list_is_read_in_its_order() {
        let sheets = base_sheets();
        let list = noctua_design_dist::FILES
            .iter()
            .find(|(name, _)| *name == "base.html")
            .map(|(_, list)| *list)
            .expect("the design system ships its list");
        assert_eq!(
            sheets.len(),
            list.matches("<link rel=\"stylesheet\"").count(),
            "every link is read: {sheets:?}"
        );
        assert!(sheets.len() > 1, "{sheets:?}");
        for sheet in &sheets {
            assert!(
                noctua_design_dist::FILES
                    .iter()
                    .any(|(name, _)| name == sheet),
                "{sheet} is not shipped"
            );
        }
        let mut positions = sheets.iter().map(|sheet| {
            list.find(&format!("href=\"{sheet}\""))
                .expect("in the list")
        });
        let mut before = positions.next().expect("one at least");
        for after in positions {
            assert!(before < after, "out of order: {sheets:?}");
            before = after;
        }
    }

    /// A line is read for its `href` only when it opens a stylesheet link.
    #[test]
    fn only_a_stylesheets_link_is_read() {
        let list = "<!-- <link rel=\"stylesheet\" href=\"no.css\"> -->\n\
                    <link rel=\"stylesheet\" href=\"a.css\">\n\
                    <link rel=\"preload\" href=\"b.css\">\n  \
                    <link rel=\"stylesheet\" href=\"c.css\" id=\"x\">\n\
                    <link rel=\"stylesheet\" href=\"d.css";
        assert_eq!(linked(list), ["a.css", "c.css"]);
    }

    /// The palette's own sheet follows the list where the list does not
    /// hold it, and is not linked twice where it does.
    #[test]
    fn the_palettes_sheet_follows_the_list_once() {
        let base = base_sheets();
        for palette in Palette::ALL {
            let own = noctua_design::model::stylesheet_href(palette);
            let sheets = Theme::chosen(Mode::Light, palette, false).sheets();
            assert_eq!(
                sheets.iter().filter(|sheet| **sheet == own).count(),
                1,
                "{palette:?}: {sheets:?}"
            );
            assert_eq!(&sheets[..base.len()], &base[..], "{palette:?}");
            if base.contains(&own) {
                assert_eq!(sheets.len(), base.len(), "{palette:?}");
            } else {
                assert_eq!(sheets.last(), Some(&own), "{palette:?}");
                assert_eq!(sheets.len(), base.len() + 1, "{palette:?}");
            }
        }
    }

    /// In the dark, every palette's backdrop is its dark surface's value;
    /// in the light, there is none.
    #[test]
    fn the_backdrop_is_the_dark_surface_and_nothing_in_the_light() {
        for palette in Palette::ALL {
            let name = format!("palettes/{}.json", palette.as_str());
            let (_, values) = noctua_design_dist::FILES
                .iter()
                .find(|(shipped, _)| *shipped == name)
                .expect("the design system ships every palette's values");
            let surface = dark_value(values, "--nc-neutral-bg-app").expect("a dark surface");
            assert_eq!(
                Theme::chosen(Mode::Dark, palette, false).backdrop(),
                Some(surface),
                "{palette:?}"
            );
            assert_eq!(
                Theme::chosen(Mode::Light, palette, true).backdrop(),
                None,
                "{palette:?}"
            );
        }
    }

    /// A value is read from the dark list alone, by its exact token.
    #[test]
    fn a_value_is_the_dark_lists_by_its_token() {
        let values = r##"{"light": [["--t", "#010203"]], "dark": [["--u", "#0a0b0c"], ["--t", "#a0b0c0"], ["--t-x", "#ffffff"]]}"##;
        assert_eq!(dark_value(values, "--t"), Some([0xa0, 0xb0, 0xc0]));
        assert_eq!(dark_value(values, "--u"), Some([0x0a, 0x0b, 0x0c]));
        assert_eq!(dark_value(values, "--v"), None);
        assert_eq!(
            dark_value(r##"{"light": [["--t", "#010203"]]}"##, "--t"),
            None
        );
        assert_eq!(dark_value("not json", "--t"), None);
        assert_eq!(dark_value(r#"{"dark": [["--t"]]}"#, "--t"), None);
    }

    /// Only `#rrggbb`, in lower case, is a colour.
    #[test]
    fn only_six_hex_digits_are_a_colour() {
        assert_eq!(rgb("#00ff7f"), Some([0, 255, 127]));
        for other in [
            "00ff7f",
            "#0f7",
            "#00ff7f00",
            "#00ff7g",
            "# 0ff7f",
            "#00FF7F",
            "",
            "#",
        ] {
            assert_eq!(rgb(other), None, "{other}");
        }
    }
}
