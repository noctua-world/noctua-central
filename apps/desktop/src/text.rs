//! The interface's own texts, said in the window's locale
//! (`NOCTUA.md §12`, `§10.9`).
//!
//! **Every word the window says that is no command's and no error's comes
//! from one registry**, `catalog/interface.toml`, by its generated handle
//! (`nc_catalog::interface::Text`): the footer's name, the channel's word,
//! the line that names a compositor's binding. A text written where it is
//! drawn would exist in one language and in no page of the documentation; a
//! handle exists in every locale or the catalog does not compile, and a text
//! taken out of the registry stops this crate compiling.
//!
//! A control is still named by its command's display name
//! (`crate::commands::title_of`) and a refusal said by its code's message
//! (`crate::host::refused_said`): those are their own registries'.

use nc_catalog::Locale;
use nc_catalog::interface::Text;

/// What `text` says in `locale`, a text that interpolates nothing.
///
/// The catalog holds every text in every locale, so the fallback — the
/// text's id — is what a build made past its own gate would show.
#[must_use]
pub fn said(text: Text, locale: Locale) -> String {
    nc_catalog::message(text.key(), locale)
        .unwrap_or_else(|| text.id())
        .to_owned()
}

/// What `text` says in `locale` with each placeholder filled from `values`,
/// which name every parameter the registry declares for it.
///
/// A value the sentence asks for and `values` does not hold leaves the text
/// said by its id, never a sentence with braces in it.
#[must_use]
pub fn said_with(text: Text, locale: Locale, values: &[(&str, &str)]) -> String {
    nc_catalog::render(text.key(), locale, values)
        .and_then(Result::ok)
        .unwrap_or_else(|| text.id().to_owned())
}

#[cfg(test)]
mod tests {
    use nc_catalog::Locale;
    use nc_catalog::interface::Text;

    use super::{said, said_with};

    /// Every text is said in every locale: one that takes nothing by
    /// [`said`], and one that takes parameters by [`said_with`] given a value
    /// for each, with no brace left in it and every value in it.
    #[test]
    fn every_text_is_said_in_every_locale_with_its_parameters_filled() {
        for text in Text::ALL {
            for locale in Locale::ALL {
                let values: Vec<(&str, String)> = text
                    .params()
                    .iter()
                    .map(|name| (*name, format!("<{name}>")))
                    .collect();
                let borrowed: Vec<(&str, &str)> = values
                    .iter()
                    .map(|(name, value)| (*name, value.as_str()))
                    .collect();
                let filled = said_with(*text, *locale, &borrowed);
                assert!(!filled.is_empty() && filled != text.id(), "{text:?}");
                assert!(!filled.contains(['{', '}']), "{text:?}: {filled}");
                for (_, value) in &values {
                    assert!(filled.contains(value.as_str()), "{text:?}: {filled}");
                }
                if text.params().is_empty() {
                    assert_eq!(said(*text, *locale), filled, "{text:?}");
                }
            }
        }
    }

    /// A text is said in the locale asked, and one whose parameter was not
    /// given is said by its id rather than with a brace in it.
    #[test]
    fn a_text_is_said_in_the_locale_asked_and_never_with_a_brace_left() {
        assert_eq!(said(Text::PartFooter, Locale::En), "Footer");
        assert_eq!(said(Text::PartFooter, Locale::PtBR), "Rodapé");
        assert_eq!(
            said_with(Text::SeatNumbered, Locale::PtBR, &[("seat", "2")]),
            "Posto 2"
        );
        assert_eq!(
            said_with(Text::SeatNumbered, Locale::En, &[]),
            Text::SeatNumbered.id()
        );
    }
}
