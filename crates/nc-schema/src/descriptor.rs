//! A command's declaration, judged whole.
//!
//! `NOCTUA.md §11.7` says a descriptor missing a field is rejected at
//! `activate`, naming the command, and the catalog compiler refuses the same
//! things at build time. [`judge`] is that rule, written once: it reports
//! **every** problem it finds rather than the first, because a person fixing a
//! declaration should not learn its defects one build at a time.
//!
//! A field is missing when it holds nothing but whitespace. What a caller
//! counts as nothing beyond that is its own business — the catalog compiler also
//! treats a value made of invisible characters as missing, and hands it here
//! as the empty string.
//!
//! # What this answers (`§2.4`)
//!
//! **A command nothing can authorize is never registered.** No action or no
//! selector means a request the decision point cannot build; a selector the
//! schema does not back means one it would build from nothing.
//!
//! **A read that mutates is refused.** `mutating` and `reversibility` say one
//! thing twice — the WIT carries the first and `§11.7` added the second — and a
//! command that is a read to one reader and a write to another passes wherever
//! only reads may.
//!
//! **A title shows everything it holds.** It is the one field a person reads
//! before running a command, and the name beside it is held to ASCII, so a
//! direction override in it would show one command under another's words. It
//! is held to [`crate::unseen`], as the argument schema is.
//!
//! **What this does not answer**: whether the action is one the policy schema
//! declares, and whether a namespace belongs to whoever declares it. Both need
//! the decision point and the plugin's manifest, which this crate never sees.

use core::fmt;

use crate::selector::{self, Selector, SelectorError};
use crate::{Schema, SchemaError, unseen};

/// The longest command name, in characters.
pub const NAME_MAX: usize = 96;

/// The longest title, in characters.
pub const TITLE_MAX: usize = 128;

/// Whether a command can be undone, and whether a human must confirm it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reversibility {
    /// Changes nothing.
    Read,
    /// Changes state.
    Write,
    /// Cannot be reversed, so a human confirms it.
    Irreversible,
}

/// A command as its author declared it, field by field.
#[derive(Debug, Clone, Copy)]
pub struct Declared<'a> {
    /// Dotted, lower-case, namespaced: `journal.verify`.
    pub name: &'a str,
    /// The label a person reads, or the message key that resolves to one.
    pub title: &'a str,
    /// The argument schema's text.
    pub args_schema: &'a str,
    /// The Cedar action the decision point evaluates.
    pub action: &'a str,
    /// `realm`, or `argument:<name>`.
    pub resource_selector: &'a str,
    /// Whether it changes state at all.
    pub mutating: bool,
    /// Read, write or irreversible.
    pub reversibility: Reversibility,
}

/// A field a declaration can leave empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The command's name.
    Name,
    /// Its title.
    Title,
    /// Its argument schema.
    ArgsSchema,
    /// Its action.
    Action,
    /// Its resource selector.
    ResourceSelector,
}

impl fmt::Display for Field {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Name => "name",
            Self::Title => "title",
            Self::ArgsSchema => "argument schema",
            Self::Action => "action",
            Self::ResourceSelector => "resource selector",
        })
    }
}

/// One thing wrong with a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptorProblem {
    /// A field that holds nothing.
    Missing(Field),
    /// A name outside `[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+`, or past [`NAME_MAX`].
    NameMalformed,
    /// A title past [`TITLE_MAX`] characters.
    TitleTooLong,
    /// A title holding a character a reader sees nothing of, which is named.
    TitleInvisibleCharacter(char),
    /// An argument schema outside the subset.
    Schema(SchemaError),
    /// A selector the grammar or the schema does not back.
    Selector(SelectorError),
    /// Declared `read` and mutating.
    ReadThatMutates,
    /// Declared `write` or `irreversible` and not mutating.
    MutationUndeclared,
}

/// The predicate a caller writes its own subject in front of.
///
/// `` `journal.verify` declares no action ``: the catalog compiler names the
/// command and its file, the kernel names the command and its plugin, and
/// neither has to strip a subject this crate chose for them.
impl fmt::Display for DescriptorProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(field) => write!(formatter, "declares no {field}"),
            Self::NameMalformed => write!(
                formatter,
                "is not named as lower-case segments joined by `.` — at least two, each \
                 `[a-z][a-z0-9-]*`, at most {NAME_MAX} characters in all"
            ),
            Self::TitleTooLong => {
                write!(formatter, "has a title longer than {TITLE_MAX} characters")
            }
            Self::TitleInvisibleCharacter(hidden) => write!(
                formatter,
                "has a title holding `U+{:04X}`, which a reader sees nothing of — and a title \
                 is what a person reads before running the command",
                u32::from(*hidden)
            ),
            Self::Schema(error) => write!(formatter, "has an argument schema refused {error}"),
            Self::Selector(error) => {
                write!(formatter, "has a resource selector refused: {error}")
            }
            Self::ReadThatMutates => formatter.write_str(
                "is declared `read` and mutating, and a read that mutates passes wherever only \
                 reads may",
            ),
            Self::MutationUndeclared => formatter.write_str(
                "changes state and declares mutating=false, and undo, the journal and the \
                 read-only frames all read `mutating`",
            ),
        }
    }
}

impl std::error::Error for DescriptorProblem {}

/// A declaration that passed, with what judging it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    /// The compiled argument schema.
    pub schema: Schema,
    /// The resource selector, resolved against it.
    pub selector: Selector,
}

/// Judges a declaration, reporting every problem found.
///
/// # Errors
///
/// Every [`DescriptorProblem`] found, in field order and never an empty list:
/// [`DescriptorProblem::Missing`], [`DescriptorProblem::NameMalformed`],
/// [`DescriptorProblem::TitleTooLong`],
/// [`DescriptorProblem::TitleInvisibleCharacter`], [`DescriptorProblem::Schema`],
/// [`DescriptorProblem::Selector`], [`DescriptorProblem::ReadThatMutates`] and
/// [`DescriptorProblem::MutationUndeclared`]. A selector is held to the schema
/// only when the schema compiled, so a broken schema is one problem rather
/// than two.
pub fn judge(declared: &Declared<'_>) -> Result<Judged, Vec<DescriptorProblem>> {
    let mut problems = Vec::new();
    if says_nothing(declared.name) {
        problems.push(DescriptorProblem::Missing(Field::Name));
    } else if !command_name_is_well_formed(declared.name) {
        problems.push(DescriptorProblem::NameMalformed);
    }
    if says_nothing(declared.title) {
        problems.push(DescriptorProblem::Missing(Field::Title));
    } else if declared.title.chars().count() > TITLE_MAX {
        problems.push(DescriptorProblem::TitleTooLong);
    } else if let Some(hidden) = declared.title.chars().find(|held| unseen(*held)) {
        problems.push(DescriptorProblem::TitleInvisibleCharacter(hidden));
    }
    let schema = if says_nothing(declared.args_schema) {
        problems.push(DescriptorProblem::Missing(Field::ArgsSchema));
        None
    } else {
        Schema::compile(declared.args_schema)
            .map_err(|error| problems.push(DescriptorProblem::Schema(error)))
            .ok()
    };
    if says_nothing(declared.action) {
        problems.push(DescriptorProblem::Missing(Field::Action));
    }
    let selector = if says_nothing(declared.resource_selector) {
        problems.push(DescriptorProblem::Missing(Field::ResourceSelector));
        None
    } else {
        match selector::written(declared.resource_selector) {
            Err(error) => {
                problems.push(DescriptorProblem::Selector(error));
                None
            }
            Ok(written) => schema.as_ref().and_then(|schema| {
                written
                    .resolved(schema)
                    .map_err(|error| problems.push(DescriptorProblem::Selector(error)))
                    .ok()
            }),
        }
    };
    match (declared.reversibility, declared.mutating) {
        (Reversibility::Read, true) => problems.push(DescriptorProblem::ReadThatMutates),
        (Reversibility::Write | Reversibility::Irreversible, false) => {
            problems.push(DescriptorProblem::MutationUndeclared);
        }
        _ => {}
    }
    match (schema, selector) {
        (Some(schema), Some(selector)) if problems.is_empty() => Ok(Judged { schema, selector }),
        _ => Err(problems),
    }
}

/// Whether a field holds nothing but whitespace.
fn says_nothing(value: &str) -> bool {
    value.trim().is_empty()
}

/// Whether `name` is a command name: dotted lower-case segments, at most
/// [`NAME_MAX`] characters.
///
/// ```text
/// name = segment 1*("." segment)
/// segment = [a-z] *[a-z0-9-]
/// ```
///
/// At least two segments, because the first is the namespace a plugin owns and
/// a name with no namespace belongs to nobody.
#[must_use]
pub fn command_name_is_well_formed(name: &str) -> bool {
    name.len() <= NAME_MAX
        && name.split('.').count() >= 2
        && name.split('.').all(|segment| {
            let mut characters = segment.chars();
            characters
                .next()
                .is_some_and(|first| first.is_ascii_lowercase())
                && characters
                    .all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '-')
        })
}

#[cfg(test)]
mod tests {

    use super::{
        Declared, DescriptorProblem, Field, NAME_MAX, Reversibility, TITLE_MAX,
        command_name_is_well_formed, judge,
    };
    use crate::{SchemaFault, Selector, SelectorError};

    /// A declaration the judge accepts, for each test to break one field of.
    fn sound() -> Declared<'static> {
        Declared {
            name: "reference.note.put",
            title: "Put a note",
            args_schema: r#"{"type":"object","additionalProperties":false,
                "properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},
                              "text":{"type":"string","maxLength":4096}},
                "required":["note","text"]}"#,
            action: "Write",
            resource_selector: "argument:note",
            mutating: true,
            reversibility: Reversibility::Write,
        }
    }

    #[test]
    fn a_sound_declaration_is_judged_with_its_schema_and_selector() {
        let judged = judge(&sound()).expect("the fixture is sound");
        assert_eq!(judged.schema.properties(), ["note", "text"]);
        assert_eq!(
            judged.selector,
            Selector::Argument {
                property: "note".to_owned(),
                entity_type: "Content".to_owned()
            }
        );
    }

    #[test]
    fn each_field_left_empty_is_named() {
        for (field, blank) in [
            (
                Field::Name,
                Declared {
                    name: "",
                    ..sound()
                },
            ),
            (
                Field::Title,
                Declared {
                    title: " ",
                    ..sound()
                },
            ),
            (
                Field::ArgsSchema,
                Declared {
                    args_schema: "\n",
                    ..sound()
                },
            ),
            (
                Field::Action,
                Declared {
                    action: "",
                    ..sound()
                },
            ),
            (
                Field::ResourceSelector,
                Declared {
                    resource_selector: "\t",
                    ..sound()
                },
            ),
        ] {
            let problems = judge(&blank).expect_err("a field is empty");
            assert!(
                problems.contains(&DescriptorProblem::Missing(field)),
                "an empty {field} was not named: {problems:?}"
            );
        }
    }

    #[test]
    fn a_missing_field_is_named_once_and_not_judged_as_well() {
        // An empty name is not also a malformed one, an empty schema is not
        // also unparsable, and an empty selector is not also outside the
        // grammar: one absence, one problem.
        let problems = judge(&Declared {
            name: "",
            args_schema: "",
            resource_selector: "",
            ..sound()
        })
        .expect_err("three fields are empty");
        assert_eq!(
            problems,
            [
                DescriptorProblem::Missing(Field::Name),
                DescriptorProblem::Missing(Field::ArgsSchema),
                DescriptorProblem::Missing(Field::ResourceSelector),
            ]
        );
    }

    #[test]
    fn a_name_outside_the_grammar_is_refused() {
        let longest = format!("a.{}", "b".repeat(NAME_MAX - 2));
        let past = format!("a.{}", "b".repeat(NAME_MAX - 1));
        for name in [
            "verify",
            "Journal.verify",
            "journal.Verify",
            "journal..verify",
            ".journal.verify",
            "journal.verify.",
            "journal.1verify",
            "journal.-verify",
            "journal_x.verify",
            "journal verify.x",
            "jöurnal.verify",
            past.as_str(),
        ] {
            assert!(!command_name_is_well_formed(name), "`{name}` was accepted");
            assert_eq!(
                judge(&Declared { name, ..sound() }),
                Err(vec![DescriptorProblem::NameMalformed]),
                "`{name}` was not refused as malformed"
            );
        }
        for name in [
            "journal.verify",
            "limits.prune",
            "a.b-2.c3",
            longest.as_str(),
        ] {
            assert!(command_name_is_well_formed(name), "`{name}` was refused");
        }
    }

    #[test]
    fn a_title_past_its_bound_is_refused() {
        let longest = "é".repeat(TITLE_MAX);
        assert!(
            judge(&Declared {
                title: &longest,
                ..sound()
            })
            .is_ok(),
            "a title of exactly the bound, counted in characters, was refused"
        );
        let past = "t".repeat(TITLE_MAX + 1);
        assert_eq!(
            judge(&Declared {
                title: &past,
                ..sound()
            }),
            Err(vec![DescriptorProblem::TitleTooLong])
        );
    }

    /// A title holding a character a reader sees nothing of is refused.
    ///
    /// The title is the one other field of free text beside the argument
    /// schema, and it is held to the same rule: without this, a title with a
    /// right-to-left override is accepted, showing a person a command under
    /// words it does not say.
    #[test]
    fn a_title_holding_a_character_a_reader_sees_nothing_of_is_refused() {
        for (title, hidden) in [
            ("Keep \u{202e}all drafts", '\u{202e}'),
            ("Put a note\u{e0041}\u{e0042}", '\u{e0041}'),
            ("Put\u{200b}a note", '\u{200b}'),
            ("Save\u{fe0f}\u{fe0f}", '\u{fe0f}'),
            ("Put\u{3164}a note", '\u{3164}'),
        ] {
            assert_eq!(
                judge(&Declared { title, ..sound() }),
                Err(vec![DescriptorProblem::TitleInvisibleCharacter(hidden)]),
                "`U+{:04X}` in a title was accepted",
                u32::from(hidden)
            );
        }
        assert!(
            judge(&Declared {
                title: "Übersicht öffnen — «notas»",
                ..sound()
            })
            .is_ok(),
            "a title of visible characters beyond ASCII was refused"
        );
    }

    /// An argument schema holding one is refused by the judge, not the compiler alone.
    ///
    /// This asks the judge itself: the kernel asks the judge of a plugin's
    /// command at `activate`, so a refusal standing in the compiler alone
    /// would accept there a description spelling `IGN` in tag characters
    /// behind a right-to-left override.
    #[test]
    fn an_argument_schema_a_reader_cannot_see_all_of_is_refused_by_the_judge() {
        let problems = judge(&Declared {
            args_schema: "{\"type\":\"object\",\"additionalProperties\":false,\
                \"description\":\"\u{e0049}\u{e0047}\u{e004e}\u{202e}\"}",
            action: "Read",
            resource_selector: "realm",
            mutating: false,
            reversibility: Reversibility::Read,
            ..sound()
        })
        .expect_err("the schema holds characters a reader sees nothing of");
        assert!(
            matches!(
                problems.as_slice(),
                [DescriptorProblem::Schema(error)]
                    if error.fault == SchemaFault::InvisibleCharacter('\u{e0049}')
            ),
            "{problems:?}"
        );
    }

    #[test]
    fn a_schema_outside_the_subset_is_the_schemas_problem() {
        let problems = judge(&Declared {
            args_schema: r##"{"type":"object","additionalProperties":false,"$ref":"#"}"##,
            resource_selector: "realm",
            ..sound()
        })
        .expect_err("`$ref` is outside the subset");
        assert!(
            matches!(
                problems.as_slice(),
                [DescriptorProblem::Schema(error)]
                    if error.fault == SchemaFault::UnknownKeyword("$ref".to_owned())
            ),
            "{problems:?}"
        );
    }

    #[test]
    fn a_selector_the_schema_does_not_back_is_the_selectors_problem() {
        assert_eq!(
            judge(&Declared {
                resource_selector: "argument:text",
                ..sound()
            }),
            Err(vec![DescriptorProblem::Selector(
                SelectorError::NotAnEntity("text".to_owned())
            )])
        );
        assert_eq!(
            judge(&Declared {
                resource_selector: "Argument:note",
                ..sound()
            }),
            Err(vec![DescriptorProblem::Selector(SelectorError::Malformed(
                "Argument:note".to_owned()
            ))])
        );
    }

    #[test]
    fn a_selector_outside_the_grammar_is_refused_even_when_the_schema_is_too() {
        let problems = judge(&Declared {
            args_schema: "{",
            resource_selector: "everything",
            ..sound()
        })
        .expect_err("both are broken");
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(matches!(problems[0], DescriptorProblem::Schema(_)));
        assert!(matches!(
            problems[1],
            DescriptorProblem::Selector(SelectorError::Malformed(_))
        ));
    }

    #[test]
    fn a_read_that_mutates_is_refused() {
        assert_eq!(
            judge(&Declared {
                reversibility: Reversibility::Read,
                ..sound()
            }),
            Err(vec![DescriptorProblem::ReadThatMutates])
        );
    }

    #[test]
    fn a_change_declared_as_not_mutating_is_refused() {
        for reversibility in [Reversibility::Write, Reversibility::Irreversible] {
            assert_eq!(
                judge(&Declared {
                    reversibility,
                    mutating: false,
                    ..sound()
                }),
                Err(vec![DescriptorProblem::MutationUndeclared]),
                "{reversibility:?} with mutating=false was accepted"
            );
        }
        assert!(
            judge(&Declared {
                reversibility: Reversibility::Irreversible,
                ..sound()
            })
            .is_ok(),
            "an irreversible mutating command is a declaration, not a defect"
        );
        assert!(
            judge(&Declared {
                reversibility: Reversibility::Read,
                mutating: false,
                ..sound()
            })
            .is_ok(),
            "a read that does not mutate is the plainest command there is"
        );
    }

    #[test]
    fn every_problem_is_reported_and_not_only_the_first() {
        let problems = judge(&Declared {
            name: "Nameless",
            title: &"t".repeat(TITLE_MAX + 1),
            args_schema: r#"{"type":"object"}"#,
            action: " ",
            resource_selector: "argument:note",
            mutating: true,
            reversibility: Reversibility::Read,
        })
        .expect_err("everything is wrong");
        assert_eq!(problems.len(), 5, "{problems:?}");
        assert_eq!(problems[0], DescriptorProblem::NameMalformed);
        assert_eq!(problems[1], DescriptorProblem::TitleTooLong);
        assert!(
            matches!(&problems[2], DescriptorProblem::Schema(error) if error.fault == SchemaFault::OpenObject)
        );
        assert_eq!(problems[3], DescriptorProblem::Missing(Field::Action));
        assert_eq!(problems[4], DescriptorProblem::ReadThatMutates);
    }

    #[test]
    fn every_problem_reads_as_a_sentence_naming_its_subject() {
        for (problem, names) in [
            (DescriptorProblem::Missing(Field::Name), "name"),
            (DescriptorProblem::Missing(Field::Title), "title"),
            (
                DescriptorProblem::Missing(Field::ArgsSchema),
                "argument schema",
            ),
            (DescriptorProblem::Missing(Field::Action), "action"),
            (
                DescriptorProblem::Missing(Field::ResourceSelector),
                "resource selector",
            ),
            (DescriptorProblem::NameMalformed, "96"),
            (DescriptorProblem::TitleTooLong, "128"),
            (
                DescriptorProblem::TitleInvisibleCharacter('\u{202e}'),
                "`U+202E`",
            ),
            (DescriptorProblem::ReadThatMutates, "`read`"),
            (DescriptorProblem::MutationUndeclared, "mutating=false"),
            (
                DescriptorProblem::Selector(SelectorError::NotRequired("note".to_owned())),
                "`note`",
            ),
        ] {
            let said = problem.to_string();
            assert!(said.contains(names), "`{said}` does not name `{names}`");
        }
        let schema = judge(&Declared {
            args_schema: "[]",
            ..sound()
        })
        .expect_err("refused");
        assert!(
            schema[0].to_string().contains("\"type\": \"object\""),
            "{}",
            schema[0]
        );
    }
}
