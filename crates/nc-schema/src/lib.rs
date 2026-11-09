//! The closed subset of JSON Schema a command's arguments are declared in.
//!
//! `NOCTUA.md §11.4` makes a command's argument schema its AI tool interface,
//! and `§11.7` makes the kernel refuse a descriptor it cannot use. This crate is
//! the one rule both need: [`Schema`] compiles and validates the subset,
//! [`selector`] reads what a command acts on out of its arguments, and
//! [`descriptor::judge`] holds a whole declaration to both.
//!
//! **Two callers, one rule.** The catalog compiler judges every command
//! `catalog/commands.toml` declares, at build time; `§11.7` has the kernel
//! judge every command a plugin declares, at `activate`, which
//! `crates/nc-kernel/src/registry.rs` does. Written twice, the two would agree
//! until the day a keyword was accepted by one and refused by the other — and
//! the one that accepted it would be the one an agent reads.
//!
//! # The subset
//!
//! ```text
//! type                  object | string | integer | boolean | array, required everywhere
//! object                additionalProperties: false (required), properties, required ⊆ properties
//! string                minLength, maxLength (characters), enum (distinct strings), x-entity
//! integer               minimum, maximum (i64)
//! array                 items (required), minItems, maxItems
//! everywhere            description, title (annotations, never validated against)
//! ```
//!
//! `x-entity` is this platform's own keyword: it names the Cedar entity type a
//! string argument identifies, which is what a `argument:<name>` selector reads.
//! The annotations decide nothing, and are kept for [`Schema::arguments`], the
//! view a command line is read through and help is printed from — beside each
//! property, with the options of an `enum` in the order written, and outside
//! the equality two schemas are compared by.
//!
//! # What this answers (`§2.4`)
//!
//! **An unknown keyword is refused when a schema is compiled, never skipped
//! when an instance is validated.** A validator that ignores what it does not
//! implement accepts everything the author believed `pattern`, `oneOf` or
//! `$ref` refused, and the author finds out from an incident. So the subset is
//! closed: every keyword is in [`KEYWORDS`], each belongs to one type, and one
//! written on the wrong type is refused rather than read as a no-op.
//!
//! **An object is closed too.** `additionalProperties: false` is required, so
//! an argument nobody declared never reaches the code that runs the command.
//!
//! **The walk's depth is bounded by the schema and its breadth by the
//! arguments**, whose size `nc-canon` bounds before this runs. Validation
//! descends no further than the compiled schema, which nests at most
//! [`MAX_SCHEMA_DEPTH`] deep, and it walks the keys the arguments hold, never
//! the names the schema declares. A key is looked up in an index and a string
//! in its enum's set, and compiling builds both with one lookup per name or
//! option, so nothing costs the product of a schema's names and anything
//! else. The required names are walked for an object that holds them all,
//! which has as many keys, or for one that lacks some, which adds to the list;
//! and a refusal lists at most [`MAX_VIOLATIONS`] violations, after which the
//! walk descends no further — so a megabyte of wrong items costs a short list,
//! not a megabyte of diagnostics.
//!
//! **A name from outside is repeated escaped and cut short**, in a fault, in a
//! violation, in the pointer beside either and in a selector's refusal. A
//! schema's names and a selector's text are whatever its author wrote — a
//! plugin's, at `activate` — and a diagnostic is printed to a terminal, where
//! a control character moves the cursor, a direction override turns the rest
//! of the line around and a megabyte of name is a megabyte of output.
//!
//! **A key written twice in a schema is refused.** A reader that keeps the
//! second of two `additionalProperties` and a reader that keeps the first
//! would be two tool interfaces for one text.
//!
//! **A schema's text holds no character a reader sees nothing of.** The schema
//! is the command's AI tool interface, and a direction override, a run of tag
//! characters or a run of variation selectors reads one way to the person
//! reviewing it and another to the agent it is handed to. [`INVISIBLE_RANGES`]
//! and [`BLANK_RANGES`] are the tables of them, together [`unseen`] — the
//! catalog compiler's escapers read the first, and its untranslated rule both
//! — and JSON's own whitespace, `\n`, `\r` and `\t`, is all the schema rule
//! exempts from them.
//!
//! **A character spelled as a JSON escape is refused once it is decoded**, in
//! a title, a description, a property's name and an `enum`'s option:
//! `\u202e` is six characters a reviewer sees in the text, which the rule
//! above reads, and the string it parses to holds the override itself, which
//! help would print to a terminal — `\u001b[2J` in a description clears the
//! screen of whoever asks what a command takes. Every character [`unseen`]
//! answers for is refused there, JSON's whitespace included, which inside a
//! string is a line break a printer would start a forged line with.
//!
//! # What this deliberately does not answer
//!
//! Whether an identifier names an entity that exists is the decision point's
//! question, and an unknown one is denied by default there. The size of the
//! arguments and their parse — duplicate keys, fractions, depth — are `nc-canon`'s,
//! before anything here runs: this crate is handed a value.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod descriptor;
pub mod selector;

pub use descriptor::{
    Declared, DescriptorProblem, Field, Judged, NAME_MAX, Reversibility, TITLE_MAX,
    command_name_is_well_formed, judge,
};
pub use selector::{ENTITY_ID_MAX, EntityRef, ExtractError, Selector, SelectorError};

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

/// Every keyword the subset holds, each beside the one type it belongs to.
///
/// `None` is a keyword every type may carry: `type` itself and the two
/// annotations. [`KEYWORDS`] is read out of this table rather than written a
/// second time, so the two cannot disagree about what exists.
const OWNED_BY: [(&str, Option<Kind>); 15] = [
    ("type", None),
    ("description", None),
    ("title", None),
    ("properties", Some(Kind::Object)),
    ("required", Some(Kind::Object)),
    ("additionalProperties", Some(Kind::Object)),
    ("minLength", Some(Kind::String)),
    ("maxLength", Some(Kind::String)),
    ("enum", Some(Kind::String)),
    ("x-entity", Some(Kind::String)),
    ("minimum", Some(Kind::Integer)),
    ("maximum", Some(Kind::Integer)),
    ("items", Some(Kind::Array)),
    ("minItems", Some(Kind::Array)),
    ("maxItems", Some(Kind::Array)),
];

/// Every keyword a schema may use; anything else is refused at compile.
pub const KEYWORDS: [&str; 15] = {
    let mut names = [""; 15];
    let mut at = 0;
    while at < OWNED_BY.len() {
        names[at] = OWNED_BY[at].0;
        at += 1;
    }
    names
};

/// How deep schemas may nest, counting the top one.
///
/// Sixteen is far past any argument list a person would write and far below
/// what a recursive walk can be made to cost; it is also what bounds
/// [`Schema::validate`], which never descends further than the schema does.
pub const MAX_SCHEMA_DEPTH: usize = 16;

/// The most violations one refusal lists, in the order they were found.
///
/// A caller needs one to refuse and a person a handful to fix, and an array of
/// a million wrong items would otherwise be a million diagnostics.
pub const MAX_VIOLATIONS: usize = 64;

/// How many characters of a name a diagnostic repeats before it elides.
///
/// A property name comes from whoever wrote the arguments, and a refusal that
/// echoes a megabyte back to them has turned one bad request into two.
const EXCERPT: usize = 64;

/// Every code point a reader sees nothing of, as inclusive ranges.
///
/// **Here, because a schema is refused by it, and one table for every reader.**
/// In the catalog compiler alone it would hold a command's argument schema and
/// not a plugin's: the kernel judges a plugin's schema through [`judge`], and a
/// check outside it lets a direction override and a run of tag characters
/// pass there. The compiler re-exports
/// this array and [`shows_nothing`] rather than keeping a copy, for the reason
/// the next paragraph gives.
///
/// **One list, not two that disagree.** A sweep
/// in `crates/nc-catalog-compiler/tests/the_gates_fire.rs` that transcribed
/// the set a second time and iterated a third set of ranges would reach none
/// of `U+110BD`/`U+110CD`, `U+00AD`, `U+180E`, `U+FFF9`–`U+FFFB`
/// or `U+1D17B`–`U+1D182`, leaving those fifteen code points decided by
/// nothing — measured, all of them deleted from [`shows_nothing`]
/// and `cargo test -p nc-catalog-compiler` green. With the Kaithi pair
/// gone, `catalog/i18n/pt-BR.toml`'s `error.journal.chain-broken`
/// — `NC-JRNL-0001`, critical, non-retryable, the journal-tampering message —
/// filed as the **English** sentence plus `U+110BD` is accepted
/// by `cargo xtask build`, publishes that English sentence in the pt-BR column
/// of `docs/errors.md`, and reaches the generated crate carrying the character
/// as itself. That is the `U+070F` case below, one plane up.
///
/// A hand list bound to another hand list is not a check, so there is one list:
/// [`shows_nothing`] answers from it and the sweep iterates it. What the sweep
/// proves is then a fact about each character — that it reaches no doc line, no
/// string literal, no published cell and not the reservation ledger — rather
/// than a fact about the predicate.
///
/// **The eleven `Cf` points a hand transcription of `Cc` and `Cf` can miss**,
/// the Arabic number and sign marks among them — measured with one that did:
/// `U+070F` appended to the English sentence and filed as the pt-BR
/// translation of `NC-JRNL-0001` is accepted by `cargo xtask build`,
/// `docs/errors.md` publishes the identical English sentence in both language
/// columns, and the character reaches a generated Rust string literal as
/// itself. The same plant with `U+2063` —
/// which that list did name — is refused. Byte-identical sentences, opposite
/// verdicts, decided by one code point.
///
/// **Only what a reader sees nothing of.** Widening the ranges to make the sweep
/// look thorough is the opposite error: `U+2010` is a hyphen and belongs in the
/// output. Ordered and non-overlapping, and a gate beside the sweep checks both,
/// counts the points and asks `char::escape_debug` — std's own Unicode tables —
/// whether every one of them is a character it declines to print.
///
/// **And only the escapers' question.** Whether two sentences a reader cannot
/// tell apart are the same sentence is the catalog compiler's
/// `tells_two_sentences_apart`, and whether a text a reader is shown hides
/// anything is [`unseen`]; both read this array together with
/// [`BLANK_RANGES`]. What is held here is replaced by a *space*, so the
/// test for a character belonging here is not that a reader sees nothing of it —
/// it is that removing it cannot damage the words around it.
pub const INVISIBLE_RANGES: [(u32, u32); 25] = [
    (0x0000, 0x001F),   // the C0 controls
    (0x007F, 0x009F),   // delete, and the C1 controls
    (0x00AD, 0x00AD),   // the soft hyphen
    (0x0600, 0x0605),   // the Arabic number signs
    (0x061C, 0x061C),   // the Arabic letter mark
    (0x06DD, 0x06DD),   // the Arabic end of ayah
    (0x070F, 0x070F),   // the Syriac abbreviation mark
    (0x0890, 0x0891),   // the Arabic pound and piastre marks
    (0x08E2, 0x08E2),   // the Arabic disputed end of ayah
    (0x180E, 0x180E),   // the Mongolian vowel separator
    (0x200B, 0x200F),   // the zero-width space, the joiners and the two marks
    (0x2028, 0x2029),   // the line and paragraph separators
    (0x202A, 0x202E),   // the embedding and override family
    (0x2060, 0x2064),   // the word joiner and the invisible operators
    (0x2066, 0x2069),   // the isolate family
    (0x206A, 0x206F),   // the deprecated shaping selectors
    (0xFEFF, 0xFEFF),   // the byte-order mark, invisible anywhere but the start
    (0xFFF9, 0xFFFB),   // the interlinear annotation controls
    (0x110BD, 0x110BD), // the Kaithi number sign
    (0x110CD, 0x110CD), // the Kaithi number sign above
    (0x13430, 0x1343F), // the Egyptian hieroglyph format controls
    (0x1BCA0, 0x1BCA3), // the shorthand format controls
    (0x1D173, 0x1D17A), // the musical beam and phrase controls
    (0x1D17B, 0x1D182), // the musical combining controls
    (0xE0000, 0xE0FFF), // the whole tag plane, which spells ASCII invisibly
];

/// Whether this character shows a reader nothing.
///
/// **Wider than the catalog compiler's `is_a_direction_mark`, because the
/// question is different.** That predicate is about *direction*, and it is the
/// right one for a message that could be made to render backwards. This one is
/// about whether two strings a reader cannot tell apart are the same string —
/// which is what the untranslated-message rule asks, and `split_whitespace`
/// drops none of these.
///
/// With `split_whitespace` in the rule, `catalog/i18n/pt-BR.toml`'s
/// `NC-JRNL-0001` — `critical`, non-retryable, the journal-tampering message —
/// set to the **English** sentence plus one `U+2063 INVISIBLE SEPARATOR` is
/// accepted by `cargo xtask build`, and `docs/errors.md` publishes the English
/// sentence in the pt-BR column of the row a reader consults when the audit
/// chain does not add up. `U+200B` would be caught, but by
/// `clippy::invisible_characters` on the generated crate rather than by the
/// catalog's own rule.
///
/// The general category is `Cc` and `Cf`, and the answer comes from
/// [`INVISIBLE_RANGES`] rather than from clauses written here: `is_control()`
/// and the compiler's `is_a_direction_mark` are both subsets of it, which the
/// gate beside the sweep sweeps the whole character space to prove.
#[must_use]
pub fn shows_nothing(character: char) -> bool {
    let point = character as u32;
    INVISIBLE_RANGES
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&point))
}

/// Every further code point a reader sees nothing of, as inclusive ranges.
///
/// **[`INVISIBLE_RANGES`] answers one question, not two.** It decides
/// [`shows_nothing`], which four escapers read to say what must not reach a
/// generated file. The untranslated-message rule asks something else, whether
/// two sentences a
/// reader cannot tell apart are the same sentence. `U+3164 HANGUL FILLER` is the
/// character that pulls the two apart: category `Lo`, a letter, and drawn as
/// nothing. Measured, `catalog/i18n/pt-BR.toml`'s `error.journal.chain-broken` —
/// `NC-JRNL-0001`, `critical`, non-retryable, the journal-tampering message —
/// filed as the **English** sentence with one `U+3164` appended to it:
///
/// ```text
/// compile(root, Bootstrap::Refuse)  ->  Ok, 0 problem(s)
/// docs/errors.md  ->  | `NC-JRNL-0001` | critical | no | … |
///                       The activity history does not add up, … |
///                       The activity history does not add up, … |
/// ```
///
/// One sentence in both language columns of the row a reader consults when the
/// audit chain does not add up, under a clean build. That is the `U+2063`
/// case and the `U+070F` case a third time, and the three are one class.
///
/// **What keeps these out of the escapers' array is what the escapers do.**
/// Measured, all 27 points below inserted into [`INVISIBLE_RANGES`] in order,
/// its length taken to 33 and the frozen count in the sweep from 4244 to 4271:
/// this crate's suite passes (the compiler's
/// `the_blank_set_is_a_second_set_and_no_escaper_reads_it` is what refuses it),
/// and `char::escape_debug` — the guard that stops that array widening into
/// visible characters — declines to print every one of the 27, so it says
/// nothing either. What the escapers then do to three ordinary sentences is
/// the argument:
///
/// ```text
/// "⚠\u{FE0F} the disk is full"  ->  /// ⚠  the disk is full
/// "한\u{3164}국"                 ->  한 국
/// "a\u{034F}b"                   ->  a b
/// ```
///
/// A warning sign loses its emoji presentation and gains a stray space, and a
/// Korean word is cut in two, because what [`shows_nothing`] holds is replaced
/// by a space and these characters sit *inside* words. So they are two sets: a
/// character that cannot tell two sentences apart is not thereby a character
/// that has to be taken out of a doc line. Nothing that writes a file reads
/// this one: what reads it beside the untranslated rule refuses a text, or
/// replaces such a character with one a reader sees ([`unseen`]).
///
/// **Unicode's own name for it is `Default_Ignorable_Code_Point`**, the property
/// whose meaning is that a conforming renderer draws nothing at all. What is
/// here is that property's assigned members which [`INVISIBLE_RANGES`] does not
/// already hold — the fillers, the inherent vowels, the grapheme joiner and the
/// variation selectors. Ordered, disjoint, and disjoint from
/// [`INVISIBLE_RANGES`]: a point in both would mean one of the two arrays has
/// taken the other's question back, which is the state this split exists to end.
///
/// **What this does not close, said plainly.** The rule still compares
/// characters one at a time, so the next member of the class is a homoglyph
/// rather than a blank — the English sentence with one Latin `o` written as
/// `U+043E CYRILLIC SMALL LETTER O` reads identically and compares different.
/// Closing that needs confusable folding, which is a dependency this crate does
/// not carry and a decision wider than this repair.
pub const BLANK_RANGES: [(u32, u32); 8] = [
    (0x034F, 0x034F), // the combining grapheme joiner
    (0x115F, 0x1160), // the Hangul choseong and jungseong fillers
    (0x17B4, 0x17B5), // the Khmer inherent vowels
    (0x180B, 0x180D), // the Mongolian free variation selectors one to three
    (0x180F, 0x180F), // the Mongolian free variation selector four
    (0x3164, 0x3164), // the Hangul filler
    (0xFE00, 0xFE0F), // the variation selectors
    (0xFFA0, 0xFFA0), // the halfwidth Hangul filler
];

/// Whether a reader sees nothing of this character, as a text a person reads
/// and an agent is handed is held to: one [`shows_nothing`] answers for, or one
/// of [`BLANK_RANGES`].
///
/// **The refusing reader's question, which is neither of the other two.** An
/// escaper replaces what [`shows_nothing`] holds with a space, and must leave
/// [`BLANK_RANGES`] alone, since those sit inside words; a schema's text, a
/// command's title, and a surface's title and texts are refused instead, and a
/// refusal damages no word — so they are held to both, and a line or a label
/// shown to a person has either replaced by `U+FFFD`. The measured case is
/// sixteen variation selectors, a byte in every two of them, carried unseen
/// through a schema's description and a title.
#[must_use]
pub fn unseen(character: char) -> bool {
    let point = character as u32;
    shows_nothing(character)
        || BLANK_RANGES
            .iter()
            .any(|(first, last)| (*first..=*last).contains(&point))
}

/// The five types, as the subset spells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Object,
    String,
    Integer,
    Boolean,
    Array,
}

impl Kind {
    /// Every type, in the order the subset lists them.
    const ALL: [Self; 5] = [
        Self::Object,
        Self::String,
        Self::Integer,
        Self::Boolean,
        Self::Array,
    ];

    /// The type a spelling names, if it names one of the five.
    ///
    /// Read back through [`Kind::spelling`], so the two directions are one
    /// table and cannot disagree.
    fn named(spelling: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.spelling() == spelling)
    }

    /// How the subset spells this type.
    const fn spelling(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::String => "string",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Array => "array",
        }
    }
}

/// A compiled argument schema: an object at the top, closed all the way down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    root: Object,
}

/// An object schema: its properties in the order the schema declares them.
///
/// Every name is looked up through `index` and `required`, never by scanning
/// `properties`: a scan per argument key costs the product of the keys and
/// the declared names, and a plugin's schema and a caller's arguments choose
/// both, on the host, with no fuel to meter it.
#[derive(Debug, Clone)]
pub(crate) struct Object {
    /// The declared properties, in the order the schema declares them.
    pub(crate) properties: Vec<(String, Node)>,
    /// What a person is shown of each property, beside it in `properties`.
    shown: Vec<Shown>,
    /// Where each declared name sits in `properties`.
    index: BTreeMap<String, usize>,
    /// The names an instance has to carry.
    required: BTreeSet<String>,
}

// **What a schema decides, and not what it shows.** Two schemas that refuse and
// accept the same instances are one schema to the kernel, which holds a
// plugin's declaration of a catalog command to the catalog's by this
// equality: a title or a description reworded is not a different command.
impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        self.properties == other.properties
            && self.index == other.index
            && self.required == other.required
    }
}

impl Eq for Object {}

/// What a person is shown of one property: its title and its description,
/// each decoded and held to [`unseen`] when the schema was compiled.
#[derive(Debug, Clone, Default)]
struct Shown {
    /// The property's `title`, where it has one.
    title: Option<String>,
    /// The property's `description`, where it has one.
    description: Option<String>,
}

impl Shown {
    /// The annotations `property` carries, read once it has compiled.
    fn of(property: &Map<String, Value>) -> Self {
        let text = |annotation: &str| {
            property
                .get(annotation)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        Self {
            title: text("title"),
            description: text("description"),
        }
    }
}

/// The strings a string's `enum` lists: a set a value is looked up in, and
/// the order the schema wrote them in, which help lists them by.
#[derive(Debug, Clone)]
pub(crate) struct Options {
    /// The options, as a set.
    ///
    /// A string is looked up in it: a list scanned per string costs the
    /// product of the options and the strings, both chosen by whoever writes
    /// the schema and the arguments, on the host, with no fuel to meter it.
    set: BTreeSet<String>,
    /// The same options, in the order the schema writes them.
    written: Vec<String>,
}

// The set alone: the order the options are written in is shown, and decides
// nothing, as an annotation does.
impl PartialEq for Options {
    fn eq(&self, other: &Self) -> bool {
        self.set == other.set
    }
}

impl Eq for Options {}

impl Object {
    /// The schema a declared property carries.
    pub(crate) fn property(&self, name: &str) -> Option<&Node> {
        let at = *self.index.get(name)?;
        self.properties.get(at).map(|(_, node)| node)
    }

    /// Whether an instance has to carry `name`.
    pub(crate) fn requires(&self, name: &str) -> bool {
        self.required.contains(name)
    }
}

/// One compiled schema, of one of the five types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Object(Object),
    String {
        min_length: Option<u64>,
        max_length: Option<u64>,
        /// The options `enum` lists.
        one_of: Option<Options>,
        entity: Option<String>,
    },
    Integer {
        minimum: Option<i64>,
        maximum: Option<i64>,
    },
    Boolean,
    Array {
        items: Box<Node>,
        min_items: Option<u64>,
        max_items: Option<u64>,
    },
}

impl Schema {
    /// Compiles a schema's text, refusing anything outside the subset.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`] locating the first thing refused, whose fault is one
    /// of: [`SchemaFault::InvisibleCharacter`], [`SchemaFault::NotJson`] (a key
    /// written twice included), [`SchemaFault::NotAnObjectAtTheTop`],
    /// [`SchemaFault::UnknownKeyword`], [`SchemaFault::KeywordNotForType`],
    /// [`SchemaFault::WrongKeywordType`], [`SchemaFault::TypeMissing`],
    /// [`SchemaFault::TypeUnsupported`], [`SchemaFault::OpenObject`],
    /// [`SchemaFault::RequiredNamesNoProperty`], [`SchemaFault::BoundsInverted`],
    /// [`SchemaFault::EntityOnNonString`], [`SchemaFault::EntityNotATypeName`],
    /// [`SchemaFault::EnumEmptyOrDuplicate`], [`SchemaFault::ItemsMissing`] or
    /// [`SchemaFault::TooDeep`].
    pub fn compile(text: &str) -> Result<Self, SchemaError> {
        // **The text first, before anything reads it.** JSON's own whitespace
        // is the one exemption: a schema written over several lines is how most
        // are written, and `\n` is in the table's first row. The rest of that
        // row never reaches a parsed schema anyway — JSON refuses a raw control
        // character inside a string — so what this adds is the rest of the
        // table: the direction family, the zero-widths, the tag plane and their
        // kin.
        if let Some(hidden) = text
            .chars()
            .find(|held| unseen(*held) && !matches!(held, '\n' | '\r' | '\t'))
        {
            return Err(SchemaError {
                path: String::new(),
                fault: SchemaFault::InvisibleCharacter(hidden),
            });
        }
        let value = strict::parse(text).map_err(|error| SchemaError {
            path: String::new(),
            fault: SchemaFault::NotJson(error),
        })?;
        let top = value
            .as_object()
            .filter(|top| top.get("type").and_then(Value::as_str) == Some("object"));
        let Some(top) = top else {
            return Err(SchemaError {
                path: String::new(),
                fault: SchemaFault::NotAnObjectAtTheTop,
            });
        };
        // The rules every schema answers to, asked of the top as of any other;
        // its type was read as `object` above, so what comes back is that or a
        // refusal.
        the_kind_of(top, "", 1)?;
        Ok(Self {
            root: compile_object(top, "", 1)?,
        })
    }

    /// Validates arguments against the schema, listing what is wrong.
    ///
    /// # Errors
    ///
    /// Every [`Violation`] found, at most [`MAX_VIOLATIONS`] of them, in the
    /// order the arguments hold their keys; within one object, the required
    /// names it lacks come after its keys. A caller refusing the arguments needs
    /// only that the list is not empty.
    pub fn validate(&self, instance: &Value) -> Result<(), Vec<Violation>> {
        let mut found = Vec::new();
        let mut path = String::new();
        check_object(&self.root, instance, &mut path, &mut found);
        if found.is_empty() { Ok(()) } else { Err(found) }
    }

    /// The top-level properties, in the order the schema declares them.
    #[must_use]
    pub fn properties(&self) -> Vec<&str> {
        self.root
            .properties
            .iter()
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// The top-level object, which a selector reads its property from.
    pub(crate) fn root(&self) -> &Object {
        &self.root
    }

    /// Every top-level argument, as a person is shown it: in the order the
    /// schema declares them, each with whether it is required, its title and
    /// its description, and the shape a value of it takes.
    ///
    /// **A view, and nothing a caller builds a schema from**: what it reads is
    /// what [`Schema::compile`] kept, so the arguments a command line is read
    /// into are the properties [`Schema::validate`] holds it to.
    #[must_use]
    pub fn arguments(&self) -> Vec<Argument<'_>> {
        arguments_of(&self.root)
    }
}

/// One argument of a command, as a person is shown it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argument<'a> {
    /// Its name, as the schema declares it.
    pub name: &'a str,
    /// Whether an instance has to carry it.
    pub required: bool,
    /// Its `title`, where it has one.
    pub title: Option<&'a str>,
    /// Its `description`, where it has one.
    pub description: Option<&'a str>,
    /// What a value of it is.
    pub shape: Shape<'a>,
}

/// What a value of an argument is, with every bound the schema sets on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape<'a> {
    /// Text.
    String {
        /// The fewest characters it holds.
        min_length: Option<u64>,
        /// The most characters it holds.
        max_length: Option<u64>,
        /// The values `enum` allows, in the order the schema writes them.
        options: Option<Vec<&'a str>>,
        /// The Cedar entity type it names, from `x-entity`.
        entity: Option<&'a str>,
    },
    /// A whole number that fits in 64 signed bits.
    Integer {
        /// The least it may be.
        minimum: Option<i64>,
        /// The most it may be.
        maximum: Option<i64>,
    },
    /// True or false.
    Boolean,
    /// A list of values of one shape.
    Array {
        /// What each item is.
        items: Box<Shape<'a>>,
        /// The fewest items it holds.
        min_items: Option<u64>,
        /// The most items it holds.
        max_items: Option<u64>,
    },
    /// An object, closed, with arguments of its own.
    Object(Vec<Argument<'a>>),
}

/// The arguments `object` declares, in its order.
fn arguments_of(object: &Object) -> Vec<Argument<'_>> {
    object
        .properties
        .iter()
        .zip(&object.shown)
        .map(|((name, node), shown)| Argument {
            name,
            required: object.requires(name),
            title: shown.title.as_deref(),
            description: shown.description.as_deref(),
            shape: shape_of(node),
        })
        .collect()
}

/// The shape `node` gives a value.
fn shape_of(node: &Node) -> Shape<'_> {
    match node {
        Node::Object(object) => Shape::Object(arguments_of(object)),
        Node::String {
            min_length,
            max_length,
            one_of,
            entity,
        } => Shape::String {
            min_length: *min_length,
            max_length: *max_length,
            options: one_of
                .as_ref()
                .map(|options| options.written.iter().map(String::as_str).collect()),
            entity: entity.as_deref(),
        },
        Node::Integer { minimum, maximum } => Shape::Integer {
            minimum: *minimum,
            maximum: *maximum,
        },
        Node::Boolean => Shape::Boolean,
        Node::Array {
            items,
            min_items,
            max_items,
        } => Shape::Array {
            items: Box::new(shape_of(items)),
            min_items: *min_items,
            max_items: *max_items,
        },
    }
}

/// A schema refused, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    /// A JSON Pointer (RFC 6901) to what was refused; empty is the top.
    ///
    /// Each token is repeated the way every name in a diagnostic here is —
    /// control characters escaped and a long one cut short — so it resolves
    /// for any name a person would write and prints safely for any other.
    pub path: String,
    /// What is wrong there.
    pub fault: SchemaFault,
}

impl fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(formatter, "at the top of the schema: {}", self.fault)
        } else {
            write!(formatter, "at `{}`: {}", self.path, self.fault)
        }
    }
}

impl std::error::Error for SchemaError {}

/// Why a schema is outside the subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaFault {
    /// A character a reader sees nothing of, JSON's whitespace aside; named.
    InvisibleCharacter(char),
    /// The text is not JSON, or writes one key twice; the reader's own words.
    NotJson(String),
    /// The top level is not `"type": "object"`: arguments arrive as a map.
    NotAnObjectAtTheTop,
    /// A keyword the subset does not hold.
    UnknownKeyword(String),
    /// A keyword of the subset, written on a type it does not belong to.
    KeywordNotForType {
        /// The keyword.
        keyword: String,
        /// The type it was written on.
        type_name: &'static str,
    },
    /// A keyword holding a value of the wrong shape.
    WrongKeywordType(&'static str),
    /// A schema without `type`, which would accept anything.
    TypeMissing,
    /// A `type` outside the five.
    TypeUnsupported(String),
    /// An object schema without `"additionalProperties": false`.
    OpenObject,
    /// `required` names a property `properties` does not declare.
    RequiredNamesNoProperty(String),
    /// A lower bound above its upper bound; the lower keyword is named.
    BoundsInverted(&'static str),
    /// `x-entity` on a type that is not `string`.
    EntityOnNonString,
    /// `x-entity` naming something that is not a Cedar type name.
    EntityNotATypeName(String),
    /// `enum` empty, or naming one string twice.
    EnumEmptyOrDuplicate,
    /// An array schema that does not say what its items are.
    ItemsMissing,
    /// Schemas nested deeper than [`MAX_SCHEMA_DEPTH`].
    TooDeep,
}

impl fmt::Display for SchemaFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvisibleCharacter(hidden) => write!(
                formatter,
                "its text holds `U+{:04X}`, which a reader sees nothing of — and a schema \
                 is the tool interface an agent reads, so it would say one thing to a person \
                 and another to the agent",
                u32::from(*hidden)
            ),
            Self::NotJson(error) => write!(formatter, "it is not JSON: {error}"),
            Self::NotAnObjectAtTheTop => formatter.write_str(
                "its top level is not `\"type\": \"object\"`, and arguments arrive as a named map",
            ),
            Self::UnknownKeyword(keyword) => write!(
                formatter,
                "`{keyword}` is not a keyword of the subset this platform validates, and a \
                 keyword the validator does not know is one it would silently ignore"
            ),
            Self::KeywordNotForType { keyword, type_name } => write!(
                formatter,
                "`{keyword}` does not apply to `{type_name}`, so it would constrain nothing"
            ),
            Self::WrongKeywordType(keyword) => {
                write!(formatter, "`{keyword}` must hold ")?;
                write_the_shape_of(keyword, formatter)
            }
            Self::TypeMissing => {
                formatter.write_str("it has no `type`, so it would accept anything")
            }
            Self::TypeUnsupported(named) => write!(
                formatter,
                "`type` is `{named}`, and the subset holds object, string, integer, boolean and \
                 array — no number, because canonical JSON carries no fraction, and no null"
            ),
            Self::OpenObject => formatter.write_str(
                "an object must say `\"additionalProperties\": false`, or it accepts arguments \
                 nobody declared",
            ),
            Self::RequiredNamesNoProperty(name) => write!(
                formatter,
                "`required` names `{name}`, which `properties` does not declare"
            ),
            Self::BoundsInverted(lower) => {
                write!(formatter, "`{lower}` is above `")?;
                write_the_upper_bound_of(lower, formatter)?;
                formatter.write_str("`, so nothing satisfies it")
            }
            Self::EntityOnNonString => formatter.write_str(
                "`x-entity` names the type of an entity id, and an id is a string: it belongs on \
                 `\"type\": \"string\"` only",
            ),
            Self::EntityNotATypeName(named) => write!(
                formatter,
                "`x-entity` is `{named}`, which is not a Cedar type name (`Name` or \
                 `Namespace::Name`)"
            ),
            Self::EnumEmptyOrDuplicate => {
                formatter.write_str("`enum` must list at least one string, and none twice")
            }
            Self::ItemsMissing => {
                formatter.write_str("an array must say what its items are, in `items`")
            }
            Self::TooDeep => write!(formatter, "schemas nest deeper than {MAX_SCHEMA_DEPTH}"),
        }
    }
}

/// Writes what a keyword has to hold, in the words a diagnostic uses.
///
/// Written into the formatter rather than handed back: a function returning a
/// `&'static str` it computed is a door the type wall refuses outside the log's
/// own crate, and the answer here is only ever printed.
fn write_the_shape_of(keyword: &str, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match keyword {
        "type" => "one of the five type names, written as a string",
        "properties" => "an object whose every value is a schema",
        "required" => "a list of distinct property names",
        "enum" => "a list of strings",
        "items" => "one schema, written as an object",
        "minimum" | "maximum" => "a whole number that fits in 64 signed bits",
        "minLength" | "maxLength" | "minItems" | "maxItems" => "a whole number, zero or more",
        _ => "a string",
    })
}

/// Writes the upper bound a lower bound is compared with.
fn write_the_upper_bound_of(lower: &str, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match lower {
        "minLength" => "maxLength",
        "minimum" => "maximum",
        _ => "maxItems",
    })
}

/// Arguments that do not satisfy a schema, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// A JSON Pointer into the arguments; empty is the arguments as a whole.
    ///
    /// Its tokens are the schema's names and the arrays' positions, repeated
    /// as [`SchemaError::path`] repeats them.
    pub path: String,
    /// What is wrong there.
    pub kind: ViolationKind,
}

impl fmt::Display for Violation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(formatter, "the arguments: {}", self.kind)
        } else {
            write!(formatter, "`{}`: {}", self.path, self.kind)
        }
    }
}

/// Why a value does not satisfy its schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    /// Not the type the schema declares, which is named.
    WrongType(&'static str),
    /// A required property that is absent.
    MissingRequired(String),
    /// A property the schema does not declare.
    Unexpected(String),
    /// Fewer characters than `minLength`.
    TooShort,
    /// More characters than `maxLength`.
    TooLong,
    /// Below `minimum`.
    TooSmall,
    /// Above `maximum`.
    TooLarge,
    /// None of the strings `enum` lists.
    NotOneOf,
    /// Fewer items than `minItems`.
    TooFewItems,
    /// More items than `maxItems`.
    TooManyItems,
}

impl fmt::Display for ViolationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongType(expected) => write!(formatter, "is not {}", with_an_article(expected)),
            Self::MissingRequired(name) => write!(formatter, "`{name}` is required and absent"),
            Self::Unexpected(name) => {
                write!(
                    formatter,
                    "`{name}` is not an argument this command declares"
                )
            }
            Self::TooShort => formatter.write_str("is shorter than `minLength` allows"),
            Self::TooLong => formatter.write_str("is longer than `maxLength` allows"),
            Self::TooSmall => formatter.write_str("is below `minimum`"),
            Self::TooLarge => formatter.write_str("is above `maximum`"),
            Self::NotOneOf => formatter.write_str("is none of the values `enum` lists"),
            Self::TooFewItems => formatter.write_str("holds fewer items than `minItems` allows"),
            Self::TooManyItems => formatter.write_str("holds more items than `maxItems` allows"),
        }
    }
}

/// A type name with the article English puts before it.
fn with_an_article(type_name: &str) -> String {
    if type_name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        format!("an {type_name}")
    } else {
        format!("a {type_name}")
    }
}

/// A name from outside, fit to repeat in a diagnostic.
///
/// Every character [`shows_nothing`] answers for is escaped — the controls,
/// which move the cursor of the terminal a name is printed to, and the
/// direction family and the tag plane, which a reader of the line would never
/// see — and past [`EXCERPT`] characters the rest is elided.
pub(crate) fn excerpt(text: &str) -> String {
    let mut shown = String::new();
    for (count, character) in text.chars().enumerate() {
        if count == EXCERPT {
            shown.push('…');
            break;
        }
        if shows_nothing(character) {
            shown.extend(character.escape_default());
        } else {
            shown.push(character);
        }
    }
    shown
}

/// Appends one reference token to a JSON Pointer, escaped as RFC 6901 says.
///
/// The token is first cut to what a diagnostic may repeat of a name — see
/// [`excerpt`] — because a pointer is printed beside the fault it locates, and
/// the names in it are the schema author's. A name a person would write is
/// repeated whole, so the pointer still resolves.
fn push_segment(path: &mut String, segment: &str) {
    path.push('/');
    for character in excerpt(segment).chars() {
        match character {
            '~' => path.push_str("~0"),
            '/' => path.push_str("~1"),
            other => path.push(other),
        }
    }
}

/// A JSON Pointer one token further down.
fn joined(path: &str, segment: &str) -> String {
    let mut out = path.to_owned();
    push_segment(&mut out, segment);
    out
}

/// A refusal at `path`.
fn refused<T>(path: &str, fault: SchemaFault) -> Result<T, SchemaError> {
    Err(SchemaError {
        path: path.to_owned(),
        fault,
    })
}

/// Compiles one schema object, `depth` deep.
fn compile_node(
    schema: &Map<String, Value>,
    path: &str,
    depth: usize,
) -> Result<Node, SchemaError> {
    match the_kind_of(schema, path, depth)? {
        Kind::Object => compile_object(schema, path, depth).map(Node::Object),
        Kind::String => compile_string(schema, path),
        Kind::Integer => compile_integer(schema, path),
        Kind::Boolean => Ok(Node::Boolean),
        Kind::Array => compile_array(schema, path, depth),
    }
}

/// The type a schema object declares, once the rules every type shares hold.
///
/// Those rules are the depth, a closed keyword set, a `type` among the five,
/// every keyword on the type it belongs to, and annotations that are text.
fn the_kind_of(schema: &Map<String, Value>, path: &str, depth: usize) -> Result<Kind, SchemaError> {
    if depth > MAX_SCHEMA_DEPTH {
        return refused(path, SchemaFault::TooDeep);
    }
    if let Some(unknown) = schema
        .keys()
        .find(|written| !KEYWORDS.contains(&written.as_str()))
    {
        return refused(
            &joined(path, unknown),
            SchemaFault::UnknownKeyword(excerpt(unknown)),
        );
    }
    let kind = match schema.get("type") {
        None => return refused(path, SchemaFault::TypeMissing),
        Some(Value::String(named)) => match Kind::named(named) {
            Some(kind) => kind,
            None => {
                return refused(
                    &joined(path, "type"),
                    SchemaFault::TypeUnsupported(excerpt(named)),
                );
            }
        },
        Some(_) => return refused(&joined(path, "type"), SchemaFault::WrongKeywordType("type")),
    };
    for (keyword, owner) in OWNED_BY {
        if !schema.contains_key(keyword) || owner.is_none_or(|owner| owner == kind) {
            continue;
        }
        let fault = if keyword == "x-entity" {
            SchemaFault::EntityOnNonString
        } else {
            SchemaFault::KeywordNotForType {
                keyword: keyword.to_owned(),
                type_name: kind.spelling(),
            }
        };
        return refused(&joined(path, keyword), fault);
    }
    for annotation in ["description", "title"] {
        if schema.get(annotation).is_some_and(|held| !held.is_string()) {
            return refused(
                &joined(path, annotation),
                SchemaFault::WrongKeywordType(annotation),
            );
        }
        // Decoded, as a help line prints it: the text rule above reads the
        // escape `\u001b` as six characters a reviewer sees, and this reads
        // the escape character it parses to.
        if let Some(hidden) = schema
            .get(annotation)
            .and_then(Value::as_str)
            .and_then(first_unseen)
        {
            return refused(
                &joined(path, annotation),
                SchemaFault::InvisibleCharacter(hidden),
            );
        }
    }
    Ok(kind)
}

/// The first character of a decoded string a reader sees nothing of.
///
/// **JSON's own whitespace is not exempt here**, as it is in a schema's text:
/// inside a string a line break is a character a terminal acts on, written
/// only as an escape, and an annotation, an option or a name holding one
/// forges a line of whatever prints it.
fn first_unseen(text: &str) -> Option<char> {
    text.chars().find(|character| unseen(*character))
}

/// Compiles an object schema's properties and the ones it requires.
fn compile_object(
    schema: &Map<String, Value>,
    path: &str,
    depth: usize,
) -> Result<Object, SchemaError> {
    if schema.get("additionalProperties") != Some(&Value::Bool(false)) {
        return refused(path, SchemaFault::OpenObject);
    }
    let mut properties = Vec::new();
    let mut shown = Vec::new();
    let mut index = BTreeMap::new();
    if let Some(declared) = schema.get("properties") {
        let under = joined(path, "properties");
        let Some(declared) = declared.as_object() else {
            return refused(&under, SchemaFault::WrongKeywordType("properties"));
        };
        for (name, property) in declared {
            let at = joined(&under, name);
            // A name is a flag a person types and a line help prints, so one
            // holding an escape character is refused as an annotation is.
            if let Some(hidden) = first_unseen(name) {
                return refused(&at, SchemaFault::InvisibleCharacter(hidden));
            }
            let Some(property) = property.as_object() else {
                return refused(&at, SchemaFault::WrongKeywordType("properties"));
            };
            index.insert(name.clone(), properties.len());
            properties.push((name.clone(), compile_node(property, &at, depth + 1)?));
            shown.push(Shown::of(property));
        }
    }
    let mut required = BTreeSet::new();
    if let Some(named) = schema.get("required") {
        let under = joined(path, "required");
        let Some(named) = named.as_array() else {
            return refused(&under, SchemaFault::WrongKeywordType("required"));
        };
        for (position, name) in named.iter().enumerate() {
            let at = joined(&under, &position.to_string());
            let Some(name) = name.as_str() else {
                return refused(&at, SchemaFault::WrongKeywordType("required"));
            };
            if required.contains(name) {
                return refused(&at, SchemaFault::WrongKeywordType("required"));
            }
            if !index.contains_key(name) {
                return refused(&at, SchemaFault::RequiredNamesNoProperty(excerpt(name)));
            }
            required.insert(name.to_owned());
        }
    }
    Ok(Object {
        properties,
        shown,
        index,
        required,
    })
}

/// Compiles a string schema: its length bounds, its options and its entity.
fn compile_string(schema: &Map<String, Value>, path: &str) -> Result<Node, SchemaError> {
    let min_length = count(schema, "minLength", path)?;
    let max_length = count(schema, "maxLength", path)?;
    if let (Some(lower), Some(upper)) = (min_length, max_length)
        && lower > upper
    {
        return refused(path, SchemaFault::BoundsInverted("minLength"));
    }
    let one_of = match schema.get("enum") {
        None => None,
        Some(Value::Array(listed)) => {
            let mut options = BTreeSet::new();
            let mut written = Vec::new();
            for option in listed {
                let Some(option) = option.as_str() else {
                    return refused(&joined(path, "enum"), SchemaFault::WrongKeywordType("enum"));
                };
                // An option is a value a person types and help lists, so it
                // is held to the same characters as a name.
                if let Some(hidden) = first_unseen(option) {
                    return refused(
                        &joined(path, "enum"),
                        SchemaFault::InvisibleCharacter(hidden),
                    );
                }
                if !options.insert(option.to_owned()) {
                    return refused(&joined(path, "enum"), SchemaFault::EnumEmptyOrDuplicate);
                }
                written.push(option.to_owned());
            }
            if options.is_empty() {
                return refused(&joined(path, "enum"), SchemaFault::EnumEmptyOrDuplicate);
            }
            Some(Options {
                set: options,
                written,
            })
        }
        Some(_) => return refused(&joined(path, "enum"), SchemaFault::WrongKeywordType("enum")),
    };
    let entity = match schema.get("x-entity") {
        None => None,
        Some(Value::String(named)) if is_a_cedar_type_name(named) => Some(named.clone()),
        Some(Value::String(named)) => {
            return refused(
                &joined(path, "x-entity"),
                SchemaFault::EntityNotATypeName(excerpt(named)),
            );
        }
        Some(_) => {
            return refused(
                &joined(path, "x-entity"),
                SchemaFault::WrongKeywordType("x-entity"),
            );
        }
    };
    Ok(Node::String {
        min_length,
        max_length,
        one_of,
        entity,
    })
}

/// Compiles an integer schema's bounds.
fn compile_integer(schema: &Map<String, Value>, path: &str) -> Result<Node, SchemaError> {
    let bound = |keyword: &'static str| -> Result<Option<i64>, SchemaError> {
        match schema.get(keyword) {
            None => Ok(None),
            Some(held) => match held.as_i64() {
                Some(bound) => Ok(Some(bound)),
                None => refused(
                    &joined(path, keyword),
                    SchemaFault::WrongKeywordType(keyword),
                ),
            },
        }
    };
    let minimum = bound("minimum")?;
    let maximum = bound("maximum")?;
    if let (Some(lower), Some(upper)) = (minimum, maximum)
        && lower > upper
    {
        return refused(path, SchemaFault::BoundsInverted("minimum"));
    }
    Ok(Node::Integer { minimum, maximum })
}

/// Compiles an array schema: what its items are, and how many.
fn compile_array(
    schema: &Map<String, Value>,
    path: &str,
    depth: usize,
) -> Result<Node, SchemaError> {
    let Some(items) = schema.get("items") else {
        return refused(path, SchemaFault::ItemsMissing);
    };
    let at = joined(path, "items");
    let Some(items) = items.as_object() else {
        return refused(&at, SchemaFault::WrongKeywordType("items"));
    };
    let min_items = count(schema, "minItems", path)?;
    let max_items = count(schema, "maxItems", path)?;
    if let (Some(lower), Some(upper)) = (min_items, max_items)
        && lower > upper
    {
        return refused(path, SchemaFault::BoundsInverted("minItems"));
    }
    Ok(Node::Array {
        items: Box::new(compile_node(items, &at, depth + 1)?),
        min_items,
        max_items,
    })
}

/// A keyword that holds a count: a whole number, zero or more, when present.
fn count(
    schema: &Map<String, Value>,
    keyword: &'static str,
    path: &str,
) -> Result<Option<u64>, SchemaError> {
    match schema.get(keyword) {
        None => Ok(None),
        Some(held) => match held.as_u64() {
            Some(counted) => Ok(Some(counted)),
            None => refused(
                &joined(path, keyword),
                SchemaFault::WrongKeywordType(keyword),
            ),
        },
    }
}

/// Words Cedar reserves, which no segment of a type name may be.
const CEDAR_RESERVED: [&str; 9] = [
    "true", "false", "if", "then", "else", "in", "is", "like", "has",
];

/// Whether `name` is a Cedar type name: identifiers joined by `::`.
pub(crate) fn is_a_cedar_type_name(name: &str) -> bool {
    name.split("::").all(|segment| {
        let mut characters = segment.chars();
        characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
            && !CEDAR_RESERVED.contains(&segment)
            && !segment.starts_with("__cedar")
    })
}

/// Records a violation, unless the list is already as long as it may be.
fn note(found: &mut Vec<Violation>, path: &str, kind: ViolationKind) {
    if found.len() < MAX_VIOLATIONS {
        found.push(Violation {
            path: path.to_owned(),
            kind,
        });
    }
}

/// Checks `value` against an object schema.
///
/// **The walk is over what the arguments hold, not over what the schema
/// declares.** Each key is looked up; the declared names are never walked, so
/// an object of one key costs one lookup however many names its schema
/// declares. The required names are walked after the keys: an object holding
/// them all has at least as many keys and pays with its own, and one lacking
/// some adds to a list whose filling stops [`check`] from descending again.
fn check_object(object: &Object, value: &Value, path: &mut String, found: &mut Vec<Violation>) {
    let Some(given) = value.as_object() else {
        note(found, path, ViolationKind::WrongType("object"));
        return;
    };
    for (key, held) in given {
        if let Some(property) = object.property(key) {
            let before = path.len();
            push_segment(path, key);
            check(property, held, path, found);
            path.truncate(before);
        } else {
            note(found, path, ViolationKind::Unexpected(excerpt(key)));
        }
    }
    for name in &object.required {
        if !given.contains_key(name) {
            note(found, path, ViolationKind::MissingRequired(excerpt(name)));
        }
    }
}

/// Checks `value` against one schema, appending what is wrong to `found`.
///
/// **Once the list is full, nothing is descended into.** This is the one place
/// that stops the walk, and every schema below the top is reached through it:
/// without it the arguments are still walked to the end for a list that takes
/// nothing more, and each object among them walks its schema's required names
/// again.
fn check(node: &Node, value: &Value, path: &mut String, found: &mut Vec<Violation>) {
    if found.len() >= MAX_VIOLATIONS {
        return;
    }
    match node {
        Node::Object(object) => check_object(object, value, path, found),
        Node::String {
            min_length,
            max_length,
            one_of,
            entity: _,
        } => {
            let Some(text) = value.as_str() else {
                note(found, path, ViolationKind::WrongType("string"));
                return;
            };
            let length = text.chars().count() as u64;
            if min_length.is_some_and(|lower| length < lower) {
                note(found, path, ViolationKind::TooShort);
            }
            if max_length.is_some_and(|upper| length > upper) {
                note(found, path, ViolationKind::TooLong);
            }
            if one_of
                .as_ref()
                .is_some_and(|options| !options.set.contains(text))
            {
                note(found, path, ViolationKind::NotOneOf);
            }
        }
        Node::Integer { minimum, maximum } => {
            let Some(number) = value.as_i64() else {
                note(found, path, ViolationKind::WrongType("integer"));
                return;
            };
            if minimum.is_some_and(|lower| number < lower) {
                note(found, path, ViolationKind::TooSmall);
            }
            if maximum.is_some_and(|upper| number > upper) {
                note(found, path, ViolationKind::TooLarge);
            }
        }
        Node::Boolean => {
            if !value.is_boolean() {
                note(found, path, ViolationKind::WrongType("boolean"));
            }
        }
        Node::Array {
            items,
            min_items,
            max_items,
        } => check_array(items, *min_items, *max_items, value, path, found),
    }
}

/// Checks `value` against an array schema, then each of its items.
fn check_array(
    items: &Node,
    min_items: Option<u64>,
    max_items: Option<u64>,
    value: &Value,
    path: &mut String,
    found: &mut Vec<Violation>,
) {
    let Some(held) = value.as_array() else {
        note(found, path, ViolationKind::WrongType("array"));
        return;
    };
    let length = held.len() as u64;
    if min_items.is_some_and(|lower| length < lower) {
        note(found, path, ViolationKind::TooFewItems);
    }
    if max_items.is_some_and(|upper| length > upper) {
        note(found, path, ViolationKind::TooManyItems);
    }
    for (index, item) in held.iter().enumerate() {
        let before = path.len();
        push_segment(path, &index.to_string());
        check(items, item, path, found);
        path.truncate(before);
    }
}

/// A JSON reader that refuses a key written twice.
///
/// `serde_json::Value` keeps the second of two equal keys without a word, and
/// another reader of the same text keeps the first — so this reads into the
/// same `Value` through a visitor that refuses instead. Numbers, strings and
/// the parse itself are `serde_json`'s.
mod strict {
    use core::fmt;

    use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
    use serde_json::{Map, Value};

    /// One value, read with every key in every object distinct.
    struct Strict(Value);

    impl<'de> Deserialize<'de> for Strict {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_any(Refusing).map(Strict)
        }
    }

    /// The visitor that builds a [`Value`] and refuses a repeated key.
    struct Refusing;

    impl<'de> Visitor<'de> for Refusing {
        type Value = Value;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON value")
        }

        fn visit_bool<E>(self, held: bool) -> Result<Value, E> {
            Ok(Value::Bool(held))
        }

        fn visit_i64<E>(self, held: i64) -> Result<Value, E> {
            Ok(Value::Number(held.into()))
        }

        fn visit_u64<E>(self, held: u64) -> Result<Value, E> {
            Ok(Value::Number(held.into()))
        }

        fn visit_f64<E>(self, held: f64) -> Result<Value, E> {
            // JSON cannot spell a number that is not finite, so `from_f64`
            // always answers here; `Null` is what it would mean if it did not.
            Ok(serde_json::Number::from_f64(held).map_or(Value::Null, Value::Number))
        }

        fn visit_str<E>(self, held: &str) -> Result<Value, E> {
            Ok(Value::String(held.to_owned()))
        }

        fn visit_string<E>(self, held: String) -> Result<Value, E> {
            Ok(Value::String(held))
        }

        fn visit_unit<E>(self) -> Result<Value, E> {
            Ok(Value::Null)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
            let mut items = Vec::new();
            while let Some(Strict(item)) = sequence.next_element()? {
                items.push(item);
            }
            Ok(Value::Array(items))
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
            let mut object = Map::new();
            while let Some(key) = map.next_key::<String>()? {
                if object.contains_key(&key) {
                    return Err(de::Error::custom(format_args!(
                        "the key `{}` is written twice",
                        super::excerpt(&key)
                    )));
                }
                let Strict(held) = map.next_value()?;
                object.insert(key, held);
            }
            Ok(Value::Object(object))
        }
    }

    /// Reads `text` as one JSON value with no key repeated in any object.
    pub(super) fn parse(text: &str) -> Result<Value, String> {
        let mut reader = serde_json::Deserializer::from_str(text);
        let Strict(value) = Strict::deserialize(&mut reader).map_err(|error| error.to_string())?;
        reader.end().map_err(|error| error.to_string())?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{Value, json};

    use super::{
        EXCERPT, KEYWORDS, MAX_SCHEMA_DEPTH, MAX_VIOLATIONS, Schema, SchemaError, SchemaFault,
        Violation, ViolationKind, excerpt, is_a_cedar_type_name, shows_nothing,
    };
    use crate::{Selector, SelectorError};

    /// A JSON Pointer from reference tokens already escaped.
    ///
    /// Built rather than written as a literal, because the determinism scan
    /// over this crate reads a string opening with a slash as a path on this
    /// machine.
    fn pointer(tokens: &[&str]) -> String {
        tokens.iter().fold(String::new(), |mut path, token| {
            path.push('/');
            path.push_str(token);
            path
        })
    }

    /// The fault a schema's text is refused with.
    fn fault(text: &str) -> SchemaFault {
        Schema::compile(text)
            .expect_err("the schema is outside the subset")
            .fault
    }

    /// The whole refusal, path included.
    fn refusal(text: &str) -> SchemaError {
        Schema::compile(text).expect_err("the schema is outside the subset")
    }

    /// A closed object schema around `properties` and `required`.
    fn closed(properties: &str, required: &str) -> String {
        format!(
            r#"{{"type":"object","additionalProperties":false,"properties":{{{properties}}},"required":[{required}]}}"#
        )
    }

    /// What validating `instance` against `schema` refuses, kinds only.
    fn violations(schema: &str, instance: &Value) -> Vec<ViolationKind> {
        Schema::compile(schema)
            .expect("the schema is inside the subset")
            .validate(instance)
            .expect_err("the instance is refused")
            .into_iter()
            .map(|violation| violation.kind)
            .collect()
    }

    // --- the subset itself ---

    #[test]
    fn every_keyword_in_the_subset_compiles_where_it_belongs() {
        let schema = Schema::compile(
            r#"{"type":"object","additionalProperties":false,"title":"t","description":"d",
                "properties":{
                  "text":{"type":"string","minLength":1,"maxLength":8,"enum":["a","b"],"x-entity":"Content"},
                  "count":{"type":"integer","minimum":-3,"maximum":3},
                  "flag":{"type":"boolean"},
                  "list":{"type":"array","items":{"type":"string"},"minItems":0,"maxItems":2}},
                "required":["text"]}"#,
        )
        .expect("every keyword in its place");
        assert_eq!(schema.properties(), ["text", "count", "flag", "list"]);
    }

    /// What shows a keyword of the subset is not a no-op.
    #[derive(Clone, Copy)]
    enum Evidence {
        /// Validating `instance` against `schema`, both JSON, finds a
        /// violation that reads `said`.
        ///
        /// A schema that is not `top` is the one property `p` of a closed
        /// object, and the instance is what `p` holds. Taking the keyword out
        /// either refuses the schema, because the keyword is required where it
        /// is written, or finds no such violation.
        Refuses {
            schema: &'static str,
            top: bool,
            instance: &'static str,
            said: &'static str,
        },
        /// Read by the selector, never by `validate`: `argument:p` needs it.
        NamesTheEntity,
        /// An annotation: the schema compiles to the same thing without it.
        Annotates,
    }

    /// Every keyword of the subset, each beside what shows it is not a no-op.
    ///
    /// Written by hand, not read out of the keyword table: the table compared
    /// with itself is a test restating the implementation (`§19.5`), and a
    /// keyword admitted to it and implemented nowhere passes that.
    const WHAT_EACH_KEYWORD_DOES: [(&str, Evidence); 15] = [
        (
            "type",
            Evidence::Refuses {
                schema: r#"{"type":"integer"}"#,
                top: false,
                instance: r#""1""#,
                said: "`/p`: is not an integer",
            },
        ),
        (
            "properties",
            Evidence::Refuses {
                schema: r#"{"type":"object","additionalProperties":false,"properties":{"p":{"type":"boolean"}}}"#,
                top: true,
                instance: r#"{"p":1}"#,
                said: "`/p`: is not a boolean",
            },
        ),
        (
            "required",
            Evidence::Refuses {
                schema: r#"{"type":"object","additionalProperties":false,"properties":{"p":{"type":"boolean"}},"required":["p"]}"#,
                top: true,
                instance: "{}",
                said: "the arguments: `p` is required and absent",
            },
        ),
        (
            "additionalProperties",
            Evidence::Refuses {
                schema: r#"{"type":"object","additionalProperties":false}"#,
                top: true,
                instance: r#"{"q":true}"#,
                said: "the arguments: `q` is not an argument this command declares",
            },
        ),
        (
            "minLength",
            Evidence::Refuses {
                schema: r#"{"type":"string","minLength":2}"#,
                top: false,
                instance: r#""a""#,
                said: "`/p`: is shorter than `minLength` allows",
            },
        ),
        (
            "maxLength",
            Evidence::Refuses {
                schema: r#"{"type":"string","maxLength":1}"#,
                top: false,
                instance: r#""ab""#,
                said: "`/p`: is longer than `maxLength` allows",
            },
        ),
        (
            "enum",
            Evidence::Refuses {
                schema: r#"{"type":"string","enum":["a"]}"#,
                top: false,
                instance: r#""b""#,
                said: "`/p`: is none of the values `enum` lists",
            },
        ),
        ("x-entity", Evidence::NamesTheEntity),
        (
            "minimum",
            Evidence::Refuses {
                schema: r#"{"type":"integer","minimum":0}"#,
                top: false,
                instance: "-1",
                said: "`/p`: is below `minimum`",
            },
        ),
        (
            "maximum",
            Evidence::Refuses {
                schema: r#"{"type":"integer","maximum":0}"#,
                top: false,
                instance: "1",
                said: "`/p`: is above `maximum`",
            },
        ),
        (
            "items",
            Evidence::Refuses {
                schema: r#"{"type":"array","items":{"type":"boolean"}}"#,
                top: false,
                instance: "[1]",
                said: "`/p/0`: is not a boolean",
            },
        ),
        (
            "minItems",
            Evidence::Refuses {
                schema: r#"{"type":"array","items":{"type":"boolean"},"minItems":1}"#,
                top: false,
                instance: "[]",
                said: "`/p`: holds fewer items than `minItems` allows",
            },
        ),
        (
            "maxItems",
            Evidence::Refuses {
                schema: r#"{"type":"array","items":{"type":"boolean"},"maxItems":0}"#,
                top: false,
                instance: "[true]",
                said: "`/p`: holds more items than `maxItems` allows",
            },
        ),
        ("description", Evidence::Annotates),
        ("title", Evidence::Annotates),
    ];

    /// A closed object schema whose one required property `p` is `property`.
    fn one(property: &Value) -> Value {
        json!({"type": "object", "additionalProperties": false,
               "properties": {"p": property}, "required": ["p"]})
    }

    /// What validating `instance` against `schema` says, if `schema` compiles.
    fn said(schema: &Value, instance: &Value) -> Option<Vec<String>> {
        let compiled = Schema::compile(&schema.to_string()).ok()?;
        Some(match compiled.validate(instance) {
            Ok(()) => Vec::new(),
            Err(found) => found.iter().map(ToString::to_string).collect(),
        })
    }

    /// Every keyword the subset admits decides something, or says it cannot.
    ///
    /// Compared with the table it is read from, the keyword list cannot
    /// fail: `uniqueItems` added to the table compiles, reaches every schema as
    /// a keyword of the subset, and constrains nothing — with every suite
    /// green, measured. That is the defect the crate's own header exists to
    /// refuse.
    #[test]
    fn every_keyword_in_the_subset_decides_something_or_is_an_annotation() {
        let shown: BTreeSet<&str> = WHAT_EACH_KEYWORD_DOES
            .iter()
            .map(|(keyword, _)| *keyword)
            .collect();
        assert_eq!(
            shown.len(),
            WHAT_EACH_KEYWORD_DOES.len(),
            "a keyword has two cases"
        );
        for keyword in KEYWORDS {
            assert!(
                shown.contains(keyword),
                "`{keyword}` has no case showing it constrains anything, and a keyword the \
                 subset admits and nothing reads is one an author believes refuses what it \
                 does not"
            );
        }
        for keyword in &shown {
            assert!(
                KEYWORDS.contains(keyword),
                "`{keyword}` has a case and is not a keyword of the subset"
            );
        }
        for (keyword, evidence) in WHAT_EACH_KEYWORD_DOES {
            let written = |text: &str| -> Value {
                serde_json::from_str(text).expect("the case is written in JSON")
            };
            match evidence {
                Evidence::Refuses {
                    schema,
                    top,
                    instance,
                    said: expected,
                } => {
                    let (mut schema, instance) = if top {
                        (written(schema), written(instance))
                    } else {
                        (one(&written(schema)), json!({ "p": written(instance) }))
                    };
                    let with = said(&schema, &instance).expect("the case is inside the subset");
                    assert!(
                        with.iter().any(|found| found == expected),
                        "`{keyword}` did not say `{expected}` of {instance}: {with:?}"
                    );
                    let holder = if top {
                        pointer(&[])
                    } else {
                        pointer(&["properties", "p"])
                    };
                    schema
                        .pointer_mut(&holder)
                        .and_then(Value::as_object_mut)
                        .and_then(|held| held.remove(keyword))
                        .unwrap_or_else(|| panic!("`{keyword}`'s case does not hold it"));
                    let without = said(&schema, &instance).unwrap_or_default();
                    assert!(
                        !without.iter().any(|found| found == expected),
                        "`{expected}` is said without `{keyword}` too, so the case shows \
                         nothing about it"
                    );
                }
                Evidence::NamesTheEntity => {
                    let with = one(&json!({"type": "string", "maxLength": 8, keyword: "Content"}));
                    let with = Schema::compile(&with.to_string()).expect("inside the subset");
                    let selected = Selector::parse("argument:p", &with).expect("backed");
                    assert_eq!(selected.entity_type(), "Content");
                    let without = one(&json!({"type": "string", "maxLength": 8}));
                    let without = Schema::compile(&without.to_string()).expect("inside the subset");
                    assert_eq!(
                        Selector::parse("argument:p", &without),
                        Err(SelectorError::NotAnEntity("p".to_owned()))
                    );
                }
                Evidence::Annotates => {
                    let annotated = one(&json!({"type": "boolean", keyword: "words"}));
                    assert_eq!(
                        Schema::compile(&annotated.to_string()),
                        Schema::compile(&one(&json!({"type": "boolean"})).to_string()),
                        "`{keyword}` changed what the schema compiles to"
                    );
                }
            }
        }
    }

    #[test]
    fn properties_are_listed_in_the_order_the_schema_declares_them() {
        let schema = Schema::compile(&closed(
            r#""zeta":{"type":"boolean"},"alpha":{"type":"boolean"},"mid":{"type":"boolean"}"#,
            "",
        ))
        .expect("inside the subset");
        assert_eq!(schema.properties(), ["zeta", "alpha", "mid"]);
        let empty = Schema::compile(r#"{"type":"object","additionalProperties":false}"#)
            .expect("an object with no properties takes no arguments");
        assert_eq!(empty.properties(), [] as [&str; 0]);
        assert!(empty.validate(&json!({})).is_ok());
    }

    /// Every argument is shown as the schema declares it.
    ///
    /// In its order, with whether it is required, its annotations, every
    /// bound, the options in the order written rather than sorted, and an
    /// object's own arguments.
    #[test]
    fn the_arguments_are_shown_as_the_schema_declares_them() {
        use super::{Argument, Shape};
        let schema = Schema::compile(
            r#"{"type":"object","additionalProperties":false,"title":"top","properties":{
                "zeta":{"type":"string","title":"Z","description":"the last letter",
                        "minLength":1,"maxLength":8,"enum":["b","a"],"x-entity":"Content"},
                "count":{"type":"integer","minimum":-3,"maximum":3},
                "flag":{"type":"boolean","description":"on or off"},
                "list":{"type":"array","items":{"type":"integer","maximum":9},"minItems":1,"maxItems":2},
                "inner":{"type":"object","additionalProperties":false,
                         "properties":{"x":{"type":"boolean","title":"X"}},"required":["x"]}},
              "required":["count","zeta"]}"#,
        )
        .expect("inside the subset");
        assert_eq!(
            schema.arguments(),
            [
                Argument {
                    name: "zeta",
                    required: true,
                    title: Some("Z"),
                    description: Some("the last letter"),
                    shape: Shape::String {
                        min_length: Some(1),
                        max_length: Some(8),
                        options: Some(vec!["b", "a"]),
                        entity: Some("Content"),
                    },
                },
                Argument {
                    name: "count",
                    required: true,
                    title: None,
                    description: None,
                    shape: Shape::Integer {
                        minimum: Some(-3),
                        maximum: Some(3),
                    },
                },
                Argument {
                    name: "flag",
                    required: false,
                    title: None,
                    description: Some("on or off"),
                    shape: Shape::Boolean,
                },
                Argument {
                    name: "list",
                    required: false,
                    title: None,
                    description: None,
                    shape: Shape::Array {
                        items: Box::new(Shape::Integer {
                            minimum: None,
                            maximum: Some(9),
                        }),
                        min_items: Some(1),
                        max_items: Some(2),
                    },
                },
                Argument {
                    name: "inner",
                    required: false,
                    title: None,
                    description: None,
                    shape: Shape::Object(vec![Argument {
                        name: "x",
                        required: true,
                        title: Some("X"),
                        description: None,
                        shape: Shape::Boolean,
                    }]),
                },
            ]
        );
        let empty = Schema::compile(r#"{"type":"object","additionalProperties":false}"#)
            .expect("an object with no properties takes no arguments");
        assert_eq!(empty.arguments(), [] as [Argument<'_>; 0]);
    }

    /// What a schema shows is not what it is compared by.
    ///
    /// Two schemas that differ only in their annotations and in the order their
    /// options are written are one schema to the kernel, which holds a plugin's
    /// declaration of a catalog command to the catalog's by this equality,
    /// while one that requires or allows another thing is another schema.
    #[test]
    fn what_a_schema_shows_is_not_what_it_is_compared_by() {
        let compiled = |properties: &str, required: &str| {
            Schema::compile(&closed(properties, required)).expect("inside the subset")
        };
        let written = compiled(
            r#""a":{"type":"string","title":"A","description":"one","enum":["x","y"]}"#,
            r#""a""#,
        );
        assert_eq!(
            written,
            compiled(
                r#""a":{"type":"string","title":"B","description":"two","enum":["y","x"]}"#,
                r#""a""#,
            )
        );
        assert_eq!(
            written,
            compiled(r#""a":{"type":"string","enum":["x","y"]}"#, r#""a""#)
        );
        for other in [
            compiled(r#""a":{"type":"string","enum":["x","y"]}"#, ""),
            compiled(r#""a":{"type":"string","enum":["x","z"]}"#, r#""a""#),
            compiled(r#""a":{"type":"string","enum":["x"]}"#, r#""a""#),
            compiled(r#""b":{"type":"string","enum":["x","y"]}"#, r#""b""#),
        ] {
            assert_ne!(written, other);
        }
    }

    /// A character a reader sees nothing of is refused once its escape is
    /// decoded, wherever a printer would show it: a title, a description, a
    /// property's name and an `enum`'s option, at the top and nested.
    ///
    /// **A real defect, closed here**: the text rule read the escape as the
    /// six characters a reviewer sees, so `"description":"\u001b[2J"` compiled
    /// and decoded to the escape character, which help printing what a command
    /// takes would have sent to the terminal — the screen cleared by a
    /// plugin's schema.
    #[test]
    fn a_character_a_reader_sees_nothing_of_is_refused_once_its_escape_is_decoded() {
        for (escape, hidden) in [
            ("\\u001b", '\u{1b}'),
            ("\\n", '\n'),
            ("\\t", '\t'),
            ("\\u202e", '\u{202e}'),
            ("\\u200b", '\u{200b}'),
            ("\\ufe0f", '\u{fe0f}'),
            ("\\udb40\\udc41", '\u{e0041}'),
        ] {
            for (text, at) in [
                (
                    closed(
                        &format!(r#""p":{{"type":"boolean","description":"a{escape}[2J"}}"#),
                        "",
                    ),
                    pointer(&["properties", "p", "description"]),
                ),
                (
                    closed(
                        &format!(r#""p":{{"type":"boolean","title":"a{escape}b"}}"#),
                        "",
                    ),
                    pointer(&["properties", "p", "title"]),
                ),
                (
                    format!(
                        r#"{{"type":"object","additionalProperties":false,"description":"{escape}"}}"#
                    ),
                    pointer(&["description"]),
                ),
                (
                    closed(
                        &format!(
                            r#""l":{{"type":"array","items":{{"type":"string","title":"{escape}"}}}}"#
                        ),
                        "",
                    ),
                    pointer(&["properties", "l", "items", "title"]),
                ),
                (
                    closed(
                        &format!(r#""p":{{"type":"string","enum":["ok","a{escape}"]}}"#),
                        "",
                    ),
                    pointer(&["properties", "p", "enum"]),
                ),
            ] {
                assert_eq!(
                    refusal(&text),
                    SchemaError {
                        path: at,
                        fault: SchemaFault::InvisibleCharacter(hidden)
                    },
                    "`{escape}` was accepted, decoded, in `{text}`"
                );
            }
            let named = closed(&format!(r#""no{escape}te":{{"type":"boolean"}}"#), "");
            assert_eq!(
                refusal(&named).fault,
                SchemaFault::InvisibleCharacter(hidden),
                "`{escape}` was accepted, decoded, in a property's name"
            );
        }
        // The control: an annotation, a name and an option a reader sees all
        // of compile, escapes of visible characters included.
        assert!(
            Schema::compile(&closed(
                r#""não":{"type":"string","title":"S\u00e3o","enum":["é","\u00e9x"]}"#,
                "",
            ))
            .is_ok()
        );
    }

    // --- one test per fault ---

    #[test]
    fn a_schema_that_is_not_json_is_refused() {
        for text in [
            "{",
            "",
            "{\"type\":\"object\"} trailing",
            "{'type':'object'}",
        ] {
            assert!(
                matches!(fault(text), SchemaFault::NotJson(_)),
                "`{text}` was read as JSON"
            );
        }
    }

    /// A key written twice is refused, because two readers keep different ones.
    ///
    /// `serde_json` keeps the second of two equal keys, so without this
    /// `additionalProperties` written `true` then `false` compiles as closed
    /// while any reader keeping the first sees it open.
    #[test]
    fn a_key_written_twice_is_refused_rather_than_read_as_either() {
        let twice = r#"{"type":"object","additionalProperties":true,"additionalProperties":false}"#;
        let SchemaFault::NotJson(said) = fault(twice) else {
            panic!("a repeated key was not refused as unreadable");
        };
        assert!(said.contains("additionalProperties"), "{said}");
        let nested = closed(r#""a":{"type":"string","type":"integer"}"#, "");
        assert!(matches!(fault(&nested), SchemaFault::NotJson(_)));
    }

    /// A character a reader sees nothing of is refused wherever the text holds it.
    ///
    /// With the rule standing in the catalog compiler alone, [`crate::judge`] —
    /// which the kernel asks of a plugin's command at `activate` — accepts a
    /// description spelled in tag characters and a property named with a
    /// zero-width space, measured, and the header's one rule for two callers is
    /// two. The characters are written here by hand rather than read out of the
    /// table, so a row dropped from it is a refusal this stops seeing.
    #[test]
    fn a_character_a_reader_sees_nothing_of_is_refused_in_a_schemas_text() {
        for hidden in [
            '\u{202e}',  // the right-to-left override
            '\u{202c}',  // the pop that ends it
            '\u{2066}',  // an isolate
            '\u{200b}',  // the zero-width space
            '\u{2063}',  // the invisible separator
            '\u{feff}',  // the byte-order mark
            '\u{00ad}',  // the soft hyphen
            '\u{e0041}', // a tag character, which spells `A` unseen
            '\u{e0fff}', // the tag plane's last point
            '\u{fe00}',  // the first variation selector
            '\u{fe0f}',  // the last of them
            '\u{3164}',  // the Hangul filler, a letter drawn as nothing
            '\u{115f}',  // the Hangul choseong filler
            '\u{034f}',  // the combining grapheme joiner
            '\u{180b}',  // a Mongolian free variation selector
            '\u{17b4}',  // a Khmer inherent vowel
        ] {
            for text in [
                closed(
                    &format!(r#""p":{{"type":"boolean","description":"read{hidden}me"}}"#),
                    "",
                ),
                closed(&format!(r#""no{hidden}te":{{"type":"boolean"}}"#), ""),
            ] {
                assert_eq!(
                    refusal(&text),
                    SchemaError {
                        path: String::new(),
                        fault: SchemaFault::InvisibleCharacter(hidden)
                    },
                    "`U+{:04X}` was accepted in a schema's text",
                    u32::from(hidden)
                );
            }
        }
    }

    /// A description carrying bytes as variation selectors — four bits each, so
    /// two to a byte, and drawn as nothing — is refused, as a run of tag
    /// characters is. **A real bug of this shape**: `rm -rf` as twelve
    /// selectors compiled, read by an agent and unseen by the person who
    /// reviewed the schema.
    #[test]
    fn a_description_carrying_bytes_as_variation_selectors_is_refused() {
        let smuggled: String = "rm -rf"
            .bytes()
            .flat_map(|byte| [byte >> 4, byte & 0xF])
            .filter_map(|nibble| char::from_u32(0xFE00 + u32::from(nibble)))
            .collect();
        assert_eq!(smuggled.chars().count(), 12);
        let text = closed(
            &format!(r#""p":{{"type":"boolean","description":"Saves a note.{smuggled}"}}"#),
            "",
        );
        assert!(
            matches!(
                refusal(&text).fault,
                SchemaFault::InvisibleCharacter(hidden) if (0xFE00..=0xFE0F).contains(&u32::from(hidden))
            ),
            "{:?}",
            refusal(&text)
        );
    }

    /// JSON's own whitespace is not refused, though the table's first row holds it.
    ///
    /// The control beside the refusal above: a schema written over several
    /// lines, indented with tabs, is how most schemas are written.
    #[test]
    fn a_schema_written_over_several_lines_is_not_refused() {
        let text = "{\r\n\t\"type\": \"object\",\n\t\"additionalProperties\": false\n}\n";
        assert!(
            Schema::compile(text).is_ok(),
            "JSON's own whitespace was refused as a character a reader sees nothing of"
        );
        for exempt in ['\n', '\r', '\t'] {
            assert!(
                shows_nothing(exempt),
                "the table does not hold `{}`, so exempting it proves nothing",
                exempt.escape_default()
            );
        }
    }

    #[test]
    fn a_schema_whose_top_is_not_an_object_schema_is_refused() {
        for text in [
            "[]",
            "\"hello\"",
            "42",
            "null",
            "{}",
            r#"{"type":"array","items":{"type":"string"}}"#,
            r#"{"type":["object"]}"#,
        ] {
            assert_eq!(fault(text), SchemaFault::NotAnObjectAtTheTop, "{text}");
        }
    }

    #[test]
    fn an_unknown_keyword_is_refused_wherever_it_is_written() {
        for keyword in [
            "$ref", "$schema", "oneOf", "pattern", "format", "default", "const",
        ] {
            let at_the_top =
                format!(r#"{{"type":"object","additionalProperties":false,"{keyword}":1}}"#);
            assert_eq!(
                refusal(&at_the_top),
                SchemaError {
                    path: pointer(&[keyword]),
                    fault: SchemaFault::UnknownKeyword(keyword.to_owned())
                }
            );
        }
        let nested = closed(r#""a/b":{"type":"string","pattern":"^x$"}"#, "");
        assert_eq!(
            refusal(&nested),
            SchemaError {
                path: pointer(&["properties", "a~1b", "pattern"]),
                fault: SchemaFault::UnknownKeyword("pattern".to_owned())
            },
            "the pointer is not escaped as RFC 6901 says, or points elsewhere"
        );
    }

    #[test]
    fn a_keyword_written_on_a_type_it_does_not_belong_to_is_refused() {
        for (property, keyword, type_name) in [
            (
                r#"{"type":"integer","minLength":1}"#,
                "minLength",
                "integer",
            ),
            (r#"{"type":"string","minimum":1}"#, "minimum", "string"),
            (r#"{"type":"boolean","enum":["a"]}"#, "enum", "boolean"),
            (
                r#"{"type":"string","items":{"type":"string"}}"#,
                "items",
                "string",
            ),
            (
                r#"{"type":"array","items":{"type":"string"},"required":[]}"#,
                "required",
                "array",
            ),
        ] {
            assert_eq!(
                refusal(&closed(&format!(r#""p":{property}"#), "")),
                SchemaError {
                    path: pointer(&["properties", "p", keyword]),
                    fault: SchemaFault::KeywordNotForType {
                        keyword: keyword.to_owned(),
                        type_name
                    }
                }
            );
        }
    }

    #[test]
    fn a_keyword_holding_the_wrong_shape_is_refused() {
        for (property, keyword) in [
            (r#"{"type":5}"#, "type"),
            (r#"{"type":"string","description":5}"#, "description"),
            (r#"{"type":"string","title":{}}"#, "title"),
            (r#"{"type":"string","minLength":-1}"#, "minLength"),
            (r#"{"type":"string","maxLength":1.5}"#, "maxLength"),
            (r#"{"type":"string","enum":"a"}"#, "enum"),
            (r#"{"type":"string","enum":["a",1]}"#, "enum"),
            (r#"{"type":"string","x-entity":5}"#, "x-entity"),
            (r#"{"type":"integer","minimum":"0"}"#, "minimum"),
            (r#"{"type":"integer","maximum":1.0}"#, "maximum"),
            (
                r#"{"type":"integer","maximum":18446744073709551615}"#,
                "maximum",
            ),
            (r#"{"type":"array","items":[{"type":"string"}]}"#, "items"),
            (
                r#"{"type":"array","items":{"type":"string"},"minItems":"1"}"#,
                "minItems",
            ),
            (
                r#"{"type":"array","items":{"type":"string"},"maxItems":-2}"#,
                "maxItems",
            ),
            (
                r#"{"type":"object","additionalProperties":false,"properties":[]}"#,
                "properties",
            ),
            (
                r#"{"type":"object","additionalProperties":false,"properties":{"q":5}}"#,
                "properties",
            ),
            (
                r#"{"type":"object","additionalProperties":false,"required":"q"}"#,
                "required",
            ),
            (
                r#"{"type":"object","additionalProperties":false,"required":[1]}"#,
                "required",
            ),
        ] {
            assert_eq!(
                fault(&closed(&format!(r#""p":{property}"#), "")),
                SchemaFault::WrongKeywordType(keyword),
                "{property}"
            );
        }
        // A property named twice in `required` is the same shape refused.
        assert_eq!(
            fault(&closed(r#""p":{"type":"boolean"}"#, r#""p","p""#)),
            SchemaFault::WrongKeywordType("required")
        );
    }

    #[test]
    fn a_schema_without_a_type_is_refused() {
        assert_eq!(
            refusal(&closed(r#""p":{"description":"anything at all"}"#, "")),
            SchemaError {
                path: pointer(&["properties", "p"]),
                fault: SchemaFault::TypeMissing
            }
        );
    }

    #[test]
    fn a_type_outside_the_five_is_refused() {
        for named in ["number", "null", "Object", "float"] {
            assert_eq!(
                fault(&closed(&format!(r#""p":{{"type":"{named}"}}"#), "")),
                SchemaFault::TypeUnsupported(named.to_owned())
            );
        }
    }

    #[test]
    fn an_object_that_does_not_close_itself_is_refused() {
        for top in [
            r#"{"type":"object"}"#,
            r#"{"type":"object","additionalProperties":true}"#,
            r#"{"type":"object","additionalProperties":{"type":"string"}}"#,
            r#"{"type":"object","additionalProperties":"false"}"#,
        ] {
            assert_eq!(fault(top), SchemaFault::OpenObject, "{top}");
        }
        assert_eq!(
            refusal(&closed(r#""inner":{"type":"object"}"#, "")),
            SchemaError {
                path: pointer(&["properties", "inner"]),
                fault: SchemaFault::OpenObject
            },
            "a nested object is held to the same rule as the top"
        );
    }

    #[test]
    fn required_naming_an_undeclared_property_is_refused() {
        assert_eq!(
            refusal(&closed(r#""p":{"type":"boolean"}"#, r#""p","q""#)),
            SchemaError {
                path: pointer(&["required", "1"]),
                fault: SchemaFault::RequiredNamesNoProperty("q".to_owned())
            }
        );
    }

    #[test]
    fn bounds_that_cross_are_refused_and_bounds_that_meet_are_not() {
        for (crossing, meeting, lower) in [
            (
                r#"{"type":"string","minLength":3,"maxLength":2}"#,
                r#"{"type":"string","minLength":2,"maxLength":2}"#,
                "minLength",
            ),
            (
                r#"{"type":"integer","minimum":1,"maximum":0}"#,
                r#"{"type":"integer","minimum":0,"maximum":0}"#,
                "minimum",
            ),
            (
                r#"{"type":"array","items":{"type":"boolean"},"minItems":2,"maxItems":1}"#,
                r#"{"type":"array","items":{"type":"boolean"},"minItems":1,"maxItems":1}"#,
                "minItems",
            ),
        ] {
            let text = closed(&format!(r#""p":{crossing}"#), "");
            assert_eq!(fault(&text), SchemaFault::BoundsInverted(lower));
            assert!(
                fault(&text)
                    .to_string()
                    .contains(&lower.replace("min", "max")),
                "the sentence does not name the upper bound it crossed"
            );
            assert!(
                Schema::compile(&closed(&format!(r#""p":{meeting}"#), "")).is_ok(),
                "equal bounds were refused: {meeting}"
            );
        }
    }

    #[test]
    fn an_entity_type_on_anything_but_a_string_is_refused() {
        for property in [
            r#"{"type":"integer","x-entity":"Content"}"#,
            r#"{"type":"array","items":{"type":"string"},"x-entity":"Content"}"#,
        ] {
            assert_eq!(
                fault(&closed(&format!(r#""p":{property}"#), "")),
                SchemaFault::EntityOnNonString
            );
        }
    }

    #[test]
    fn an_entity_type_that_is_not_a_cedar_type_name_is_refused() {
        for named in [
            "",
            "Content Type",
            "1Content",
            "Noctua::",
            "::Content",
            "in",
            "Ns::is",
            "__cedar::Content",
            "Cont-ent",
        ] {
            assert!(
                !is_a_cedar_type_name(named),
                "`{named}` was read as a type name"
            );
            assert_eq!(
                fault(&closed(
                    &format!(r#""p":{{"type":"string","x-entity":"{named}"}}"#),
                    ""
                )),
                SchemaFault::EntityNotATypeName(named.to_owned())
            );
        }
        for named in ["Content", "Noctua::Content", "_x", "User2", "Realm"] {
            assert!(is_a_cedar_type_name(named), "`{named}` is a type name");
        }
    }

    #[test]
    fn an_enum_that_is_empty_or_repeats_itself_is_refused() {
        for listed in ["[]", r#"["a","b","a"]"#] {
            assert_eq!(
                fault(&closed(
                    &format!(r#""p":{{"type":"string","enum":{listed}}}"#),
                    ""
                )),
                SchemaFault::EnumEmptyOrDuplicate,
                "{listed}"
            );
        }
    }

    #[test]
    fn an_array_that_does_not_say_what_it_holds_is_refused() {
        assert_eq!(
            refusal(&closed(r#""p":{"type":"array","maxItems":3}"#, "")),
            SchemaError {
                path: pointer(&["properties", "p"]),
                fault: SchemaFault::ItemsMissing
            }
        );
    }

    #[test]
    fn schemas_nested_past_the_bound_are_refused() {
        // `depth` schemas, the top one included: arrays of arrays of a boolean.
        let nested = |depth: usize| {
            let mut inner = r#"{"type":"boolean"}"#.to_owned();
            for _ in 2..depth {
                inner = format!(r#"{{"type":"array","items":{inner}}}"#);
            }
            closed(&format!(r#""p":{inner}"#), "")
        };
        assert!(
            Schema::compile(&nested(MAX_SCHEMA_DEPTH)).is_ok(),
            "a schema exactly at the bound was refused"
        );
        assert_eq!(fault(&nested(MAX_SCHEMA_DEPTH + 1)), SchemaFault::TooDeep);
    }

    // --- one test per violation ---

    #[test]
    fn a_value_of_the_wrong_type_is_refused_naming_the_type_expected() {
        let schema = closed(
            r#""o":{"type":"object","additionalProperties":false},"s":{"type":"string"},
               "i":{"type":"integer"},"b":{"type":"boolean"},
               "a":{"type":"array","items":{"type":"boolean"}}"#,
            "",
        );
        assert_eq!(
            violations(
                &schema,
                &json!({"o": [], "s": 1, "i": "1", "b": 0, "a": {}})
            ),
            [
                ViolationKind::WrongType("object"),
                ViolationKind::WrongType("string"),
                ViolationKind::WrongType("integer"),
                ViolationKind::WrongType("boolean"),
                ViolationKind::WrongType("array"),
            ]
        );
        assert_eq!(
            violations(&schema, &json!([])),
            [ViolationKind::WrongType("object")],
            "arguments that are not a map are refused as a whole"
        );
    }

    #[test]
    fn a_number_that_is_not_a_64_bit_integer_is_the_wrong_type() {
        let schema = closed(r#""i":{"type":"integer"}"#, "");
        for held in [json!(1.5), json!(1.0), json!(u64::MAX)] {
            assert_eq!(
                violations(&schema, &json!({ "i": held })),
                [ViolationKind::WrongType("integer")],
                "{held} was read as an integer"
            );
        }
    }

    #[test]
    fn a_required_argument_that_is_absent_is_named() {
        let schema = closed(
            r#""need":{"type":"boolean"},"may":{"type":"boolean"}"#,
            r#""need""#,
        );
        assert_eq!(
            violations(&schema, &json!({"may": true})),
            [ViolationKind::MissingRequired("need".to_owned())]
        );
        assert!(
            Schema::compile(&schema)
                .expect("inside the subset")
                .validate(&json!({"need": false}))
                .is_ok(),
            "an optional argument left out was refused"
        );
    }

    #[test]
    fn an_argument_nobody_declared_is_refused() {
        let schema = closed(r#""p":{"type":"boolean"}"#, "");
        assert_eq!(
            violations(&schema, &json!({"p": true, "extra": 1})),
            [ViolationKind::Unexpected("extra".to_owned())]
        );
    }

    #[test]
    fn a_string_is_measured_in_characters_and_not_in_bytes() {
        let schema = closed(r#""s":{"type":"string","minLength":2,"maxLength":3}"#, "");
        let compiled = Schema::compile(&schema).expect("inside the subset");
        // Two characters and four bytes: short in neither sense and long in bytes.
        assert!(compiled.validate(&json!({"s": "éé"})).is_ok());
        assert!(compiled.validate(&json!({"s": "ééé"})).is_ok());
        assert_eq!(
            violations(&schema, &json!({"s": "é"})),
            [ViolationKind::TooShort]
        );
        assert_eq!(
            violations(&schema, &json!({"s": "éééé"})),
            [ViolationKind::TooLong]
        );
    }

    #[test]
    fn an_integer_outside_its_bounds_is_refused() {
        let schema = closed(r#""i":{"type":"integer","minimum":-1,"maximum":1}"#, "");
        assert_eq!(
            violations(&schema, &json!({"i": -2})),
            [ViolationKind::TooSmall]
        );
        assert_eq!(
            violations(&schema, &json!({"i": 2})),
            [ViolationKind::TooLarge]
        );
        let compiled = Schema::compile(&schema).expect("inside the subset");
        for held in [-1, 0, 1] {
            assert!(compiled.validate(&json!({ "i": held })).is_ok(), "{held}");
        }
    }

    #[test]
    fn a_string_its_enum_does_not_list_is_refused() {
        let schema = closed(r#""s":{"type":"string","enum":["ltr","rtl"]}"#, "");
        assert_eq!(
            violations(&schema, &json!({"s": "up"})),
            [ViolationKind::NotOneOf]
        );
        assert!(
            Schema::compile(&schema)
                .expect("inside the subset")
                .validate(&json!({"s": "rtl"}))
                .is_ok()
        );
    }

    #[test]
    fn an_array_outside_its_counts_is_refused_and_each_item_is_checked() {
        let schema = closed(
            r#""a":{"type":"array","items":{"type":"integer","maximum":9},"minItems":1,"maxItems":2}"#,
            "",
        );
        assert_eq!(
            violations(&schema, &json!({"a": []})),
            [ViolationKind::TooFewItems]
        );
        assert_eq!(
            violations(&schema, &json!({"a": [1, 2, 3]})),
            [ViolationKind::TooManyItems]
        );
        let refused = Schema::compile(&schema)
            .expect("inside the subset")
            .validate(&json!({"a": [1, 10]}))
            .expect_err("the second item is too large");
        assert_eq!(
            refused,
            [Violation {
                path: pointer(&["a", "1"]),
                kind: ViolationKind::TooLarge
            }]
        );
    }

    #[test]
    fn a_violation_names_its_place_escaped_as_rfc_6901_says() {
        let schema = closed(r#""a/b~c":{"type":"boolean"}"#, "");
        let refused = Schema::compile(&schema)
            .expect("inside the subset")
            .validate(&json!({"a/b~c": 1}))
            .expect_err("wrong type");
        assert_eq!(refused[0].path, pointer(&["a~1b~0c"]));
        assert_eq!(refused[0].to_string(), "`/a~1b~0c`: is not a boolean");
    }

    /// Far above what the lookups below cost, and far below what a scan costs.
    const BOUND: std::time::Duration = std::time::Duration::from_secs(2);

    /// Runs `work` until one run fits inside [`BOUND`], at most three times.
    ///
    /// **A bound in time, and the only way to hold these.** No count of
    /// comparisons can be read from outside a lookup. So each case is sized so
    /// that the scan it refuses, planted back, takes at least four times the
    /// bound in a debug build, while the lookups take a small part of it. A
    /// run slowed by a loaded machine gets three tries; one past four times the
    /// bound is not a slow machine, and ends them. `scan` names what a failure
    /// means.
    fn fits_the_bound(scan: &str, work: impl Fn()) {
        let mut tries = Vec::new();
        while tries.len() < 3 {
            let started = std::time::Instant::now(); // allow-nondeterminism: `Instant` times this test's own run, and a test writes no generated output
            work();
            let took = started.elapsed(); // allow-nondeterminism: `elapsed` is the test's own measurement, and it reaches no generated output
            tries.push(took);
            if took < BOUND {
                return;
            }
            if took > BOUND * 4 {
                break;
            }
        }
        panic!("the work took {tries:?} against a bound of {BOUND:?}: {scan}");
    }

    /// Names are looked up, not scanned, when compiling and when validating.
    ///
    /// With `required` and every argument key each found by a scan of the
    /// declared names, what a schema of many names costs is their square — on
    /// the host, at `activate` and at every dispatch, with no fuel to meter it
    /// — while the crate's header says the schema alone bounds the work.
    #[test]
    fn many_names_cost_their_count_and_not_its_square() {
        /// Enough names that a scan of them costs many times a lookup.
        const NAMES: usize = 40_000;
        let named = |prefix: char| (0..NAMES).map(move |at| format!("{prefix}{at}"));
        let properties: serde_json::Map<String, Value> = named('p')
            .map(|name| (name, json!({"type": "boolean"})))
            .collect();
        let required: Vec<String> = named('p').collect();
        let text = json!({"type": "object", "additionalProperties": false,
                          "properties": properties, "required": required})
        .to_string();
        let arguments: serde_json::Map<String, Value> =
            named('q').map(|name| (name, json!(true))).collect();
        let arguments = Value::Object(arguments);
        fits_the_bound(
            "a name is being found by a scan of the declared names again",
            || {
                let schema = Schema::compile(&text).expect("inside the subset");
                let refused = schema
                    .validate(&arguments)
                    .expect_err("every key is undeclared");
                assert_eq!(refused.len(), MAX_VIOLATIONS);
            },
        );
    }

    /// An enum is a set, when compiled and when a string is looked up in it.
    ///
    /// An option refused as a duplicate by a scan of the options before it, and
    /// a string found by a scan of them all, cost the same square: measured in
    /// a debug build, 40,000 options took 4.9 s to compile, and 50,000 strings
    /// against 20,000 options took 6.1 s to validate.
    #[test]
    fn many_options_cost_their_count_and_not_its_square() {
        /// Enough options that a scan of them costs many times a lookup.
        const OPTIONS: usize = 60_000;
        /// Enough strings that a scan per string costs many times the bound.
        const STRINGS: usize = 40_000;
        let options: Vec<String> = (0..OPTIONS).map(|at| format!("e{at}")).collect();
        // The option a scan in the order written reaches last.
        let last = options.last().expect("there are options").clone();
        let text = json!({"type": "object", "additionalProperties": false,
                          "properties": {"a": {"type": "array",
                              "items": {"type": "string", "enum": options}}}})
        .to_string();
        let arguments = json!({ "a": vec![last; STRINGS] });
        fits_the_bound(
            "an option is being found by a scan of the enum again",
            || {
                let schema = Schema::compile(&text).expect("inside the subset");
                assert_eq!(schema.validate(&arguments), Ok(()));
            },
        );
    }

    /// An object costs the keys it holds, not the names its schema declares.
    ///
    /// Every object in the arguments walking every name its schema declares and
    /// looking each one up among its own keys costs the same: measured, 100,000
    /// objects of one key, against a schema of 20,000 names, took 33 s.
    #[test]
    fn many_objects_cost_their_own_keys_and_not_the_names_declared() {
        /// Enough names that walking them costs many times one lookup.
        const NAMES: usize = 20_000;
        /// Enough objects that a walk of the names per object passes the bound.
        const OBJECTS: usize = 30_000;
        let properties: serde_json::Map<String, Value> = (0..NAMES)
            .map(|at| (format!("p{at}"), json!({"type": "boolean"})))
            .collect();
        let text = json!({"type": "object", "additionalProperties": false,
                          "properties": {"a": {"type": "array",
                              "items": {"type": "object", "additionalProperties": false,
                                        "properties": properties}}}})
        .to_string();
        let schema = Schema::compile(&text).expect("inside the subset");
        let arguments = json!({ "a": vec![json!({"p0": true}); OBJECTS] });
        fits_the_bound(
            "an object is walking the names its schema declares again",
            || assert_eq!(schema.validate(&arguments), Ok(())),
        );
    }

    /// A full list stops the walk, however much of the arguments is left.
    ///
    /// Taken out, the early return that stops the walk once the list is full
    /// changes nothing a test of the list can see: the list stays at its bound
    /// either way, and only the time to fill it changes. Here every object lacks
    /// all but one of its schema's required names, so the first object fills
    /// the list, and without the return each object after it walks those names
    /// again.
    #[test]
    fn a_full_list_stops_the_walk_however_much_is_left() {
        /// Enough required names that walking them costs many times one lookup.
        const NAMES: usize = 20_000;
        /// Enough objects that a walk of the names per object passes the bound.
        const OBJECTS: usize = 5_000;
        let named = || (0..NAMES).map(|at| format!("p{at}"));
        let properties: serde_json::Map<String, Value> = named()
            .map(|name| (name, json!({"type": "boolean"})))
            .collect();
        let required: Vec<String> = named().collect();
        let text = json!({"type": "object", "additionalProperties": false,
                          "properties": {"a": {"type": "array",
                              "items": {"type": "object", "additionalProperties": false,
                                        "properties": properties, "required": required}}}})
        .to_string();
        let schema = Schema::compile(&text).expect("inside the subset");
        let arguments = json!({ "a": vec![json!({"p0": true}); OBJECTS] });
        fits_the_bound("the walk goes on descending after the list is full", || {
            let refused = schema
                .validate(&arguments)
                .expect_err("every object lacks a required name");
            assert_eq!(refused.len(), MAX_VIOLATIONS);
        });
    }

    #[test]
    fn a_refusal_lists_at_most_its_bound() {
        let schema = closed(r#""a":{"type":"array","items":{"type":"boolean"}}"#, "");
        let many: Vec<u32> = (0..10_000).collect();
        let refused = Schema::compile(&schema)
            .expect("inside the subset")
            .validate(&json!({ "a": many }))
            .expect_err("every item is the wrong type");
        assert_eq!(refused.len(), MAX_VIOLATIONS);
        assert_eq!(
            refused[0].path,
            pointer(&["a", "0"]),
            "the first found is the first listed"
        );
        // **And within one object, where nothing returns early.** The walk
        // stops descending once the list is full, which is what the items above
        // meet first; undeclared keys are noted in one loop with no descent
        // between them, so the bound on the list itself is what holds them.
        let undeclared: serde_json::Map<String, Value> = (0..1_000)
            .map(|index| (format!("k{index}"), json!(true)))
            .collect();
        let refused = Schema::compile(&schema)
            .expect("inside the subset")
            .validate(&Value::Object(undeclared))
            .expect_err("every key is undeclared");
        assert_eq!(refused.len(), MAX_VIOLATIONS);
    }

    /// A name repeated in a diagnostic is escaped and cut short.
    ///
    /// Escaped for every character the table holds and not only the controls:
    /// otherwise an argument named with a right-to-left override comes back in
    /// a violation as the override itself, turning the rest of the printed line
    /// around — and so would a property named with one written as a JSON
    /// escape, which the schema rule lets pass because the text shows six
    /// characters a reviewer can see.
    #[test]
    fn a_name_repeated_in_a_diagnostic_is_escaped_and_cut_short() {
        assert_eq!(excerpt("plain"), "plain");
        assert_eq!(excerpt("line\nbreak"), "line\\nbreak");
        for (hidden, shown) in [
            ("a\u{202e}b", "a\\u{202e}b"),
            ("a\u{e0041}b", "a\\u{e0041}b"),
            ("a\u{200b}b", "a\\u{200b}b"),
        ] {
            assert_eq!(
                excerpt(hidden),
                shown,
                "a character a reader cannot see was repeated"
            );
        }
        let overridden = Schema::compile(&closed(r#""p":{"type":"boolean"}"#, ""))
            .expect("inside the subset")
            .validate(&json!({ "ev\u{202e}il": true }))
            .expect_err("undeclared");
        assert!(
            !overridden[0].to_string().chars().any(shows_nothing),
            "an argument's name reached a violation carrying a character a reader cannot see"
        );
        let long = "x".repeat(100);
        let shown = excerpt(&long);
        assert_eq!(
            shown.chars().count(),
            65,
            "sixty-four characters and the ellipsis"
        );
        assert!(shown.ends_with('…'));
        let refused = Schema::compile(&closed(r#""p":{"type":"boolean"}"#, ""))
            .expect("inside the subset")
            .validate(&json!({ long.clone(): true }))
            .expect_err("undeclared");
        assert_eq!(refused[0].kind, ViolationKind::Unexpected(shown));
    }

    /// The pointer beside a fault repeats its names as the fault does.
    ///
    /// A fault escaped and cut short while the pointer printed beside it
    /// repeats the same name raw lets a property named with an escape sequence
    /// clear the terminal it is reported to, and a keyword a megabyte long come
    /// back as a diagnostic a megabyte long.
    #[test]
    fn a_pointer_repeats_a_name_escaped_and_cut_short() {
        // `ESC [2J` clears a terminal's screen. The name is refused for it,
        // at a pointer that repeats it escaped.
        let clearing = refusal(&closed(r#""\u001b[2Jx":{"type":"number"}"#, ""));
        let said = clearing.to_string();
        assert!(!said.chars().any(char::is_control), "{said:?}");
        assert_eq!(
            clearing.path,
            pointer(&["properties", r"\u{1b}[2Jx"]),
            "the name was dropped or mangled rather than escaped"
        );

        let long = "k".repeat(1_000_000);
        let said = refusal(&format!(
            r#"{{"type":"object","additionalProperties":false,"{long}":1}}"#
        ))
        .to_string();
        assert!(
            !said.contains(&"k".repeat(EXCERPT + 1)),
            "a name longer than a diagnostic repeats reached it whole"
        );
        // The name twice, cut short, and the sentence around it.
        assert!(said.chars().count() < 2 * (EXCERPT + 1) + 256, "{said}");

        // And in a violation, whose names are the arguments' where the schema
        // declares none of them: a schema's own names hold no control.
        let compiled = Schema::compile(&closed(r#""other":{"type":"boolean"}"#, r#""other""#))
            .expect("inside the subset");
        for instance in [json!({"\u{7}bell": 1}), json!({"other": 1})] {
            let refused = compiled.validate(&instance).expect_err("refused");
            for violation in refused {
                let said = violation.to_string();
                assert!(!said.chars().any(char::is_control), "{said:?}");
            }
        }
        the_four_other_places_a_schemas_words_come_back_are_cut_short();
    }

    /// A key written twice, a `type`, a `required` name and an `x-entity`, cut short.
    ///
    /// Each is a megabyte long and opens with `ESC [2J`. Any of the four could
    /// repeat the name raw, and a probe would then print diagnostics of a
    /// million characters carrying the escape — while the header promises a
    /// name from outside is escaped and cut short wherever a fault repeats it.
    fn the_four_other_places_a_schemas_words_come_back_are_cut_short() {
        let name = format!(r"\u001b[2J{}", "n".repeat(1_000_000));
        for (text, reached) in [
            (
                closed(
                    &format!(r#""{name}":{{"type":"boolean"}},"{name}":{{"type":"boolean"}}"#),
                    "",
                ),
                "a key written twice",
            ),
            (closed(&format!(r#""p":{{"type":"{name}"}}"#), ""), "a type"),
            (
                closed(r#""p":{"type":"boolean"}"#, &format!(r#""{name}""#)),
                "a required name",
            ),
            (
                closed(
                    &format!(r#""p":{{"type":"string","x-entity":"{name}"}}"#),
                    "",
                ),
                "an entity type",
            ),
        ] {
            let refused = refusal(&text);
            assert!(
                matches!(
                    (&refused.fault, reached),
                    (SchemaFault::NotJson(_), "a key written twice")
                        | (SchemaFault::TypeUnsupported(_), "a type")
                        | (SchemaFault::RequiredNamesNoProperty(_), "a required name")
                        | (SchemaFault::EntityNotATypeName(_), "an entity type")
                ),
                "the case for {reached} was refused as something else"
            );
            let said = refused.to_string();
            assert!(
                !said.chars().any(char::is_control),
                "{reached}: a control character reached the diagnostic"
            );
            // The name cut short, and the sentence around it.
            assert!(
                said.chars().count() < 2 * (EXCERPT + 1) + 256,
                "{reached}: the diagnostic is {} characters long",
                said.chars().count()
            );
        }
    }

    #[test]
    fn every_fault_and_violation_reads_as_a_sentence_naming_its_subject() {
        for (said, names) in [
            (
                SchemaFault::InvisibleCharacter('\u{202e}').to_string(),
                "`U+202E`",
            ),
            (SchemaFault::NotJson("EOF".to_owned()).to_string(), "EOF"),
            (SchemaFault::NotAnObjectAtTheTop.to_string(), "\"object\""),
            (
                SchemaFault::UnknownKeyword("pattern".to_owned()).to_string(),
                "`pattern`",
            ),
            (
                SchemaFault::KeywordNotForType {
                    keyword: "minLength".to_owned(),
                    type_name: "integer",
                }
                .to_string(),
                "`integer`",
            ),
            (
                SchemaFault::WrongKeywordType("items").to_string(),
                "one schema",
            ),
            (
                SchemaFault::WrongKeywordType("required").to_string(),
                "distinct",
            ),
            (
                SchemaFault::WrongKeywordType("minimum").to_string(),
                "64 signed",
            ),
            (
                SchemaFault::WrongKeywordType("maxItems").to_string(),
                "zero or more",
            ),
            (
                SchemaFault::WrongKeywordType("type").to_string(),
                "five type names",
            ),
            (
                SchemaFault::WrongKeywordType("properties").to_string(),
                "every value",
            ),
            (
                SchemaFault::WrongKeywordType("enum").to_string(),
                "list of strings",
            ),
            (
                SchemaFault::WrongKeywordType("x-entity").to_string(),
                "a string",
            ),
            (SchemaFault::TypeMissing.to_string(), "`type`"),
            (
                SchemaFault::TypeUnsupported("number".to_owned()).to_string(),
                "`number`",
            ),
            (SchemaFault::OpenObject.to_string(), "additionalProperties"),
            (
                SchemaFault::RequiredNamesNoProperty("q".to_owned()).to_string(),
                "`q`",
            ),
            (
                SchemaFault::BoundsInverted("minItems").to_string(),
                "`maxItems`",
            ),
            (SchemaFault::EntityOnNonString.to_string(), "x-entity"),
            (
                SchemaFault::EntityNotATypeName("1x".to_owned()).to_string(),
                "`1x`",
            ),
            (SchemaFault::EnumEmptyOrDuplicate.to_string(), "`enum`"),
            (SchemaFault::ItemsMissing.to_string(), "`items`"),
            (SchemaFault::TooDeep.to_string(), "16"),
            (
                ViolationKind::WrongType("integer").to_string(),
                "an integer",
            ),
            (ViolationKind::WrongType("string").to_string(), "a string"),
            (
                ViolationKind::MissingRequired("need".to_owned()).to_string(),
                "`need`",
            ),
            (
                ViolationKind::Unexpected("extra".to_owned()).to_string(),
                "`extra`",
            ),
            (ViolationKind::TooShort.to_string(), "minLength"),
            (ViolationKind::TooLong.to_string(), "maxLength"),
            (ViolationKind::TooSmall.to_string(), "minimum"),
            (ViolationKind::TooLarge.to_string(), "maximum"),
            (ViolationKind::NotOneOf.to_string(), "enum"),
            (ViolationKind::TooFewItems.to_string(), "minItems"),
            (ViolationKind::TooManyItems.to_string(), "maxItems"),
        ] {
            assert!(said.contains(names), "`{said}` does not name `{names}`");
        }
    }

    #[test]
    fn a_refusal_says_where_it_is() {
        let at_the_top = refusal("[]").to_string();
        assert!(
            at_the_top.starts_with("at the top of the schema"),
            "{at_the_top}"
        );
        let within = refusal(&closed(r#""p":{"type":"number"}"#, "")).to_string();
        assert!(within.starts_with("at `/properties/p/type`"), "{within}");
        let whole = Violation {
            path: String::new(),
            kind: ViolationKind::WrongType("object"),
        };
        assert_eq!(whole.to_string(), "the arguments: is not an object");
    }
}
