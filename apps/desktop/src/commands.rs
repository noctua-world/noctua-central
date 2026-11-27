//! The command list (`NOCTUA.md §12.9`): every registered command, filtered
//! by what follows the `;` or `:` the prompt line begins with, and a chosen
//! command's parameters, offered and completed from its argument schema.
//!
//! **Every registered command, straight from the registry**: the daemon's
//! registry, asked in each locale, holds the catalog's commands and every
//! loaded plugin's, so nothing is listed twice and a plugin's command is
//! listed while it is loaded; before the daemon has answered, the catalog's
//! own are listed ([`Commands::from_catalog`]).
//!
//! **Filtered by the one fuzzy core, as `kernel.find` filters** (§12.4): each
//! command is an `nc_fuzzy::Item` of the fields `kernel.find` builds one of,
//! in its order — its dotted name, its display name in each locale, then its
//! tags in each locale, a name a `Role::Name` and a tag a `Role::Tag` — ranked
//! by `nc_fuzzy::Matcher`, so the window and the CLI find the same commands in
//! the same order. **Matched locally, never asked**: every dispatch is
//! journaled, and a filter runs at the rate a person types.
//!
//! **Coloured per word, by actual positions** ([`hits`]): a row shows the
//! command's display name in the window's locale, and each word of the query
//! that matched that field marks the characters it matched, the first word's
//! slot first — a word that matched another field marks nothing on it. A
//! position is a character, and the window draws a grapheme cluster whole
//! ([`whole`]).
//!
//! **Parameters are text the prompt holds** ([`arguments`]): once a command
//! that takes parameters is chosen, the prompt line holds its arguments as
//! `name=value` words, a word without `=` continuing the value before it — and
//! a command of one parameter takes the whole line as its value — so the
//! draft, which the daemon keeps, is all that was given; Enter runs the command
//! with them, each value read as its schema's type where it reads as one and
//! handed to the kernel as text where it does not, so the schema's refusal is
//! the kernel's. What is offered ([`offered`]) is the parameters not given yet
//! and, while a parameter with a closed set of values is being typed, those
//! values that begin with what was typed.
//!
//! **Decided here, where `NOCTUA.md` is silent**: parameters are listed
//! required first, then by name, since a schema's object is read unordered;
//! a grapheme cluster is a character and every combining mark, variation
//! selector and emoji modifier after it, and a character joined to it by a
//! zero-width joiner — not the whole of UAX #29.

use nc_canon::Value;
use nc_catalog::Locale;
use nc_fuzzy::{Item, Matcher, Query, Role};

/// A command's texts in one locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    /// Its display name.
    pub title: String,
    /// Its line description.
    pub line: String,
    /// The words a search finds it by.
    pub tags: Vec<String>,
}

/// What a parameter's value is read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    /// A whole number.
    Integer,
    /// True or false.
    Boolean,
    /// Anything else, given as text.
    Text,
}

/// One parameter of a command, as its argument schema declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// Its name.
    pub name: String,
    /// Whether the schema requires it.
    pub required: bool,
    /// What its value is read as.
    pub kind: Type,
    /// The values it takes, where they are a closed set: its `enum`, or true
    /// and false.
    pub choices: Vec<String>,
}

/// A registered command, as the list shows and finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Its registered name.
    pub name: String,
    /// Its texts, in each locale of `Locale::ALL`, in that order.
    pub texts: Vec<Texts>,
    /// Its parameters, required first, then by name.
    pub parameters: Vec<Parameter>,
}

impl Command {
    /// Its texts in `locale`.
    #[must_use]
    pub fn texts(&self, locale: Locale) -> Option<&Texts> {
        Locale::ALL
            .iter()
            .position(|each| *each == locale)
            .and_then(|at| self.texts.get(at))
    }
}

/// The parameters `schema`, a command's argument schema as JSON text,
/// declares: none for a schema that is not an object with `properties`.
#[must_use]
pub fn parameters_of(schema: &str) -> Vec<Parameter> {
    let Some(schema) = nc_canon::parse(schema).ok().and_then(Value::into_object) else {
        return Vec::new();
    };
    let required: Vec<&str> = match schema.get("required") {
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(|name| match name {
                Value::Text(name) => Some(name.as_str()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let Some(Value::Object(properties)) = schema.get("properties") else {
        return Vec::new();
    };
    let mut parameters: Vec<Parameter> = properties
        .iter()
        .map(|(name, property)| {
            let field = |key: &str| match property {
                Value::Object(fields) => fields.get(key),
                _ => None,
            };
            let kind = match field("type") {
                Some(Value::Text(kind)) if kind == "integer" => Type::Integer,
                Some(Value::Text(kind)) if kind == "boolean" => Type::Boolean,
                _ => Type::Text,
            };
            let choices = match (kind, field("enum")) {
                (Type::Boolean, _) => vec!["true".to_owned(), "false".to_owned()],
                (_, Some(Value::Array(values))) => values
                    .iter()
                    .filter_map(|value| match value {
                        Value::Text(text) => Some(text.clone()),
                        Value::Integer(number) => Some(number.to_string()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            Parameter {
                name: name.clone(),
                required: required.contains(&name.as_str()),
                kind,
                choices,
            }
        })
        .collect();
    parameters.sort_by(|one, other| {
        other
            .required
            .cmp(&one.required)
            .then_with(|| one.name.cmp(&other.name))
    });
    parameters
}

/// Every command the list shows, and the items it finds them by.
#[derive(Debug, Clone)]
pub struct Commands {
    /// Every command, in the registry's order.
    list: Vec<Command>,
    /// Each command's item, in the same order.
    items: Vec<Item>,
}

impl PartialEq for Commands {
    fn eq(&self, other: &Self) -> bool {
        self.list == other.list
    }
}

/// A command found, as a row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Its index in the list.
    pub index: usize,
    /// The characters of its display name in the window's locale each word
    /// matched, in `MatchText`'s grammar; nothing for an empty query.
    pub hits: Option<String>,
}

/// The fields of `command` in the order `kernel.find` builds an item with:
/// its dotted name, its display name in each locale, then its tags in each.
fn fields_of(command: &Command) -> Vec<(Role, &str)> {
    let mut fields = vec![(Role::Name, command.name.as_str())];
    fields.extend(
        command
            .texts
            .iter()
            .map(|texts| (Role::Name, texts.title.as_str())),
    );
    for texts in &command.texts {
        fields.extend(texts.tags.iter().map(|tag| (Role::Tag, tag.as_str())));
    }
    fields
}

impl Commands {
    /// The list of `list`, in its order.
    #[must_use]
    pub fn new(list: Vec<Command>) -> Self {
        let items = list
            .iter()
            .map(|command| Item::new(fields_of(command)))
            .collect();
        Self { list, items }
    }

    /// The catalog's own commands, which the registry holds before any
    /// plugin is loaded.
    #[must_use]
    pub fn from_catalog() -> Self {
        Self::new(
            nc_catalog::COMMANDS
                .iter()
                .map(|command| Command {
                    name: command.name.to_owned(),
                    texts: Locale::ALL
                        .iter()
                        .map(|locale| Texts {
                            title: nc_catalog::message(command.title_key, *locale)
                                .unwrap_or(command.name)
                                .to_owned(),
                            line: nc_catalog::message(command.description_key, *locale)
                                .unwrap_or_default()
                                .to_owned(),
                            tags: nc_catalog::tags(command.tags_key, *locale)
                                .unwrap_or_default()
                                .iter()
                                .map(|tag| (*tag).to_owned())
                                .collect(),
                        })
                        .collect(),
                    parameters: parameters_of(command.args_schema),
                })
                .collect(),
        )
    }

    /// The commands the registry listed in each locale of `Locale::ALL`, in
    /// that order: a command every listing holds, in the first one's order,
    /// its texts each listing's.
    #[must_use]
    pub fn from_registry(listings: &[Vec<nc_ipc::client::Listed>]) -> Self {
        let Some(first) = listings.first() else {
            return Self::new(Vec::new());
        };
        Self::new(
            first
                .iter()
                .filter_map(|listed| {
                    let texts = listings
                        .iter()
                        .map(|listing| {
                            listing
                                .iter()
                                .find(|each| each.name == listed.name)
                                .map(|each| Texts {
                                    title: each.display_name.clone(),
                                    line: each.line.clone(),
                                    tags: each.tags.clone(),
                                })
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(Command {
                        name: listed.name.clone(),
                        texts,
                        parameters: parameters_of(&listed.args_schema),
                    })
                })
                .collect(),
        )
    }

    /// Every command.
    #[must_use]
    pub fn list(&self) -> &[Command] {
        &self.list
    }

    /// The command named `name`.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Command> {
        self.list.iter().find(|command| command.name == name)
    }

    /// The commands `typed` finds, best first — every one in the list's order
    /// for an empty query — each with the hits on its display name in
    /// `locale`; nothing for a query past the core's bound.
    #[must_use]
    pub fn found(&self, typed: &str, locale: Locale) -> Vec<Found> {
        let Ok(query) = Query::parse(typed) else {
            return Vec::new();
        };
        let shown = 1 + Locale::ALL
            .iter()
            .position(|each| *each == locale)
            .unwrap_or(0);
        Matcher::new()
            .ranked(&query, &self.items)
            .iter()
            .map(|ranked| Found {
                index: ranked.index(),
                hits: (!query.is_empty()).then(|| {
                    let title = self.list[ranked.index()]
                        .texts
                        .get(shown - 1)
                        .map_or("", |texts| texts.title.as_str());
                    hits(
                        title,
                        ranked.found().hits().iter().map(|hit| {
                            if hit.field() == shown {
                                hit.positions()
                            } else {
                                &[]
                            }
                        }),
                    )
                }),
            })
            .collect()
    }
}

/// Whether `character` belongs to the grapheme cluster before it: a
/// combining mark, a variation selector or an emoji modifier.
const fn extends(character: char) -> bool {
    matches!(
        character as u32,
        0x0300..=0x036F
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
            | 0xFE00..=0xFE0F
            | 0xE0100..=0xE01EF
            | 0x1F3FB..=0x1F3FF
    )
}

/// The zero-width joiner, which joins the character after it to the cluster
/// before it.
const JOINER: char = '\u{200D}';

/// The grapheme cluster each character of `text` is in, by the index of the
/// cluster's first character.
fn clusters(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut joined = false;
    for (at, character) in text.chars().enumerate() {
        let start = match starts.last() {
            Some(&start) if joined || extends(character) || character == JOINER => start,
            _ => at,
        };
        starts.push(start);
        joined = character == JOINER;
    }
    starts
}

/// `positions` in `text` widened to every character of each grapheme cluster
/// one of them is in, rising and each once.
#[must_use]
pub fn whole(text: &str, positions: &[usize]) -> Vec<usize> {
    let starts = clusters(text);
    let hit: Vec<usize> = positions
        .iter()
        .filter_map(|at| starts.get(*at).copied())
        .collect();
    starts
        .iter()
        .enumerate()
        .filter(|(_, start)| hit.contains(start))
        .map(|(at, _)| at)
        .collect()
}

/// `positions`, rising, as `MatchText` writes a slot: single indices and
/// inclusive ranges, joined by commas.
fn slot(positions: &[usize]) -> String {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for &at in positions {
        match runs.last_mut() {
            Some((_, last)) if *last + 1 == at => *last = at,
            _ => runs.push((at, at)),
        }
    }
    runs.iter()
        .map(|(first, last)| {
            if first == last {
                first.to_string()
            } else {
                format!("{first}-{last}")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The hits of `words` on `text`, in `MatchText`'s grammar: a slot for each
/// word, in order, each its positions widened to whole grapheme clusters.
#[must_use]
pub fn hits<'a>(text: &str, words: impl Iterator<Item = &'a [usize]>) -> String {
    words
        .map(|positions| slot(&whole(text, positions)))
        .collect::<Vec<_>>()
        .join("|")
}

/// The words of `draft`, each a parameter of `command` named by `name=` and
/// the text that follows it to the next such word.
fn given<'a>(draft: &'a str, command: &Command) -> Vec<(&'a str, String)> {
    let mut given: Vec<(&str, String)> = Vec::new();
    for word in draft.split(' ') {
        let named = word.split_once('=').filter(|(name, _)| {
            command
                .parameters
                .iter()
                .any(|parameter| parameter.name == *name)
        });
        match (named, given.last_mut()) {
            (Some((name, value)), _) => given.push((name, value.to_owned())),
            (None, Some((_, value))) => {
                value.push(' ');
                value.push_str(word);
            }
            (None, None) => {}
        }
    }
    given
}

/// The arguments `draft` gives `command`: each parameter it names, its value
/// read as its schema's type where it reads as one and as text where it does
/// not; for a command of one parameter and a draft naming none, the whole of
/// it, trimmed, where anything was typed.
#[must_use]
pub fn arguments(draft: &str, command: &Command) -> Value {
    let mut given = given(draft, command);
    if given.is_empty()
        && let [only] = command.parameters.as_slice()
        && !draft.trim().is_empty()
    {
        given.push((only.name.as_str(), draft.trim().to_owned()));
    }
    Value::object(given.into_iter().map(|(name, value)| {
        let kind = command
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .map_or(Type::Text, |parameter| parameter.kind);
        let read = match kind {
            Type::Integer => value.trim().parse::<i64>().ok().map(Value::Integer),
            Type::Boolean => match value.trim() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                _ => None,
            },
            Type::Text => None,
        };
        (name, read.unwrap_or_else(|| Value::text(value)))
    }))
}

/// What the prompt offers while it asks for a command's parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// A parameter not given yet: choosing it writes `name=`.
    Parameter(String),
    /// A value of the parameter being typed: choosing it writes it.
    Value(String),
}

impl Offer {
    /// The word an option of the list is found by.
    #[must_use]
    pub fn value(&self) -> String {
        match self {
            Self::Parameter(name) => format!("parameter:{name}"),
            Self::Value(value) => format!("value:{value}"),
        }
    }
}

/// What `draft` is offered for `command`: while its last word names a
/// parameter whose values are a closed set, those that begin with what follows
/// the `=`; otherwise every parameter it does not name yet.
#[must_use]
pub fn offered(draft: &str, command: &Command) -> Vec<Offer> {
    let last = draft.rsplit(' ').next().unwrap_or_default();
    if let Some((name, typed)) = last.split_once('=')
        && let Some(parameter) = command
            .parameters
            .iter()
            .find(|parameter| parameter.name == name && !parameter.choices.is_empty())
    {
        return parameter
            .choices
            .iter()
            .filter(|choice| choice.starts_with(typed))
            .map(|choice| Offer::Value(choice.clone()))
            .collect();
    }
    let named: Vec<&str> = given(draft, command)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    command
        .parameters
        .iter()
        .filter(|parameter| !named.contains(&parameter.name.as_str()))
        .map(|parameter| Offer::Parameter(parameter.name.clone()))
        .collect()
}

/// `draft` with `offer` chosen: a parameter's `name=` written after it, or a
/// value written over what its last word typed after the `=`, and a space.
#[must_use]
pub fn completed(draft: &str, offer: &Offer) -> String {
    match offer {
        Offer::Parameter(name) => {
            let kept = draft.trim_end();
            if kept.is_empty() {
                format!("{name}=")
            } else {
                format!("{kept} {name}=")
            }
        }
        Offer::Value(value) => {
            let at = draft.rfind('=').map_or(draft.len(), |at| at + 1);
            format!("{}{value} ", &draft[..at])
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use nc_canon::Value;
    use nc_catalog::Locale;

    use super::{
        Command, Commands, Offer, Parameter, Texts, Type, arguments, completed, hits, offered,
        parameters_of, whole,
    };

    /// A command of `name`, its display name `title` in every locale.
    fn command(name: &str, title: &str, schema: &str) -> Command {
        Command {
            name: name.to_owned(),
            texts: Locale::ALL
                .iter()
                .map(|_| Texts {
                    title: title.to_owned(),
                    line: format!("{title}, in a line."),
                    tags: vec!["tag".to_owned()],
                })
                .collect(),
            parameters: parameters_of(schema),
        }
    }

    /// The fuzzy core's answer for the catalog's commands, built as
    /// `kernel.find` builds each item: its order is the list's.
    #[test]
    fn the_catalogs_commands_are_found_by_the_one_core_as_kernel_find_finds_them() {
        let commands = Commands::from_catalog();
        assert_eq!(commands.list().len(), nc_catalog::COMMANDS.len());
        let all = commands.found("", Locale::En);
        assert_eq!(
            all.iter().map(|found| found.index).collect::<Vec<_>>(),
            (0..commands.list().len()).collect::<Vec<_>>(),
            "an empty query lists every command, in the list's order"
        );
        assert!(all.iter().all(|found| found.hits.is_none()));
        let names = |typed: &str| -> Vec<&str> {
            commands
                .found(typed, Locale::En)
                .iter()
                .map(|found| commands.list()[found.index].name.as_str())
                .collect()
        };
        assert_eq!(
            names("page.next").first(),
            Some(&"page.next"),
            "a name typed whole"
        );
        for (typed, among) in [
            ("open settings", "page.settings"),
            ("configurações", "page.settings"),
            ("abrir ajuda", "page.help"),
        ] {
            assert!(names(typed).contains(&among), "{typed}: {:?}", names(typed));
        }
        assert_eq!(commands.found("zzzzqqqq", Locale::En).len(), 0);
        assert_eq!(
            commands.found(&"a".repeat(65), Locale::En).len(),
            0,
            "a query past the core's bound finds nothing"
        );
    }

    /// Each word marks the characters it matched on the display name the
    /// row shows, in its own slot — the first word's first — and a word that
    /// matched another field marks nothing there.
    #[test]
    fn matches_are_coloured_per_word_on_the_shown_name() {
        let commands = Commands::new(vec![command("a.b", "Open settings", "{}")]);
        let found = commands.found("open set", Locale::En);
        assert_eq!(found[0].hits.as_deref(), Some("0-3|5-7"));
        let found = commands.found("set open", Locale::En);
        assert_eq!(found[0].hits.as_deref(), Some("5-7|0-3"));
        let found = commands.found("a.b open", Locale::En);
        assert_eq!(found[0].hits.as_deref(), Some("|0-3"), "the dotted name");
        let found = commands.found("tag", Locale::En);
        assert_eq!(found[0].hits.as_deref(), Some(""), "a tag");
        let mut localized = command("a.b", "Open", "{}");
        localized.texts[1].title = "Abrir".to_owned();
        let commands = Commands::new(vec![localized]);
        assert_eq!(
            commands.found("abrir", Locale::PtBR)[0].hits.as_deref(),
            Some("0-4")
        );
        assert_eq!(
            commands.found("abrir", Locale::En)[0].hits.as_deref(),
            Some(""),
            "the other locale's name is not the one shown"
        );
    }

    /// A grapheme cluster is drawn whole: a mark after a hit character, a
    /// variation selector, and a character joined by a zero-width joiner.
    #[test]
    fn a_grapheme_cluster_is_drawn_whole() {
        assert_eq!(whole("cafe\u{301}", &[3]), [3, 4]);
        assert_eq!(whole("cafe\u{301}", &[4]), [3, 4]);
        assert_eq!(whole("abc", &[0, 2]), [0, 2]);
        assert_eq!(whole("a\u{2764}\u{FE0F}b", &[1]), [1, 2]);
        assert_eq!(whole("x\u{1F469}\u{200D}\u{1F4BB}y", &[1]), [1, 2, 3]);
        assert_eq!(whole("x\u{1F44D}\u{1F3FD}", &[1]), [1, 2]);
        assert_eq!(whole("ab", &[5]), Vec::<usize>::new());
        assert_eq!(
            hits("cafe\u{301} x", [&[3_usize][..], &[6][..]].into_iter()),
            "3-4|6"
        );
        assert_eq!(hits("abcd", [&[0_usize, 1, 3][..]].into_iter()), "0-1,3");
    }

    /// The registry's listings, one per locale, make a command of every name
    /// each holds, its texts each locale's.
    #[test]
    pub(crate) fn the_registrys_listings_make_one_command_of_each_locales_texts() {
        let listed = |name: &str, title: &str| {
            nc_ipc::client::Listed {
            name: name.to_owned(),
            display_name: title.to_owned(),
            line: format!("{title}."),
            paragraph: String::new(),
            tags: vec![title.to_lowercase()],
            categories: Vec::new(),
            args_schema: r#"{"type":"object","properties":{"n":{"type":"integer"}},"additionalProperties":false}"#.to_owned(),
        }
        };
        let commands = Commands::from_registry(&[
            vec![listed("p.a", "Alpha"), listed("p.b", "Beta")],
            vec![listed("p.a", "Alfa")],
        ]);
        assert_eq!(commands.list().len(), 1, "a name one listing lacks");
        let only = &commands.list()[0];
        assert_eq!(
            only.texts(Locale::En).map(|t| t.title.as_str()),
            Some("Alpha")
        );
        assert_eq!(
            only.texts(Locale::PtBR).map(|t| t.title.as_str()),
            Some("Alfa")
        );
        assert_eq!(only.parameters[0].kind, Type::Integer);
        assert_eq!(commands.found("alfa", Locale::PtBR).len(), 1);
        assert_eq!(Commands::from_registry(&[]).list().len(), 0);
    }

    /// A schema's parameters, required first and then by name, each with its
    /// type and its closed set of values.
    #[test]
    fn a_schemas_parameters_are_read_required_first() {
        let schema = nc_catalog::command("page.open")
            .expect("declared")
            .args_schema;
        let names: Vec<(String, bool)> = parameters_of(schema)
            .into_iter()
            .map(|parameter| (parameter.name, parameter.required))
            .collect();
        assert_eq!(
            names,
            [
                ("kind".to_owned(), true),
                ("in".to_owned(), false),
                ("topic".to_owned(), false),
                ("window".to_owned(), false),
            ]
        );
        let kind = &parameters_of(schema)[0];
        assert!(kind.choices.contains(&"settings".to_owned()));
        assert_eq!(
            parameters_of(r#"{"type":"object","properties":{"on":{"type":"boolean"}}}"#),
            [Parameter {
                name: "on".to_owned(),
                required: false,
                kind: Type::Boolean,
                choices: vec!["true".to_owned(), "false".to_owned()],
            }]
        );
        assert_eq!(parameters_of("not json").len(), 0);
        assert_eq!(parameters_of("{}").len(), 0);
    }

    /// The draft gives each parameter it names the text up to the next, read
    /// as its type where it reads as one; a command of one parameter takes
    /// the whole draft.
    #[test]
    fn the_draft_gives_each_parameter_it_names() {
        let open = command(
            "page.open",
            "Open",
            nc_catalog::command("page.open")
                .expect("declared")
                .args_schema,
        );
        assert_eq!(
            arguments("kind=help window=2", &open),
            Value::object([("kind", Value::text("help")), ("window", Value::Integer(2))])
        );
        assert_eq!(
            arguments("window=two kind=help", &open),
            Value::object([
                ("kind", Value::text("help")),
                ("window", Value::text("two"))
            ]),
            "a value that is no whole number is the kernel's to refuse"
        );
        assert_eq!(
            arguments("stray kind=help", &open),
            Value::object([("kind", Value::text("help"))])
        );
        let say = command(
            "info.say",
            "Say",
            nc_catalog::command("info.say")
                .expect("declared")
                .args_schema,
        );
        assert_eq!(
            arguments(" Saved, now. ", &say),
            Value::object([("text", Value::text("Saved, now."))])
        );
        assert_eq!(
            arguments("text=two words", &say),
            Value::object([("text", Value::text("two words"))])
        );
        assert_eq!(
            arguments("   ", &say),
            Value::object(Vec::<(&str, Value)>::new())
        );
        let switch = command(
            "s.set",
            "Set",
            r#"{"type":"object","properties":{"on":{"type":"boolean"}},"required":["on"]}"#,
        );
        assert_eq!(
            arguments("true", &switch),
            Value::object([("on", Value::Bool(true))])
        );
    }

    /// What is offered: the parameters not named yet, then — while a
    /// parameter with a closed set is typed — its values that begin with what
    /// was typed; choosing one completes the draft.
    #[test]
    fn the_parameters_and_their_values_are_offered_and_completed() {
        let open = command(
            "page.open",
            "Open",
            nc_catalog::command("page.open")
                .expect("declared")
                .args_schema,
        );
        assert_eq!(
            offered("", &open),
            ["kind", "in", "topic", "window"].map(|name| Offer::Parameter(name.to_owned()))
        );
        assert_eq!(
            offered("kind=help ", &open),
            ["in", "topic", "window"].map(|name| Offer::Parameter(name.to_owned()))
        );
        assert_eq!(
            offered("kind=se", &open),
            [Offer::Value("settings".to_owned())]
        );
        assert_eq!(
            offered("window=", &open),
            ["kind", "in", "topic"].map(|name| Offer::Parameter(name.to_owned())),
            "a parameter with no closed set offers the rest"
        );
        assert_eq!(completed("", &Offer::Parameter("kind".to_owned())), "kind=");
        assert_eq!(
            completed("kind=help  ", &Offer::Parameter("in".to_owned())),
            "kind=help in="
        );
        assert_eq!(
            completed("kind=se", &Offer::Value("settings".to_owned())),
            "kind=settings "
        );
        assert_eq!(Offer::Parameter("in".to_owned()).value(), "parameter:in");
        assert_eq!(Offer::Value("help".to_owned()).value(), "value:help");
    }
}
