//! What the CLI prints, and in which language (`NOCTUA.md §20.2`, `§10.9`).
//!
//! **Two renderings of one outcome.** A person is shown the answer on standard
//! output and a refusal on standard error, its catalog code first and its
//! message in the environment's language, then the English detail beside it.
//! With `--json`, standard output is **one canonical document** — `{"ok": …}`
//! or `{"error": {"code", "detail", "exit", "message"}}` — and nothing else is
//! written, so a program reads every outcome the same way.
//!
//! **The language is the environment's**: the first of `LC_ALL`,
//! `LC_MESSAGES` and `LANG` that is set and not empty, as the C library reads
//! them; one starting `pt` is Brazilian Portuguese, and any other, or none, is
//! English. The CLI's own fixed text is English (`§2.1`); a catalog message and
//! a catalog command's title and description are in that language.
//!
//! **Every text that came from the daemon is escaped for the terminal**
//! ([`escaped`]): a plugin's title, a refusal's sentence, a result, a word a
//! person typed and a refusal repeats — each character a reader sees nothing
//! of, the controls that move a terminal's cursor and the line breaks among
//! them, is written as `\u{…}`, so no text forges a line, clears a screen or
//! turns the rest of a line around.
//!
//! # What this does not answer
//!
//! The `--json` document is the one canonical text of its value, which writes
//! the C0 controls as escapes and every other character as itself: a program
//! reading it parses it, and a person printing it raw to a terminal prints
//! what the daemon sent.

use core::fmt::Write as _;

use nc_canon::Value;
use nc_catalog::{ErrorCode, Locale};

use crate::exit::Exit;

/// The variables a language is read from, in the order they are asked.
pub const LOCALE_VARIABLES: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];

/// What the CLI reads of its environment: the language it answers in, and
/// nothing else — never a location, which is the install's (`§17.1`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// The value of each of [`LOCALE_VARIABLES`], in that order.
    locale: [Option<String>; 3],
}

impl Env {
    /// The environment `variable` answers for, asked of each of
    /// [`LOCALE_VARIABLES`].
    #[must_use]
    pub fn of(variable: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            locale: LOCALE_VARIABLES.map(&variable),
        }
    }

    /// The language the CLI answers in: Brazilian Portuguese for the first
    /// variable set and not empty starting `pt`, English otherwise.
    #[must_use]
    pub fn locale(&self) -> Locale {
        match self.locale.iter().flatten().find(|value| !value.is_empty()) {
            Some(value) if value.starts_with("pt") => Locale::PtBR,
            _ => Locale::En,
        }
    }
}

/// How a run ended: its outcome, and what it writes to each stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The outcome, whose code the process exits with.
    pub exit: Exit,
    /// What it writes to standard output.
    pub stdout: String,
    /// What it writes to standard error.
    pub stderr: String,
}

impl Outcome {
    /// The code the process exits with.
    #[must_use]
    pub const fn code(&self) -> u8 {
        self.exit.code()
    }
}

/// A run refused: the code a log is queried by, the outcome, and what
/// happened, in English.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The catalog code.
    pub code: ErrorCode,
    /// The outcome it ends the run in.
    pub exit: Exit,
    /// What happened, in English, not yet escaped.
    pub detail: String,
}

/// What the CLI writes of anything that did not come from itself.
///
/// Every character of `text` a reader sees nothing of is written as `\u{…}`,
/// and the rest as it is.
#[must_use]
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if nc_schema::unseen(character) {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(character));
        } else {
            out.push(character);
        }
    }
    out
}

/// `said` without the `[<code>] ` an error's own text opens with, which the
/// rendering writes once, in front.
#[must_use]
pub fn without_code(said: &str, code: ErrorCode) -> String {
    said.strip_prefix(&format!("[{}] ", code.code()))
        .unwrap_or(said)
        .to_owned()
}

/// The one canonical document `--json` writes, and its line's end.
fn document(value: &Value) -> String {
    let mut text = value.to_canonical();
    text.push('\n');
    text
}

/// A command's answer.
#[must_use]
pub fn answered(json: bool, payload: &Value) -> Outcome {
    let stdout = if json {
        document(&Value::object([("ok", payload.clone())]))
    } else {
        let mut text = escaped(&payload.to_canonical());
        text.push('\n');
        text
    };
    Outcome {
        exit: Exit::Success,
        stdout,
        stderr: String::new(),
    }
}

/// Text the CLI shows a person — help — which every daemon text in it was
/// escaped into already.
#[must_use]
pub fn shown(json: bool, text: String) -> Outcome {
    let stdout = if json {
        document(&Value::object([("ok", Value::text(text))]))
    } else {
        text
    };
    Outcome {
        exit: Exit::Success,
        stdout,
        stderr: String::new(),
    }
}

/// The catalog's message for `code` in `locale`, where it needs no value the
/// refusal did not carry.
///
/// **None for a message with parameters**, which a refusal carries no values
/// for: it is shown by its detail alone rather than with its braces showing.
fn message_of(code: ErrorCode, locale: Locale) -> Option<String> {
    nc_catalog::render(code.message_key(), locale, &[]).and_then(Result::ok)
}

/// A run refused, as `locale` reads it.
#[must_use]
pub fn failed(json: bool, locale: Locale, failure: &Failure) -> Outcome {
    let detail = escaped(&failure.detail);
    let message = message_of(failure.code, locale);
    if json {
        let message = message.unwrap_or_else(|| detail.clone());
        let error = Value::object([
            ("code", Value::text(failure.code.code())),
            ("detail", Value::text(failure.detail.as_str())),
            ("exit", Value::Integer(i64::from(failure.exit.code()))),
            ("message", Value::text(message)),
        ]);
        return Outcome {
            exit: failure.exit,
            stdout: document(&Value::object([("error", error)])),
            stderr: String::new(),
        };
    }
    let stderr = match message {
        Some(message) => format!("error[{}]: {message}\n  {detail}\n", failure.code.code()),
        None => format!("error[{}]: {detail}\n", failure.code.code()),
    };
    Outcome {
        exit: failure.exit,
        stdout: String::new(),
        stderr,
    }
}

#[cfg(test)]
mod tests {
    use nc_canon::Value;
    use nc_catalog::{ErrorCode, Locale};

    use super::{Env, Failure};
    use crate::exit::Exit;

    fn env(pairs: &[(&str, &str)]) -> Env {
        Env::of(|name| {
            pairs
                .iter()
                .find(|(variable, _)| *variable == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    /// The language is the first variable set and not empty.
    ///
    /// Of `LC_ALL`, `LC_MESSAGES` and `LANG`, in that order: Portuguese when it
    /// starts `pt`, and English for any other or none.
    #[test]
    fn the_language_is_the_first_variable_set_and_pt_is_portuguese() {
        for (pairs, locale) in [
            (&[][..], Locale::En),
            (&[("LANG", "pt_BR.UTF-8")], Locale::PtBR),
            (&[("LANG", "pt")], Locale::PtBR),
            (&[("LANG", "en_US.UTF-8")], Locale::En),
            (&[("LANG", "C")], Locale::En),
            (&[("LC_MESSAGES", "pt_PT"), ("LANG", "en_US")], Locale::PtBR),
            (
                &[
                    ("LC_ALL", "en_GB"),
                    ("LC_MESSAGES", "pt_BR"),
                    ("LANG", "pt_BR"),
                ],
                Locale::En,
            ),
            (&[("LC_ALL", ""), ("LC_MESSAGES", "pt_BR")], Locale::PtBR),
            (
                &[("LC_ALL", ""), ("LC_MESSAGES", ""), ("LANG", "pt_BR")],
                Locale::PtBR,
            ),
            (&[("LC_ALL", "pt_BR"), ("LANG", "en_US")], Locale::PtBR),
            (&[("LANGUAGE", "pt_BR")], Locale::En),
        ] {
            assert_eq!(env(pairs).locale(), locale, "{pairs:?}");
        }
    }

    /// Every character a reader sees nothing of is escaped.
    ///
    /// From both tables, to each end of each range; a reader's ordinary text,
    /// accents and all, is left as it is.
    #[test]
    fn every_character_a_reader_sees_nothing_of_is_escaped() {
        for (first, last) in nc_schema::INVISIBLE_RANGES
            .iter()
            .chain(nc_schema::BLANK_RANGES.iter())
        {
            for point in [*first, *last] {
                let Some(hidden) = char::from_u32(point) else {
                    continue;
                };
                let shown = super::escaped(&format!("a{hidden}b"));
                assert_eq!(shown, format!("a\\u{{{point:x}}}b"));
            }
        }
        for plain in ["Verificar histórico", "noctua — ok", "日本語", "a\\b", ""] {
            assert_eq!(super::escaped(plain), plain);
        }
        assert_eq!(super::escaped("\u{1b}[2J"), "\\u{1b}[2J");
        assert_eq!(super::escaped("one\ntwo"), "one\\u{a}two");
    }

    /// With `--json`, standard output is one canonical document and nothing
    /// is written beside it, for an answer, for text and for a refusal.
    #[test]
    fn json_is_one_canonical_document_on_standard_output() {
        let failure = Failure {
            code: ErrorCode::KernelUnconfirmed,
            exit: Exit::NeedsTheInterface,
            detail: "needs \u{1b} you".to_owned(),
        };
        for (outcome, exit) in [
            (super::answered(true, &Value::text("é\u{7}")), Exit::Success),
            (super::shown(true, "help\n".to_owned()), Exit::Success),
            (
                super::failed(true, Locale::PtBR, &failure),
                Exit::NeedsTheInterface,
            ),
        ] {
            assert_eq!(outcome.exit, exit);
            assert!(outcome.stderr.is_empty(), "{outcome:?}");
            let text = outcome
                .stdout
                .strip_suffix('\n')
                .expect("the document ends its line");
            let value = nc_canon::parse(text).expect("one JSON value");
            assert_eq!(value.to_canonical(), text, "not the canonical text");
            assert!(!text.contains('\n'), "{text}");
        }
        let refused = super::failed(true, Locale::PtBR, &failure);
        let value = nc_canon::parse(refused.stdout.trim_end()).expect("parsed");
        let expected = Value::object([(
            "error",
            Value::object([
                ("code", Value::text(ErrorCode::KernelUnconfirmed.code())),
                ("detail", Value::text("needs \u{1b} you")),
                ("exit", Value::Integer(4)),
                (
                    "message",
                    Value::text(
                        nc_catalog::message("error.kernel.unconfirmed", Locale::PtBR)
                            .expect("a message"),
                    ),
                ),
            ]),
        )]);
        assert_eq!(value, expected);
    }

    /// A refusal is shown by its code and its message in the environment's
    /// language, with its English detail escaped beside it; one whose message
    /// needs values it does not carry is shown by its detail alone.
    #[test]
    fn a_refusal_is_its_code_its_message_and_its_detail_escaped() {
        let failure = Failure {
            code: ErrorCode::KernelUnconfirmed,
            exit: Exit::NeedsTheInterface,
            detail: "the step\u{202e} needs a person".to_owned(),
        };
        for locale in [Locale::En, Locale::PtBR] {
            let outcome = super::failed(false, locale, &failure);
            assert_eq!(outcome.exit, Exit::NeedsTheInterface);
            assert_eq!(outcome.stdout, "");
            let message =
                nc_catalog::message("error.kernel.unconfirmed", locale).expect("a message");
            assert_eq!(
                outcome.stderr,
                format!(
                    "error[{}]: {message}\n  the step\\u{{202e}} needs a person\n",
                    ErrorCode::KernelUnconfirmed.code()
                )
            );
        }
        let templated = super::failed(
            false,
            Locale::En,
            &Failure {
                code: ErrorCode::ValidationOutOfRange,
                exit: Exit::Invalid,
                detail: "`from` is below 0".to_owned(),
            },
        );
        assert_eq!(
            templated.stderr,
            format!(
                "error[{}]: `from` is below 0\n",
                ErrorCode::ValidationOutOfRange.code()
            )
        );
        assert!(
            !templated.stderr.contains('{'),
            "a template's braces were shown"
        );
    }

    /// An answer is shown as its canonical text, escaped, for a person.
    #[test]
    fn an_answer_is_its_canonical_text_escaped() {
        let outcome = super::answered(false, &Value::object([("a", Value::text("\u{1b}x"))]));
        assert_eq!(outcome.stdout, "{\"a\":\"\\u001bx\"}\n");
        let outcome = super::answered(false, &Value::text("ok\u{202e}"));
        assert_eq!(outcome.stdout, "\"ok\\u{202e}\"\n");
        assert_eq!(outcome.code(), 0);
    }

    /// The code an error's own text opens with is taken off once, and text
    /// that does not open with it is left whole.
    #[test]
    fn the_code_in_front_of_an_errors_text_is_taken_off_once() {
        let code = ErrorCode::IpcUnreachable;
        let other = ErrorCode::IpcFrameTooLarge;
        assert_eq!(
            super::without_code(&format!("[{}] nobody", code.code()), code),
            "nobody"
        );
        assert_eq!(super::without_code("nobody", code), "nobody");
        assert_eq!(
            super::without_code(&format!("[{}] nobody", other.code()), code),
            format!("[{}] nobody", other.code())
        );
    }
}
