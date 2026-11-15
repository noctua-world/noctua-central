//! What a command acts on, read out of its arguments.
//!
//! A command declares a resource selector, and the decision point evaluates
//! the entity it selects (`NOCTUA.md §11.7`, `reference/identity-and-authorization.md §2`).
//! The grammar is three forms and nothing else:
//!
//! ```text
//! selector = "realm" / "argument:" name / "feature:" name
//! name     = [a-z] [a-z0-9_]*
//! ```
//!
//! `realm` is the realm the kernel serves. `argument:<name>` is the entity a
//! required string argument identifies, and the schema says which type that
//! is: the property must carry `x-entity` and a `maxLength` no longer than an
//! entity id may be. `feature:<id>` is a feature of the platform the realm's
//! directory holds — [`FEATURE_TYPE`] and the id written — for a command that
//! reads none of the realm's data, such as naming a file; whatever the
//! arguments hold, it selects that one entity, and a feature id is no longer
//! than an entity id may be. A selector the schema cannot back is refused when
//! the command is declared, never discovered when it is dispatched.
//!
//! **Only a command of the kernel's own selects a feature.** The grammar is
//! one for every caller, so this judge accepts the form from anyone; the
//! kernel refuses it in a plugin's declaration at `activate`
//! (`crates/nc-kernel/src/registry.rs`), so the entity a journal entry names
//! as a feature is only ever one the kernel's own command was decided on.
//!
//! # What this answers (`§2.4`)
//!
//! **An entity is never built by concatenation.** [`Selector::extract`] hands
//! back the type and the id apart, and an id outside [`ENTITY_ID_MAX`]
//! characters of `A-Z a-z 0-9 . _ : -` is refused — so an argument cannot
//! smuggle a `"` or a `::` into what the decision point reads as a type.
//!
//! **The realm is selected by `realm` alone.** An argument whose `x-entity` is
//! [`REALM_TYPE`] is refused: through it the caller would choose which realm's
//! id the decision point is handed, and the kernel's rule that a plugin selects
//! the realm only to read it is written against [`Selector::Realm`] — a
//! `Write` that reached the realm through an argument would pass that rule and
//! every permit on the realm with it.
//!
//! **What this does not answer** is whether the entity exists. An unknown one
//! falls to the decision point's default deny, which is indistinguishable from
//! one the caller may not touch; that is the answer to an existence oracle.

use core::fmt;

use serde_json::Value;

use crate::{Node, Schema, excerpt};

/// The selector that names the realm the kernel serves.
pub const REALM: &str = "realm";

/// The Cedar type of the entity [`Selector::Realm`] selects.
pub const REALM_TYPE: &str = "Realm";

/// The Cedar type of the entity [`Selector::Feature`] selects.
pub const FEATURE_TYPE: &str = "Feature";

/// The longest entity id, in characters; the same bound as an `OpaqueId`.
pub const ENTITY_ID_MAX: usize = 128;

/// How a command's resource is found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// The realm the kernel serves.
    Realm,
    /// The entity a required string argument identifies.
    Argument {
        /// The argument's name.
        property: String,
        /// The Cedar type its `x-entity` names.
        entity_type: String,
    },
    /// A feature of the platform, whatever the arguments hold.
    Feature {
        /// The feature's id, as the realm's directory holds it.
        id: String,
    },
}

/// An entity, as the decision point is handed one: type and id, apart.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityRef {
    /// A Cedar type name.
    pub type_name: String,
    /// An id of at most [`ENTITY_ID_MAX`] characters from the id alphabet.
    pub id: String,
}

/// A selector the grammar or the schema does not back.
///
/// Each variant holds what it names the way every diagnostic of this crate
/// repeats a name — escaped and cut short — because a selector's text is its
/// author's, and a plugin's at `activate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorError {
    /// Neither `realm` nor `argument:` or `feature:` and a well-formed name.
    Malformed(String),
    /// The argument it names is not a property the schema declares.
    NotDeclared(String),
    /// The argument it names is not required, so a dispatch could lack it.
    NotRequired(String),
    /// The argument it names is not a string carrying `x-entity`.
    NotAnEntity(String),
    /// The argument it names has no `maxLength`, or one past [`ENTITY_ID_MAX`].
    Unbounded(String),
    /// The argument it names holds an id of [`REALM_TYPE`], which `realm` alone selects.
    RealmThroughAnArgument(String),
}

impl fmt::Display for SelectorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(written) => write!(
                formatter,
                "`{written}` is neither `realm` nor `argument:` or `feature:` followed by a \
                 name in `[a-z][a-z0-9_]*`, a feature's {ENTITY_ID_MAX} characters at most"
            ),
            Self::NotDeclared(name) => write!(
                formatter,
                "`argument:{name}` names a property the schema does not declare"
            ),
            Self::NotRequired(name) => write!(
                formatter,
                "`{name}` is not in `required`, so a dispatch without it would select nothing"
            ),
            Self::NotAnEntity(name) => write!(
                formatter,
                "`{name}` is not `\"type\": \"string\"` with an `x-entity`, so what it holds \
                 names no entity type"
            ),
            Self::Unbounded(name) => write!(
                formatter,
                "`{name}` declares no `maxLength` of at most {ENTITY_ID_MAX}, the longest an \
                 entity id may be"
            ),
            Self::RealmThroughAnArgument(name) => write!(
                formatter,
                "`{name}` names `{REALM_TYPE}`, and the realm is selected by `{REALM}` alone, \
                 with the id the kernel serves — through an argument, whoever dispatches would \
                 choose which realm is authorized"
            ),
        }
    }
}

impl std::error::Error for SelectorError {}

/// Arguments a selector cannot read an entity out of.
///
/// The name each variant holds is repeated as [`SelectorError`]'s are:
/// escaped and cut short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    /// The argument is absent or is not a string.
    Absent(String),
    /// The id is not 1 to [`ENTITY_ID_MAX`] characters of the id alphabet.
    IdMalformed(String),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent(name) => write!(formatter, "`{name}` is absent or is not a string"),
            Self::IdMalformed(name) => write!(
                formatter,
                "`{name}` is not an entity id: 1 to {ENTITY_ID_MAX} characters of \
                 `A-Z a-z 0-9 . _ : -`"
            ),
        }
    }
}

impl std::error::Error for ExtractError {}

/// A selector the grammar accepts, before the schema has been asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Written<'a> {
    Realm,
    Argument(&'a str),
    Feature(&'a str),
}

/// Reads a selector against the grammar alone.
pub(crate) fn written(text: &str) -> Result<Written<'_>, SelectorError> {
    if text == REALM {
        return Ok(Written::Realm);
    }
    // Written inline rather than as constants: this tree reads a `&str`
    // constant ending in a colon as an escape-hatch marker, and a selector's
    // prefix is not one.
    let (name, written) = if let Some(name) = text.strip_prefix("argument:") {
        (name, Written::Argument(name))
    } else if let Some(id) = text.strip_prefix("feature:") {
        (id, Written::Feature(id))
    } else {
        return Err(SelectorError::Malformed(excerpt(text)));
    };
    let mut characters = name.chars();
    let well_formed = characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_');
    // A feature's id is an entity's id as it is written: bounded here, where
    // it is declared, rather than refused at every dispatch.
    let bounded = !matches!(written, Written::Feature(_)) || name.len() <= ENTITY_ID_MAX;
    if well_formed && bounded {
        Ok(written)
    } else {
        Err(SelectorError::Malformed(excerpt(text)))
    }
}

impl Written<'_> {
    /// Holds a written selector to the schema it will read arguments through.
    pub(crate) fn resolved(self, schema: &Schema) -> Result<Selector, SelectorError> {
        let name = match self {
            Self::Realm => return Ok(Selector::Realm),
            Self::Feature(id) => return Ok(Selector::Feature { id: id.to_owned() }),
            Self::Argument(name) => name,
        };
        let root = schema.root();
        let Some(property) = root.property(name) else {
            return Err(SelectorError::NotDeclared(excerpt(name)));
        };
        if !root.requires(name) {
            return Err(SelectorError::NotRequired(excerpt(name)));
        }
        let Node::String {
            max_length,
            entity: Some(entity_type),
            ..
        } = property
        else {
            return Err(SelectorError::NotAnEntity(excerpt(name)));
        };
        if entity_type == REALM_TYPE {
            return Err(SelectorError::RealmThroughAnArgument(excerpt(name)));
        }
        if max_length.is_none_or(|bound| bound > ENTITY_ID_MAX as u64) {
            return Err(SelectorError::Unbounded(excerpt(name)));
        }
        Ok(Selector::Argument {
            property: name.to_owned(),
            entity_type: entity_type.clone(),
        })
    }
}

impl Selector {
    /// Reads a selector, holding it to the grammar and to the schema.
    ///
    /// # Errors
    ///
    /// [`SelectorError::Malformed`] for text outside the grammar — a feature's
    /// id longer than [`ENTITY_ID_MAX`] included — and for an `argument:`
    /// selector [`SelectorError::NotDeclared`],
    /// [`SelectorError::NotRequired`], [`SelectorError::NotAnEntity`],
    /// [`SelectorError::RealmThroughAnArgument`] or [`SelectorError::Unbounded`]
    /// when the schema does not back it.
    pub fn parse(text: &str, schema: &Schema) -> Result<Self, SelectorError> {
        written(text)?.resolved(schema)
    }

    /// The Cedar type of the entity this selects.
    #[must_use]
    pub fn entity_type(&self) -> &str {
        match self {
            Self::Realm => REALM_TYPE,
            Self::Argument { entity_type, .. } => entity_type,
            Self::Feature { .. } => FEATURE_TYPE,
        }
    }

    /// The entity these arguments select, in the realm `realm_id` names.
    ///
    /// Arguments are validated against the schema first, so an absent
    /// argument is a caller that skipped that step; it is refused all the same.
    /// A feature reads no argument: it is the one entity its id names.
    ///
    /// # Errors
    ///
    /// [`ExtractError::Absent`] when the argument is missing or not a string,
    /// and [`ExtractError::IdMalformed`] when the id — or the realm's own —
    /// is not one the decision point may be handed.
    pub fn extract(&self, realm_id: &str, args: &Value) -> Result<EntityRef, ExtractError> {
        let (named, id) = match self {
            Self::Realm => (REALM, realm_id),
            Self::Feature { id } => ("feature", id.as_str()),
            Self::Argument { property, .. } => {
                let Some(id) = args.get(property).and_then(Value::as_str) else {
                    return Err(ExtractError::Absent(excerpt(property)));
                };
                (property.as_str(), id)
            }
        };
        if !id_is_well_formed(id) {
            return Err(ExtractError::IdMalformed(excerpt(named)));
        }
        Ok(EntityRef {
            type_name: self.entity_type().to_owned(),
            id: id.to_owned(),
        })
    }
}

/// Whether `id` is 1 to [`ENTITY_ID_MAX`] characters of `A-Z a-z 0-9 . _ : -`.
///
/// The `OpaqueId` alphabet, so an id this accepts is one the rest of the
/// platform can carry, and one holding a quote or a space never reaches the
/// decision point.
#[must_use]
pub fn id_is_well_formed(id: &str) -> bool {
    (1..=ENTITY_ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ENTITY_ID_MAX, EntityRef, ExtractError, FEATURE_TYPE, Selector, SelectorError,
        id_is_well_formed,
    };
    use crate::{Declared, DescriptorProblem, EXCERPT, Reversibility, Schema, judge};

    /// A schema with one argument of each shape a selector can meet.
    fn schema() -> Schema {
        Schema::compile(
            r#"{"type":"object","additionalProperties":false,
                "properties":{
                  "note":{"type":"string","x-entity":"Content","maxLength":128},
                  "loose":{"type":"string","x-entity":"Content","maxLength":128},
                  "count":{"type":"integer"},
                  "plain":{"type":"string","maxLength":16},
                  "long":{"type":"string","x-entity":"Content","maxLength":129},
                  "open":{"type":"string","x-entity":"Content"}},
                "required":["note","count","plain","long","open"]}"#,
        )
        .expect("the fixture is inside the subset")
    }

    #[test]
    fn realm_selects_the_realm_and_reads_no_argument() {
        let selector = Selector::parse("realm", &schema()).expect("realm is the grammar's");
        assert_eq!(selector, Selector::Realm);
        assert_eq!(selector.entity_type(), "Realm");
        assert_eq!(
            selector.extract("home", &json!({})),
            Ok(EntityRef {
                type_name: "Realm".to_owned(),
                id: "home".to_owned()
            })
        );
    }

    #[test]
    fn an_argument_the_schema_backs_selects_the_entity_its_type_names() {
        let selector = Selector::parse("argument:note", &schema()).expect("backed");
        assert_eq!(
            selector,
            Selector::Argument {
                property: "note".to_owned(),
                entity_type: "Content".to_owned()
            }
        );
        assert_eq!(selector.entity_type(), "Content");
        assert_eq!(
            selector.extract("home", &json!({"note": "photo", "count": 1})),
            Ok(EntityRef {
                type_name: "Content".to_owned(),
                id: "photo".to_owned()
            })
        );
    }

    #[test]
    fn a_selector_outside_the_grammar_is_refused() {
        for written in [
            "",
            "Realm",
            " realm",
            "realm ",
            "argument:",
            "argument",
            "argument:Note",
            "argument:1note",
            "argument:_note",
            "argument:no-te",
            "argument: note",
            "argument:note.id",
            "argument:nöte",
            "arg:note",
        ] {
            assert!(
                matches!(
                    Selector::parse(written, &schema()),
                    Err(SelectorError::Malformed(_))
                ),
                "`{written}` was not refused by the grammar"
            );
        }
        // And the grammar's own alphabet, every class of it, is accepted.
        assert!(super::written("argument:n0_te_9").is_ok());
    }

    /// A feature selector names its feature whatever the arguments hold.
    ///
    /// **It reads no argument.** A command selecting `feature:naming` is decided
    /// on `Feature::"naming"` and on nothing a caller wrote, so an argument
    /// called `feature`, or one carrying an entity, chooses nothing — and the
    /// schema is not asked to back it, since there is no argument to back.
    #[test]
    fn a_feature_selector_names_its_feature_whatever_the_arguments_hold() {
        assert_eq!(
            Selector::parse("feature:gallery", &schema()),
            Ok(Selector::Feature {
                id: "gallery".to_owned()
            }),
            "a feature selector names the feature written, and no other"
        );
        let selector = Selector::parse("feature:naming", &schema()).expect("the grammar's");
        assert_eq!(
            selector,
            Selector::Feature {
                id: "naming".to_owned()
            }
        );
        assert_eq!(selector.entity_type(), FEATURE_TYPE);
        assert_eq!(FEATURE_TYPE, "Feature");
        for arguments in [
            json!({}),
            json!({"feature": "gallery"}),
            json!({"note": "photo", "naming": "gallery"}),
            json!(null),
        ] {
            assert_eq!(
                selector.extract("home", &arguments),
                Ok(EntityRef {
                    type_name: "Feature".to_owned(),
                    id: "naming".to_owned()
                }),
                "{arguments} chose the feature"
            );
        }
        let nothing = Schema::compile(r#"{"type":"object","additionalProperties":false}"#)
            .expect("inside the subset");
        assert!(
            Selector::parse("feature:naming", &nothing).is_ok(),
            "a schema declaring nothing backs a selector that reads nothing"
        );
        let longest = format!("feature:{}", "a".repeat(ENTITY_ID_MAX));
        let selected = Selector::parse(&longest, &nothing).expect("as long as an id may be");
        assert!(selected.extract("home", &json!({})).is_ok());
    }

    /// A feature selector outside the grammar is refused when it is declared.
    #[test]
    fn a_feature_selector_outside_the_grammar_is_refused() {
        let past = format!("feature:{}", "a".repeat(ENTITY_ID_MAX + 1));
        for written in [
            "feature:",
            "feature",
            "Feature:naming",
            "feature:Naming",
            "feature:1x",
            "feature:_x",
            "feature:a-b",
            "feature: x",
            "feature:a.b",
            "feature:a:b",
            "feature:nämig",
            past.as_str(),
        ] {
            assert!(
                matches!(
                    Selector::parse(written, &schema()),
                    Err(SelectorError::Malformed(_))
                ),
                "`{written}` was not refused by the grammar"
            );
        }
        assert!(super::written("feature:n0_te_9").is_ok());
    }

    #[test]
    fn an_argument_the_schema_does_not_declare_is_refused() {
        assert_eq!(
            Selector::parse("argument:target", &schema()),
            Err(SelectorError::NotDeclared("target".to_owned()))
        );
    }

    #[test]
    fn an_argument_that_is_not_required_is_refused() {
        assert_eq!(
            Selector::parse("argument:loose", &schema()),
            Err(SelectorError::NotRequired("loose".to_owned()))
        );
    }

    #[test]
    fn an_argument_that_names_no_entity_type_is_refused() {
        for name in ["count", "plain"] {
            assert_eq!(
                Selector::parse(&format!("argument:{name}"), &schema()),
                Err(SelectorError::NotAnEntity(name.to_owned())),
                "`{name}` selects no entity and was not refused"
            );
        }
    }

    #[test]
    fn an_argument_longer_than_an_id_may_be_is_refused() {
        for name in ["long", "open"] {
            assert_eq!(
                Selector::parse(&format!("argument:{name}"), &schema()),
                Err(SelectorError::Unbounded(name.to_owned())),
                "`{name}` may exceed an id's length and was not refused"
            );
        }
    }

    /// An argument naming `Realm` is refused, however well the schema backs it.
    ///
    /// Otherwise a plugin's `Write` command selecting `argument:r`
    /// with `"x-entity": "Realm"` is judged sound and
    /// extracts `Realm::"<whatever the caller wrote>"`, while the kernel's rule
    /// that a plugin selects the realm only to read it is written against
    /// [`Selector::Realm`] — so the command passes it, and the caller chooses
    /// which realm is authorized.
    #[test]
    fn an_argument_naming_the_realm_type_is_refused() {
        let schema = Schema::compile(
            r#"{"type":"object","additionalProperties":false,
                "properties":{"r":{"type":"string","x-entity":"Realm","maxLength":128}},
                "required":["r"]}"#,
        )
        .expect("the fixture is inside the subset");
        assert_eq!(
            Selector::parse("argument:r", &schema),
            Err(SelectorError::RealmThroughAnArgument("r".to_owned()))
        );
        assert_eq!(
            Selector::parse("realm", &schema),
            Ok(Selector::Realm),
            "the realm is still selected the one way it may be"
        );
        // And through the judge, as the kernel will meet it at `activate`.
        assert_eq!(
            judge(&Declared {
                name: "reference.wipe",
                title: "Wipe",
                args_schema: r#"{"type":"object","additionalProperties":false,
                    "properties":{"r":{"type":"string","x-entity":"Realm","maxLength":128}},
                    "required":["r"]}"#,
                action: "Write",
                resource_selector: "argument:r",
                mutating: true,
                reversibility: Reversibility::Irreversible,
            }),
            Err(vec![DescriptorProblem::Selector(
                SelectorError::RealmThroughAnArgument("r".to_owned())
            )])
        );
    }

    #[test]
    fn an_absent_argument_is_refused_when_extracting() {
        let selector = Selector::parse("argument:note", &schema()).expect("backed");
        for arguments in [json!({}), json!({"note": 7}), json!(null)] {
            assert_eq!(
                selector.extract("home", &arguments),
                Err(ExtractError::Absent("note".to_owned())),
                "{arguments} was read as an entity"
            );
        }
    }

    #[test]
    fn an_id_outside_the_alphabet_or_the_length_is_refused() {
        let selector = Selector::parse("argument:note", &schema()).expect("backed");
        let longest = "a".repeat(ENTITY_ID_MAX);
        let past = "a".repeat(ENTITY_ID_MAX + 1);
        for id in ["", "a b", "a\"b", "é", "a/b", "a\u{0}", past.as_str()] {
            assert_eq!(
                selector.extract("home", &json!({ "note": id })),
                Err(ExtractError::IdMalformed("note".to_owned())),
                "`{id}` reached the decision point"
            );
        }
        for id in ["A-z_0.9:x", longest.as_str()] {
            assert!(
                selector.extract("home", &json!({ "note": id })).is_ok(),
                "`{id}` is an id and was refused"
            );
        }
        assert!(id_is_well_formed("g.current"));
    }

    #[test]
    fn the_realms_own_id_is_held_to_the_same_alphabet() {
        assert_eq!(
            Selector::Realm.extract("home office", &json!({})),
            Err(ExtractError::IdMalformed("realm".to_owned()))
        );
    }

    /// Every refusal repeats the name it refuses cut short, as `Malformed` does.
    ///
    /// Were `Malformed` alone held to the rule every diagnostic of this crate
    /// follows, a selector naming a megabyte of argument would come back from
    /// the five refusals beside it as a megabyte of diagnostic — at
    /// `activate`, where the selector is a plugin's text and the kernel prints
    /// the refusal.
    #[test]
    fn every_refusal_repeats_the_name_it_refuses_cut_short() {
        let long = "a".repeat(1_000_000);
        let selector = format!("argument:{long}");
        // A schema whose one property, named `long`, has the shape given.
        let declaring = |property: &serde_json::Value, required: bool| {
            let mut schema = json!({"type": "object", "additionalProperties": false,
                                    "properties": { long.as_str(): property }});
            if required {
                schema["required"] = json!([long]);
            }
            Schema::compile(&schema.to_string()).expect("the fixture is inside the subset")
        };
        let entity = json!({"type": "string", "x-entity": "Content", "maxLength": 128});
        let nothing = r#"{"type":"object","additionalProperties":false}"#;
        let refused = |schema: &Schema| {
            Selector::parse(&selector, schema).expect_err("the schema does not back it")
        };
        let backed = Selector::parse(&selector, &declaring(&entity, true)).expect("backed");
        let mut said = Vec::new();
        for (refusal, kind) in [
            (
                refused(&Schema::compile(nothing).expect("inside the subset")),
                "not declared",
            ),
            (refused(&declaring(&entity, false)), "not required"),
            (
                refused(&declaring(&json!({"type": "integer"}), true)),
                "not an entity",
            ),
            (
                refused(&declaring(
                    &json!({"type": "string", "x-entity": "Content"}),
                    true,
                )),
                "unbounded",
            ),
            (
                refused(&declaring(
                    &json!({"type": "string", "x-entity": "Realm", "maxLength": 128}),
                    true,
                )),
                "the realm",
            ),
        ] {
            assert!(
                matches!(
                    (&refusal, kind),
                    (SelectorError::NotDeclared(_), "not declared")
                        | (SelectorError::NotRequired(_), "not required")
                        | (SelectorError::NotAnEntity(_), "not an entity")
                        | (SelectorError::Unbounded(_), "unbounded")
                        | (SelectorError::RealmThroughAnArgument(_), "the realm")
                ),
                "the case for `{kind}` was refused as something else"
            );
            said.push(refusal.to_string());
        }
        said.extend(what_text_outside_the_grammar_is_refused_with(&long));
        for (arguments, kind) in [
            (json!({}), "absent"),
            (json!({ long.as_str(): "a b" }), "malformed"),
        ] {
            let refusal = backed.extract("home", &arguments).expect_err("no entity");
            assert!(
                matches!(
                    (&refusal, kind),
                    (ExtractError::Absent(_), "absent")
                        | (ExtractError::IdMalformed(_), "malformed")
                ),
                "the case for `{kind}` was refused as something else"
            );
            said.push(refusal.to_string());
        }
        // And through the judge, which is how the kernel meets a selector.
        let judged = judge(&Declared {
            name: "reference.note.put",
            title: "Put a note",
            args_schema: nothing,
            action: "Write",
            resource_selector: &selector,
            mutating: true,
            reversibility: Reversibility::Write,
        })
        .expect_err("the schema declares nothing");
        said.extend(judged.iter().map(ToString::to_string));
        assert_eq!(said.len(), 11, "a refusal was not reached");
        for refusal in said {
            // The name cut short, and the sentence around it.
            assert!(
                refusal.chars().count() < 2 * (EXCERPT + 1) + 256,
                "a refusal repeated {} characters of a name",
                refusal.len()
            );
            assert!(
                !refusal.chars().any(char::is_control),
                "a refusal repeated a control character of the text it refused"
            );
        }
    }

    /// What the grammar says of `long` in every way out of it.
    ///
    /// No prefix at all, and `argument:` or `feature:` before a name the
    /// grammar refuses. Each here opens with `ESC [2J`, which clears the
    /// terminal a raw repeat reaches, and any way out could repeat the text raw.
    fn what_text_outside_the_grammar_is_refused_with(long: &str) -> Vec<String> {
        let empty = Schema::compile(r#"{"type":"object","additionalProperties":false}"#)
            .expect("inside the subset");
        let mut said = Vec::new();
        for (written, kind) in [
            (format!("\u{1b}[2J{long}"), "no prefix"),
            (
                format!("argument:\u{1b}[2J{long}"),
                "a name outside the grammar",
            ),
            (
                format!("feature:\u{1b}[2J{long}"),
                "a feature outside the grammar",
            ),
        ] {
            let refusal = Selector::parse(&written, &empty).expect_err("outside the grammar");
            assert!(
                matches!(refusal, SelectorError::Malformed(_)),
                "the case for {kind} was refused as something else"
            );
            said.push(refusal.to_string());
        }
        said
    }

    #[test]
    fn every_refusal_names_what_it_refused() {
        for (refusal, names) in [
            (
                SelectorError::Malformed("argument:X".to_owned()).to_string(),
                "argument:X",
            ),
            (
                SelectorError::NotDeclared("target".to_owned()).to_string(),
                "target",
            ),
            (
                SelectorError::NotRequired("loose".to_owned()).to_string(),
                "loose",
            ),
            (
                SelectorError::NotAnEntity("count".to_owned()).to_string(),
                "count",
            ),
            (
                SelectorError::Unbounded("long".to_owned()).to_string(),
                "long",
            ),
            (
                SelectorError::RealmThroughAnArgument("home".to_owned()).to_string(),
                "home",
            ),
            (ExtractError::Absent("note".to_owned()).to_string(), "note"),
            (
                ExtractError::IdMalformed("note".to_owned()).to_string(),
                "note",
            ),
        ] {
            assert!(
                refusal.contains(names),
                "`{refusal}` does not name `{names}`"
            );
        }
    }
}
