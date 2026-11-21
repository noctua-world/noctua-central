//! `kernel.find`: the commands a query finds, matched by the one fuzzy core.
//!
//! **The registry filtered as every list is** (`NOCTUA.md §12.4`). Every
//! command the kernel can dispatch — its own and every loaded plugin's — is an
//! `nc_fuzzy::Item` of its fields, in this order: its dotted name, its display
//! name in each locale of `Locale::ALL`, then its tags in each locale, synonyms
//! included. A name and a display name are `Role::Name`, a tag `Role::Tag`, so
//! a person typing in either language finds the same command, and a name
//! outranks a tag at equal quality. Descriptions are not matched: §12.4 names
//! the name and the tags. The texts are read through the entry the registry
//! lists (`crate::registry::CommandEntry`): the catalog's for a command the
//! catalog writes, the plugin's own, judged at `activate`, for one it does not.
//!
//! **What it answers**, as canonical JSON: `found`, the commands the query
//! matched, best first and at most `limit` of them — each with its `name`, its
//! `owner` (`kernel` or `plugin`), its packed `score` and, for each word of the
//! query in the order typed, the `field` it matched (`name`, `display_name` or
//! `tag`), the `locale` of a display name or a tag, that field's `text` and the
//! `positions` of the characters matched in it, each a `char` index — and
//! `total`, how many the query matched in all. An empty query matches every
//! command, in the registry's order. Equal scores keep the registry's order,
//! which is name order, so the answer is stable while a person types.
//!
//! **Decided as every command is**: `Read` on `Feature::"registry"`, which the
//! family pack gives every person of the realm
//! (`family.a_member_finds_commands`). It reads nothing of the realm's and
//! changes nothing, so it opens no unit, and the journal's entry names the
//! command and the feature, never the query.
//!
//! **An agent finds what its session may dispatch now**: for a call tree
//! acting for an agent, the registry is cut to the decision point's probe
//! before anything is matched (`crate::session::dispatchable`) — a command
//! whose resource is known without arguments, that is not one that cannot be
//! undone, and that the agent and every one it acts for may be allowed — so its
//! search answers the set its tool list is. A person finds every registered
//! command.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! A query costs the daemon bounded work: the schema refuses one past
//! `nc_fuzzy::QUERY_CHARS_MAX` characters before this runs, and this refuses
//! it again rather than read it; every field is read to
//! `nc_fuzzy::FIELD_CHARS_MAX` characters, which every text a registry may
//! hold fits within; the registry's lock is let go before anything is matched,
//! so no load or dispatch waits on a match; and the answer lists at most
//! [`LISTED_MAX`] commands, whose widest fits one frame. It shows nothing the
//! registry does not already show the same caller, and an agent nothing its
//! session may not dispatch. **What it does not answer**: how many commands
//! the registry holds, which is the plugins a person installed, each held to
//! `commands_per_plugin`; which commands a person may dispatch — every
//! registered command is found for them; and an agent's command that takes
//! its resource from an argument, which its search does not find though a
//! dispatch of it is decided as any.

use nc_canon::Value;
use nc_catalog::{ErrorCode, Locale};
use nc_fuzzy::{Item, Match, Matcher, Query, Role};

use crate::dispatch::{Failure, Refusal, RefusalKind};
use crate::registry::{Owner, RegisteredCommand};

/// The command that finds commands.
pub(crate) const FIND: &str = "kernel.find";

/// The words a person typed, which every command found matches.
pub(crate) const QUERY: &str = "query";

/// How many of the commands found the answer lists.
pub(crate) const LIMIT: &str = "limit";

/// The most commands an answer lists.
///
/// As many are listed when `limit` is not given. It is the schema's `maximum`,
/// which a test holds equal, and the bound a test holds the widest answer to
/// within one frame: raising one without the other fails there.
pub(crate) const LISTED_MAX: usize = 32;

/// What a field of a command is, as an answer names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The dotted name the command is dispatched by.
    Name,
    /// The name a person reads first, in one locale.
    DisplayName,
    /// A word a search finds the command by, in one locale.
    Tag,
}

impl Kind {
    /// The role the matcher weighs a field of this kind by.
    const fn role(self) -> Role {
        match self {
            Self::Name | Self::DisplayName => Role::Name,
            Self::Tag => Role::Tag,
        }
    }

    /// The word an answer names it by.
    const fn word(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::DisplayName => "display_name",
            Self::Tag => "tag",
        }
    }
}

/// One field of a command the matcher reads.
#[derive(Debug, Clone, Copy)]
struct Field<'a> {
    /// What it is.
    kind: Kind,
    /// The locale it is written in, which a dotted name has none of.
    locale: Option<Locale>,
    /// Its text, as the registry lists it.
    text: &'a str,
}

/// The fields of `command`, in the order its item is built with.
///
/// The dotted name, its display name in each locale, then its tags in each
/// locale: an answer reads a word's field back by its index here.
fn fields_of(command: &RegisteredCommand) -> Vec<Field<'_>> {
    let mut fields = vec![Field {
        kind: Kind::Name,
        locale: None,
        text: command.name.as_str(),
    }];
    for &locale in Locale::ALL {
        fields.push(Field {
            kind: Kind::DisplayName,
            locale: Some(locale),
            text: command.entry.in_locale(locale).display_name.as_str(),
        });
    }
    for &locale in Locale::ALL {
        for tag in &command.entry.in_locale(locale).tags {
            fields.push(Field {
                kind: Kind::Tag,
                locale: Some(locale),
                text: tag.as_str(),
            });
        }
    }
    fields
}

/// `kernel.find`: the commands of `listed` the query `args` holds finds.
///
/// It answers at most `limit` of them, best first, and how many it found in
/// all. `listed` is the registry as it was when the dispatch read it, taken
/// before this runs, so nothing here holds the registry's lock.
///
/// # Errors
///
/// `invalid`, with the code the schema refuses arguments with, when the query
/// is longer than `nc_fuzzy::QUERY_CHARS_MAX` characters: the schema refuses
/// it first, and this does not read past the bound either.
pub(crate) fn found(
    listed: &[RegisteredCommand],
    args: &serde_json::Value,
) -> Result<String, Failure> {
    let typed = args
        .get(QUERY)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let query = Query::parse(typed).map_err(|_| too_long())?;
    let most = args
        .get(LIMIT)
        .and_then(serde_json::Value::as_u64)
        .map_or(LISTED_MAX, |asked| {
            usize::try_from(asked).map_or(LISTED_MAX, |asked| asked.min(LISTED_MAX))
        });
    let fields: Vec<Vec<Field<'_>>> = listed.iter().map(fields_of).collect();
    let items: Vec<Item> = fields
        .iter()
        .map(|each| Item::new(each.iter().map(|field| (field.kind.role(), field.text))))
        .collect();
    let ranked = Matcher::new().ranked(&query, &items);
    let total = ranked.len();
    let shown: Vec<Value> = ranked
        .iter()
        .take(most)
        .map(|one| answer_of(&listed[one.index()], &fields[one.index()], one.found()))
        .collect();
    Ok(Value::object([("found", Value::Array(shown)), ("total", count(total))]).to_canonical())
}

/// What an answer says of `command`, found by `matched` in its `fields`.
fn answer_of(command: &RegisteredCommand, fields: &[Field<'_>], matched: &Match) -> Value {
    let words = matched
        .hits()
        .iter()
        .map(|hit| {
            let field = fields[hit.field()];
            let mut said = vec![
                ("field", Value::text(field.kind.word())),
                (
                    "positions",
                    Value::Array(hit.positions().iter().map(|&at| count(at)).collect()),
                ),
                ("text", Value::text(field.text)),
            ];
            if let Some(locale) = field.locale {
                said.push(("locale", Value::text(locale.tag())));
            }
            Value::object(said)
        })
        .collect();
    Value::object([
        ("name", Value::text(command.name.as_str())),
        (
            "owner",
            Value::text(match command.owner {
                Owner::Kernel => "kernel",
                Owner::Plugin(_) => "plugin",
            }),
        ),
        (
            "score",
            Value::Integer(i64::try_from(matched.score().packed()).unwrap_or(i64::MAX)),
        ),
        ("words", Value::Array(words)),
    ])
}

/// A count or an index, as an answer writes it.
fn count(counted: usize) -> Value {
    Value::Integer(i64::try_from(counted).unwrap_or(i64::MAX))
}

/// The failure of a query past its bound, which the schema refuses first.
fn too_long() -> Failure {
    Failure::new(
        Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationArgumentsInvalid,
            format!(
                "the `{QUERY}` of `{FIND}` holds more than the {} characters a query may",
                nc_fuzzy::QUERY_CHARS_MAX
            ),
        ),
        "query-too-long",
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nc_canon::Value;
    use nc_catalog::{ErrorCode, KERNEL};
    use nc_schema::descriptor::Reversibility;

    use super::{FIND, LIMIT, LISTED_MAX, QUERY, found};
    use crate::PluginSlot;
    use crate::dispatch::RefusalKind;
    use crate::registry::{CommandEntry, EntryTexts, Owner, RegisteredCommand, Registry};

    /// Every argument `kernel.find` reads, by the name the command declares.
    const FIND_READS: [&str; 2] = [QUERY, LIMIT];

    /// A command of `owner`, shown by the same texts in every locale.
    fn command(name: &str, display_name: &str, tags: &[&str], owner: Owner) -> RegisteredCommand {
        RegisteredCommand {
            name: name.to_owned(),
            entry: CommandEntry::new(
                vec![EntryTexts {
                    display_name: display_name.to_owned(),
                    line: String::new(),
                    paragraph: String::new(),
                    tags: tags.iter().map(|&tag| tag.to_owned()).collect(),
                }],
                Vec::new(),
                None,
            ),
            args_schema: "{}".to_owned(),
            mutating: false,
            action: "Read".to_owned(),
            resource_selector: "realm".to_owned(),
            reversibility: Reversibility::Read,
            owner,
        }
    }

    /// What `found` answers over `listed` for `args`, read back.
    fn answer(listed: &[RegisteredCommand], args: &serde_json::Value) -> Value {
        let text = found(listed, args).unwrap_or_else(|failure| panic!("refused: {failure:?}"));
        let value = nc_canon::parse(&text).expect("the answer is JSON");
        assert_eq!(value.to_canonical(), text, "the answer is canonical");
        value
    }

    /// The field `key` of `value`.
    fn at(value: &Value, key: &str) -> Value {
        value
            .clone()
            .into_object()
            .and_then(|mut fields| fields.remove(key))
            .unwrap_or_else(|| panic!("no `{key}` in {value:?}"))
    }

    /// The list `key` of `value`.
    fn list(value: &Value, key: &str) -> Vec<Value> {
        at(value, key)
            .into_array()
            .unwrap_or_else(|| panic!("`{key}` of {value:?} is not a list"))
    }

    /// The text `key` of `value`.
    fn text(value: &Value, key: &str) -> String {
        at(value, key)
            .into_text()
            .unwrap_or_else(|| panic!("`{key}` of {value:?} is not text"))
    }

    /// The names the answer `value` found, in its order.
    fn names(value: &Value) -> Vec<String> {
        list(value, "found")
            .iter()
            .map(|one| text(one, "name"))
            .collect()
    }

    /// How many the answer `value` says it found in all.
    fn total(value: &Value) -> Value {
        at(value, "total")
    }

    /// One word of a command found, as `(field, locale, text, positions)`.
    fn word(said: &Value) -> (String, Option<String>, String, Vec<i64>) {
        let locale = said
            .clone()
            .into_object()
            .and_then(|mut fields| fields.remove("locale"))
            .map(|locale| locale.into_text().expect("a locale is text"));
        let positions = list(said, "positions")
            .into_iter()
            .map(|at| match at {
                Value::Integer(at) => at,
                other => panic!("a position is an integer: {other:?}"),
            })
            .collect();
        (text(said, "field"), locale, text(said, "text"), positions)
    }

    /// The positions `from..to`, as an answer writes them.
    fn run(from: i64, to: i64) -> Vec<i64> {
        (from..to).collect()
    }

    /// The query `typed`, with no limit, as the arguments a dispatch hands in.
    fn asking(typed: &str) -> serde_json::Value {
        serde_json::json!({ QUERY: typed })
    }

    /// A Portuguese query finds a command through its Portuguese tag.
    ///
    /// **Over the catalog's own registry** (Stage 14, acceptance 5): `procurar`
    /// is one of `kernel.find`'s Portuguese tags and is in none of its English
    /// texts, so the word reports the tag, its locale and its whole text; and
    /// `comando` is a run of the Portuguese display name, which a name holds
    /// before a tag that holds the same run.
    #[test]
    fn a_portuguese_query_finds_a_command_through_its_portuguese_tag() {
        let listed = Registry::from_catalog().listed();
        let answered = answer(&listed, &asking("procurar comando"));
        let first = list(&answered, "found")
            .into_iter()
            .next()
            .expect("something is found");
        assert_eq!(text(&first, "name"), FIND, "{answered:?}");
        assert_eq!(text(&first, "owner"), "kernel");
        let words: Vec<_> = list(&first, "words").iter().map(word).collect();
        assert_eq!(
            words,
            [
                (
                    "tag".to_owned(),
                    Some("pt-BR".to_owned()),
                    "procurar".to_owned(),
                    run(0, 8)
                ),
                (
                    "display_name".to_owned(),
                    Some("pt-BR".to_owned()),
                    "Encontrar um comando".to_owned(),
                    run(13, 20)
                ),
            ]
        );
        assert!(
            matches!(at(&first, "score"), Value::Integer(score) if score > 0),
            "{first:?}"
        );
    }

    /// An English query finds a command through its English display name.
    ///
    /// Both words lie in `kernel.find`'s English display name, each at its own
    /// positions — the second past the space and the article, which a run that
    /// leaked the matcher's copy without separators would answer two short.
    #[test]
    fn an_english_query_finds_a_command_through_its_english_display_name() {
        let listed = Registry::from_catalog().listed();
        let answered = answer(&listed, &asking("find command"));
        let first = list(&answered, "found")
            .into_iter()
            .next()
            .expect("something is found");
        assert_eq!(text(&first, "name"), FIND, "{answered:?}");
        let words: Vec<_> = list(&first, "words").iter().map(word).collect();
        let shown = |positions| {
            (
                "display_name".to_owned(),
                Some("en".to_owned()),
                "Find a command".to_owned(),
                positions,
            )
        };
        assert_eq!(words, [shown(run(0, 4)), shown(run(7, 14))]);
    }

    /// A command one word cannot match is not found.
    ///
    /// Every word must match: `procurar` alone finds `kernel.find`, and with a
    /// word no text of any command holds, nothing is found and nothing counted.
    #[test]
    fn a_command_one_word_cannot_match_is_not_found() {
        let listed = Registry::from_catalog().listed();
        assert!(names(&answer(&listed, &asking("procurar"))).contains(&FIND.to_owned()));
        for typed in ["procurar zzz", "find zzz"] {
            let answered = answer(&listed, &asking(typed));
            assert_eq!(names(&answered), Vec::<String>::new(), "{typed}");
            assert_eq!(total(&answered), Value::Integer(0), "{typed}");
        }
    }

    /// The answer holds at most the limit, and counts every match.
    ///
    /// **The total is of everything found, not of what is listed**, and the
    /// list is cut to `limit`, or to [`LISTED_MAX`] when none is given — and to
    /// that whatever `limit` says, so a schema widened alone widens no answer.
    #[test]
    fn the_answer_holds_at_most_the_limit_and_counts_every_match() {
        let many = LISTED_MAX + 8;
        let listed: Vec<RegisteredCommand> = (0..many)
            .map(|at| command(&format!("x.c{at:03}"), "Zeta", &["zeta"], Owner::Kernel))
            .chain([command("x.other", "Other", &["other"], Owner::Kernel)])
            .collect();
        let found_in_all = Value::Integer(i64::try_from(many).expect("fits"));
        for (limit, listed_at_most) in [
            (Some(3_u64), 3),
            (None, LISTED_MAX),
            (Some(1_000), LISTED_MAX),
        ] {
            let mut args = asking("zeta");
            if let Some(limit) = limit {
                args[LIMIT] = serde_json::json!(limit);
            }
            let answered = answer(&listed, &args);
            assert_eq!(names(&answered).len(), listed_at_most, "{limit:?}");
            assert_eq!(total(&answered), found_in_all, "{limit:?}");
        }
        let everything = answer(&listed, &asking(""));
        assert_eq!(
            total(&everything),
            Value::Integer(i64::try_from(listed.len()).expect("fits")),
            "the empty query finds every command"
        );
        assert_eq!(names(&everything).len(), LISTED_MAX);
    }

    /// The answer keeps the registry's order among equal scores.
    ///
    /// Three commands every word matches alike are answered in the order the
    /// registry listed them, which need not be the order of their names, and
    /// a better match listed after them comes first; the empty query lists
    /// every command in the registry's order.
    #[test]
    fn the_answer_keeps_the_registrys_order_among_equal_scores() {
        let listed = [
            command("x.gamma", "The zeta", &["one"], Owner::Kernel),
            command("x.alpha", "The zeta", &["two"], Owner::Kernel),
            command("x.beta", "The zeta", &["three"], Owner::Kernel),
            command("x.last", "Zeta", &["four"], Owner::Plugin(PluginSlot(0))),
        ];
        assert_eq!(
            names(&answer(&listed, &asking("zeta"))),
            ["x.last", "x.gamma", "x.alpha", "x.beta"],
            "a display name typed whole ranks first, and the rest keep their order"
        );
        let first = list(&answer(&listed, &asking("zeta")), "found")
            .into_iter()
            .next()
            .expect("found");
        assert_eq!(text(&first, "owner"), "plugin");
        assert_eq!(
            names(&answer(&listed, &asking(""))),
            ["x.gamma", "x.alpha", "x.beta", "x.last"]
        );
    }

    /// A command found by its dotted name outranks one found by a tag.
    ///
    /// **The dotted name is weighed as a name**, as a display name is: the
    /// word `zeta` lies in each command at a word start, at the same place in
    /// a field of the same length, and only what that field is differs — the
    /// one listed second has it in its dotted name, the first in a tag — so
    /// the name comes first, and its word names no locale.
    #[test]
    fn a_command_found_by_its_dotted_name_outranks_one_found_by_a_tag() {
        let listed = [
            command("x.alpha", "Other", &["x zeta"], Owner::Kernel),
            command("x.zeta", "Other", &["other"], Owner::Kernel),
        ];
        let answered = answer(&listed, &asking("zeta"));
        assert_eq!(names(&answered), ["x.zeta", "x.alpha"], "{answered:?}");
        let words: Vec<_> = list(&answered, "found")
            .iter()
            .map(|one| list(one, "words").iter().map(word).collect::<Vec<_>>())
            .collect();
        assert_eq!(
            words,
            [
                [("name".to_owned(), None, "x.zeta".to_owned(), run(2, 6))],
                [(
                    "tag".to_owned(),
                    Some("en".to_owned()),
                    "x zeta".to_owned(),
                    run(2, 6)
                )],
            ]
        );
    }

    /// The declared `kernel.find` is the one this module implements.
    ///
    /// **Both ways**, against `Command::arguments`, and the decision it is made
    /// under: `Read` on the platform's `registry` feature, changing nothing —
    /// so no unit is opened, and the permit that decides it is a person's.
    #[test]
    fn the_declared_find_is_the_one_this_module_implements() {
        let declared = nc_catalog::command(FIND).expect("declared");
        let takes: BTreeSet<&str> = declared.arguments.iter().copied().collect();
        let reads: BTreeSet<&str> = FIND_READS.iter().copied().collect();
        assert_eq!(takes, reads, "`{FIND}` declares {:?}", declared.arguments);
        assert_eq!(declared.arguments.len(), FIND_READS.len());
        assert_eq!(
            (
                declared.action,
                declared.resource_selector,
                declared.mutating
            ),
            ("Read", "feature:registry", false)
        );
        assert!(matches!(
            declared.reversibility,
            nc_catalog::Reversibility::Read
        ));
        assert_eq!((FIND, QUERY, LIMIT), ("kernel.find", "query", "limit"));
    }

    /// The bound `keyword` of the argument `property` of `kernel.find`'s schema.
    fn bound(property: &str, keyword: &str) -> u64 {
        let declared = nc_catalog::command(FIND).expect("declared");
        let schema: serde_json::Value =
            serde_json::from_str(declared.args_schema).expect("a schema is JSON");
        schema["properties"][property][keyword]
            .as_u64()
            .unwrap_or_else(|| panic!("`{FIND}` declares no {keyword} on {property}"))
    }

    /// The query bound the schema declares is the matcher's.
    ///
    /// The schema refuses a query past `maxLength` before the handler runs,
    /// and the matcher refuses one past `nc_fuzzy::QUERY_CHARS_MAX`: the two
    /// are one bound, so the handler is never handed a query it refuses, and
    /// one it would refuse is refused as invalid rather than read. The limit
    /// the schema allows is the most the handler lists.
    #[test]
    fn the_query_bound_the_schema_declares_is_the_matchers() {
        assert_eq!(
            bound(QUERY, "maxLength"),
            u64::try_from(nc_fuzzy::QUERY_CHARS_MAX).expect("fits")
        );
        assert_eq!(
            bound(LIMIT, "maximum"),
            u64::try_from(LISTED_MAX).expect("fits")
        );
        assert_eq!(bound(LIMIT, "minimum"), 1);
        let listed = Registry::from_catalog().listed();
        let widest = "é".repeat(nc_fuzzy::QUERY_CHARS_MAX);
        answer(&listed, &asking(&widest));
        let refused = found(&listed, &asking(&format!("{widest}e")))
            .expect_err("a query past its bound is refused");
        assert_eq!(
            (refused.refusal().kind(), refused.refusal().code()),
            (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid),
            "{refused:?}"
        );
    }

    /// The widest answer fits one frame.
    ///
    /// **The worst a registry can be made to answer**: [`LISTED_MAX`]
    /// commands, each found by a query of as many one-character words as its
    /// bound holds, every word echoing a whole field of at least
    /// `nc_fuzzy::FIELD_CHARS_MAX` characters of the character canonical JSON
    /// writes longest (equal words claim one field, the dotted name) — more
    /// than any text a registry holds, so this bounds every answer. It must be
    /// a payload the kernel hands on, which the compiler holds within one frame
    /// with its envelope.
    #[test]
    fn the_widest_answer_fits_one_frame() {
        let words = nc_fuzzy::WORDS_MAX;
        // A control character, which canonical JSON writes as a six-byte
        // escape and no judge lets a registry hold, so no text here is
        // narrower than one a registry may; every word echoes a whole field.
        let mark = '\u{1}';
        let widest: String = std::iter::repeat_n(mark, nc_fuzzy::FIELD_CHARS_MAX).collect();
        let listed: Vec<RegisteredCommand> = (0..LISTED_MAX * 2)
            .map(|at| {
                command(
                    &format!("{widest}{at:03}"),
                    &widest,
                    &[widest.as_str()],
                    Owner::Kernel,
                )
            })
            .collect();
        let typed = vec![mark.to_string(); words].join(" ");
        assert!(typed.chars().count() <= nc_fuzzy::QUERY_CHARS_MAX);
        let answered = answer(&listed, &asking(&typed));
        assert_eq!(names(&answered).len(), LISTED_MAX);
        for one in list(&answered, "found") {
            assert_eq!(list(&one, "words").len(), words, "every word matched");
        }
        let bytes = answered.to_canonical().len();
        assert!(
            u64::try_from(bytes).expect("fits") <= KERNEL.payload_bytes,
            "the widest answer is {bytes} bytes, past the {} a payload may be",
            KERNEL.payload_bytes
        );
    }

    /// Every text a registry may hold is read whole by the matcher.
    ///
    /// A command's dotted name, a display name and a tag are each held to a
    /// bound — by the descriptor judge and the entry judge, at the build and
    /// at `activate` — within the characters the matcher reads of a field, so
    /// no part of a text a registry holds is beyond a search: a word matching
    /// only the last character of each, at its bound, finds it there.
    #[test]
    fn every_text_a_registry_may_hold_is_read_whole_by_the_matcher() {
        for (bound, what) in [
            (nc_schema::descriptor::NAME_MAX, "a command's name"),
            (nc_schema::entry::DISPLAY_NAME_MAX, "a display name"),
            (nc_schema::entry::TAG_MAX, "a tag"),
        ] {
            assert!(
                bound <= nc_fuzzy::FIELD_CHARS_MAX,
                "{what} may hold {bound} characters, past the {} the matcher reads",
                nc_fuzzy::FIELD_CHARS_MAX
            );
        }
        let longest = |bound: usize| format!("{}q", "a".repeat(bound - 1));
        let name = format!("x.{}", &longest(nc_schema::descriptor::NAME_MAX)[2..]);
        let listed = [command(
            &name,
            &longest(nc_schema::entry::DISPLAY_NAME_MAX).replace('q', "w"),
            &[&longest(nc_schema::entry::TAG_MAX).replace('q', "z")],
            Owner::Kernel,
        )];
        for (typed, field, last) in [
            ("q", "name", nc_schema::descriptor::NAME_MAX),
            ("w", "display_name", nc_schema::entry::DISPLAY_NAME_MAX),
            ("z", "tag", nc_schema::entry::TAG_MAX),
        ] {
            let answered = answer(&listed, &asking(typed));
            let first = list(&answered, "found")
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("`{typed}` found nothing"));
            let (said, _, _, positions) = word(&list(&first, "words")[0]);
            assert_eq!(said, field, "{typed}");
            assert_eq!(
                positions,
                [i64::try_from(last - 1).expect("fits")],
                "{typed}"
            );
        }
    }
}
