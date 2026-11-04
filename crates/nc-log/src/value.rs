//! The type wall.
//!
//! `NOCTUA.md §15.4`: the central log **must not contain user data**, because it
//! stays active in production — and that is enforced **by types, not by care**.
//! A reviewer will eventually miss an email address inside a format string; the
//! compiler will not.
//!
//! [`LogValue`] is implemented for integers, booleans, durations, error codes,
//! opaque identifier newtypes, [`Variant`] and [`Literal`] — and **for no string
//! type at all**: not `String`, not `&str`, and not `&'static str`. That absence
//! is the whole mechanism.
//!
//! # Why the lifetime is not enough
//!
//! A wall of `LogValue for &'static str` rests on the reasoning that
//! a `&'a str` borrowed from a request body or a database row cannot coerce to
//! one. True, and beside the point:
//!
//! ```ignore
//! let table: &'static str = format!("user={email}").leak();
//! ```
//!
//! `String::leak` — stable since Rust 1.72 — manufactures a real `&'static str`
//! out of anything, and the leaked value satisfies the bound exactly as a
//! literal does. Chasing method names is a race that keeps being lost:
//! `Box::leak`, `String::leak`, `Vec::leak`, `into_boxed_str`, `Box::into_raw`.
//!
//! So the type carries it instead. [`Literal`]'s field is private and its
//! constructor takes **no value at all**: it reads an associated `const` off a
//! type parameter, and const evaluation happens before any run-time value
//! exists. `const TEXT: &'static str = format!("{email}").leak();` is not a rule
//! to argue with — it is *cannot call non-const fn in constants*.
//!
//! **A macro is not the wall**, which the doc on [`Literal`] itself records: a
//! macro matching a literal *token* is got past, so the constructor takes no
//! value. It is public and its trait is deliberately unsealed, because a
//! hand-written impl can only name a `const` and a `const` cannot hold a value
//! that does not exist yet. [`Variant`] is the same shape for enum states.
//!
//! This module's header says which of the two is load-bearing, because a
//! paragraph calling the macro the wall eight lines above one saying it is not
//! would be `§0.5`'s failure inside one file.
//!
//! # Three walls, not one
//!
//! * **The types**, above: nothing outside this crate can construct a [`Field`],
//!   because every variant is `#[non_exhaustive]`.
//! * **The trait**, sealed: [`crate::Event`] requires `crate::sealed::Sealed`,
//!   which only the generated structs implement, so an outside crate cannot
//!   present something else as an event.
//! * **The source gate**, `xtask/src/wall.rs`: `format!`, `to_string()`,
//!   interpolation, `.leak()` and turning text into an identifier are all
//!   refused at a logging call site. Every compile-fail fixture declares the
//!   diagnostic it must produce, and `wall.rs`'s `SHAPES` names each one — so a
//!   fixture that is deleted is a failure rather than a smaller number.
//!
//! # What the wall does not stop, said plainly
//!
//! **A caller who deliberately encodes text into a declared type.** Measured,
//! from an ordinary consumer crate with no `format!`, no `.to_string()`, no
//! `.leak()`, no macro and no `impl Event`:
//!
//! ```text
//! let unit = nc_ids::Snowflake::from_crockford(&user_text)?;   // at run time
//! log.record(&JournalAppended { sequence: 1, work_unit: Some(unit) });
//!   -> {"_msg":"journal.appended", …, "work_unit":"CPF123456789"}
//! ```
//!
//! Twelve characters of caller-chosen text reach the store verbatim, and
//! `nc-ids`' Crockford reader is lenient enough — `O` for `0`, `I` and `L` for
//! `1`, any case — that the caller need not know the alphabet. An eleven-digit
//! CPF fits in twelve characters, which is the example
//! `nc-catalog-compiler`'s own refusal of `type = "id"` uses. The same is true
//! of `u64`: any declared type wide enough to hold a number is wide enough to
//! hold a number somebody chose.
//!
//! So the guarantee is exactly this, and not more: **a value that is a string in
//! the program cannot become a field of a log event.** That closes the shape
//! every accidental leak takes — a `String` in hand and a log line that wants
//! it. It does not close a caller who sets out to smuggle, and no arrangement of
//! types can: the source gate refuses the conversion *at the call*, where it is
//! visible, and a conversion done elsewhere is beyond what any of the three
//! walls can see.
//!
//! **And two characters that reach the line as themselves.** The line is
//! canonical JSON (RFC 8785), and that standard escapes `"`, `\` and the C0
//! controls and nothing else — so U+007F (DEL) and U+2028 (LINE SEPARATOR) are
//! written raw, because escaping them would make the output no longer canonical
//! and would move every hash `nc-canon` has pinned. What they cost:
//!
//! * U+007F renders as nothing, so `"a\u{7f}b"` reads as `ab` in the operator's
//!   UI — a field that looks like a value it is not.
//! * U+2028 is valid JSON and ends a line in JavaScript, so a consumer that
//!   evaluates the line rather than parsing it sees two.
//!
//! **Both are reachable today.** A `Literal` does not come from a source literal
//! — it comes from a `const`,
//! as this file says sixty lines above, and `include_str!(concat!(env!("OUT_DIR"),
//! "/x.txt"))` is a `const` a build script fills. Measured: a build script wrote
//! `"ana\u{7f}silva\u{2028}{\"_msg\":\"forged\"}"` into `OUT_DIR`, it went
//! through `include_str!` into a `Literal`, and both characters arrived in the
//! JSON line raw — one line to `str::lines()`, two to anything evaluating it.
//!
//! The honest boundary is not "a literal" but **when the value existed**: a
//! `const` cannot hold *user* data, because it predates the user. It can hold
//! *build-machine* data of unbounded length and arbitrary content — a file in
//! the tree, an environment variable, `stringify!` of anything, a `const fn`
//! over bytes, another crate's `const`. Every one of those was measured
//! reaching the store, including into `Origin`, which is on every line.
//!
//! For a platform whose build machine and production machine may be the same
//! machine, that distinction is thinner than "not reachable from caller data"
//! makes it sound. What holds is the shape of the attack: it takes somebody who
//! can write `build.rs`, which is somebody who can write anything.
//!
//! `§15.4` is about user data reaching a store that stays active in production.
//! Against carelessness the types are decisive. Against intent the answer is
//! review and the journal, which is a different mechanism and is the one that
//! records who did it.

use core::time::Duration;

use nc_catalog::ErrorCode;
use nc_ids::{OpaqueId, Snowflake};

/// One value, in the only shapes the central log may hold.
///
/// There is no variant for text that came from anywhere but the source. That
/// absence is the point: it is the shape every accidental leak of user data
/// takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// A count.
    #[non_exhaustive]
    Unsigned(u64),
    /// A signed number.
    #[non_exhaustive]
    Signed(i64),
    /// A flag.
    #[non_exhaustive]
    Bool(bool),
    /// A span of time, in milliseconds.
    #[non_exhaustive]
    Millis(u64),
    /// An error code, as its stable `NC-…` text.
    #[non_exhaustive]
    Code(&'static str),
    /// An opaque identifier, written as it is (`§15.4`).
    ///
    /// **No event in the shipped registry uses this kind**, and that is worth
    /// knowing rather than tidying away. `journal.appended`'s `work_unit` is
    /// a `snowflake`, because `Record::work_unit` is a `Snowflake` —
    /// typed `id`, logging one would mean converting it
    /// through `OpaqueId::new`, the single constructor in this crate that
    /// accepts arbitrary text, and a missing registry kind would push callers
    /// at the one field that takes free text.
    ///
    /// `Id` stays because an opaque identifier is a real thing to log and
    /// `§15.4`'s own table names it. The residual risk in what one may *hold*
    /// is `NOCTUA.md §5.1`, which is `[OPEN]` and owns it.
    ///
    /// **And `snowflake` carries the same residual, which is worth stating
    /// beside the sentence that recommends it.** `Snowflake::from_crockford`
    /// accepts one to thirteen characters a caller chose, so the kind the `id`
    /// refusal steers callers towards is narrower than `Id` — a fixed-width
    /// value over a 32-character alphabet rather than a bounded string of any —
    /// and not different in kind.
    ///
    /// **What reaches the store is exactly twelve characters, and they are not
    /// the ones that were typed.** *"One to thirteen … and `to_crockford`
    /// writes them back unchanged"* would overstate the channel in one
    /// direction and misdescribe it in the other. Measured against the shipped
    /// crate:
    ///
    /// ```text
    /// anasilva       -> 0000ANAS11VA      ANASILVA1234 -> ANAS11VA1234
    /// A              -> 00000000000A      oIl          -> 000000000011
    /// 8000000000000  -> refused
    /// ```
    ///
    /// The value is a `u64`, so the text is zero-padded to twelve on the way
    /// back — thirteen only when the first character is 0-7 — the confusables
    /// are folded, and the case is normalised. What survives is still a channel
    /// a caller chooses, which is the point; it is twelve characters wide and it
    /// is not a transcript. The module header says what the wall does and does
    /// not close; the source gate refuses the conversion at a logging call site,
    /// and `xtask/src/wall.rs`'s `every_public_route_to_an_identifier_is_named`
    /// is what keeps the list of those conversions from falling behind.
    #[non_exhaustive]
    Id(String),
    /// A literal from the source.
    #[non_exhaustive]
    Static(&'static str),
    /// The name of an enum variant the platform declares.
    #[non_exhaustive]
    Variant(&'static str),
}

impl Field {
    /// How the value is written into one JSON line.
    ///
    /// Through `nc-canon`, so a log line is escaped by the same code that
    /// escapes everything else at a boundary — a second escaper is a second set
    /// of edge cases, and this one guards against a value ending a line.
    #[must_use]
    pub fn to_canonical(&self) -> nc_canon::Value {
        match self {
            // **A `u64` goes out as text, and the cast is why.**
            // `i64::try_from(value).unwrap_or(-1)` turns every value at or
            // above 2^63 into the same number — so a byte count, a sequence or
            // a duration past that boundary would all read as `-1` in the
            // store, and an operator would be looking at a value nothing ever
            // had. The journal guards the same defect for exactly this reason
            // (`nc-canon` has no float for the same one: a representation that
            // quietly loses range is worse than none).
            //
            // Text rather than a wider integer because JSON has no `u64` and
            // every consumer would have to agree on how it was widened.
            Self::Unsigned(value) | Self::Millis(value) => nc_canon::Value::text(value.to_string()),
            Self::Signed(value) => nc_canon::Value::Integer(*value),
            Self::Bool(value) => nc_canon::Value::Bool(*value),
            Self::Code(text) | Self::Static(text) | Self::Variant(text) => {
                nc_canon::Value::text(*text)
            }
            Self::Id(text) => nc_canon::Value::text(text.clone()),
        }
    }
}

/// What may be a field of a log event.
///
/// **Not implemented for `String` or `&str`**, and that is the guarantee. See
/// the module docs.
pub trait LogValue {
    /// The value, in one of the shapes the log may hold.
    fn to_field(&self) -> Field;
}

impl LogValue for u64 {
    fn to_field(&self) -> Field {
        Field::Unsigned(*self)
    }
}

impl LogValue for u32 {
    fn to_field(&self) -> Field {
        Field::Unsigned(u64::from(*self))
    }
}

impl LogValue for i64 {
    fn to_field(&self) -> Field {
        Field::Signed(*self)
    }
}

impl LogValue for i32 {
    fn to_field(&self) -> Field {
        Field::Signed(i64::from(*self))
    }
}

impl LogValue for bool {
    fn to_field(&self) -> Field {
        Field::Bool(*self)
    }
}

impl LogValue for Duration {
    fn to_field(&self) -> Field {
        Field::Millis(
            // Saturating rather than collapsing — a duration past `u64::MAX`
            // milliseconds is 584 million years — but it is the same shape the
            // comment above condemns, so it says what it does rather than
            // looking like an oversight.
            //
            // **And the arm is executed here.** Measured without this,
            // `unwrap_or(0)`: the whole of `nc-log` green and `check passed`.
            // Unreachable in a running process and reachable in one line of
            // test, which is the difference between a saturation that is
            // *stated* and one that is assumed —
            // `a_duration_past_what_a_number_holds_saturates_rather_than_collapsing`
            // is the line.
            u64::try_from(self.as_millis()).unwrap_or(u64::MAX),
        )
    }
}

impl LogValue for ErrorCode {
    fn to_field(&self) -> Field {
        Field::Code(self.code())
    }
}

impl LogValue for OpaqueId {
    fn to_field(&self) -> Field {
        Field::Id(self.as_str().to_owned())
    }
}

impl LogValue for Snowflake {
    fn to_field(&self) -> Field {
        Field::Id(self.to_crockford())
    }
}

/// An enum variant's name.
///
/// A newtype rather than a bare `&'static str`, so that an enum *state* and a
/// source *literal* stay distinguishable **in this crate's types** — at the call
/// site, in [`Field`], and to anything that matches on one.
///
/// **They are not distinguishable in the log.** [`Field::to_canonical`]
/// renders `Code`, `Static` and `Variant` in one arm, so all three reach the
/// store as the same bytes and an operator filtering a query cannot tell a
/// state from a literal. Filtering is therefore not the reason the newtype
/// exists, and a `Field`-level inequality never leaves the process, so it would
/// measure nothing about the store.
///
/// What the newtype is worth is still real: `variant!` can only be handed a
/// `Source`, so a variant's name cannot be built from anything but a `const`,
/// which is the wall's rule and not a query's. Carrying the distinction onto the
/// line is a change to what the log emits, and belongs to whoever decides that.
/// `a_variant_and_a_literal_reach_the_store_as_one` pins the state of affairs
/// so the prose and the rendering cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant(&'static str);

impl Variant {
    /// Builds one from a [`Source`]. **Called only by [`crate::variant!`].**
    ///
    /// Reachable, like [`Literal::of`], and harmless for the same reason: the
    /// only thing it can read is a `const`.
    #[must_use]
    pub const fn of<S: Source>() -> Self {
        Self(S::TEXT)
    }

    /// What it holds.
    #[must_use]
    pub const fn get(self) -> &'static str {
        self.0
    }
}

/// An enum variant's name, as a log field.
///
/// ```
/// # use nc_log::variant;
/// assert_eq!(variant!("dormant").get(), "dormant");
/// ```
#[macro_export]
macro_rules! variant {
    ($text:literal) => {{
        struct Written;
        impl $crate::Source for Written {
            const TEXT: &'static str = $text;
        }
        $crate::Variant::of::<Written>()
    }};
}

impl LogValue for Variant {
    fn to_field(&self) -> Field {
        Field::Variant(self.0)
    }
}

/// A literal from the source, and the only way to get one is to write one.
///
/// **The lifetime is not enough.** `LogValue` implemented for `&'static str`
/// directly rests on the reasoning that a `&'a str` borrowed from a request
/// does not coerce to `&'static str`. That much is true, and it is not the
/// whole attack: `String::leak` and `Box::leak` both manufacture a
/// genuine `&'static str` at run time from anything at all, and the leaked
/// value satisfies the trait exactly as a literal does. A source gate can chase
/// the spellings — `Box::leak`, `String::leak`, `Vec::leak`, `into_boxed_str` —
/// and will keep losing that race, one method name at a time.
///
/// So the type carries the guarantee instead — and **the macro is not what
/// carries it either**.
///
/// A private field with a `#[doc(hidden)] pub const fn from_macro(&'static str)`
/// for the macro to call — a macro expands at the *caller's* site, so
/// everything its expansion names has to be reachable there — is a wall with a
/// public gate beside it, because `#[doc(hidden)]` hides an item from rustdoc
/// and from nothing else:
///
/// ```ignore
/// let smuggled = nc_log::Literal::from_macro(email.leak());   // it compiles
/// ```
///
/// one line, from outside the crate, no `unsafe`, no macro, no gate to notice —
/// and `cargo xtask check` has nothing to say, because a source gate cannot
/// reach a consumer's repository at all. A textual needle does not help
/// either: `const MAKE: fn(&'static str) -> Literal = Literal::from_macro;`
/// never spells `from_macro(`.
///
/// # What holds: const evaluation
///
/// [`Literal::of`] reads [`Source::TEXT`], which is an **associated const**.
/// [`crate::literal!`] declares a local type implementing [`Source`] and hands it over.
/// Anyone may write that impl by hand — the trait is deliberately not sealed,
/// since the macro implements it in the caller's crate — but they may only fill
/// it with a *constant expression*, and that is the whole guarantee:
///
/// ```ignore
/// const TEXT: &'static str = format!("user={email}").leak();
/// //                         ^ cannot call non-const fn `String::leak` in constants
/// ```
///
/// Const evaluation happens at compile time, so what a `const &'static str` can
/// hold is a literal, a `concat!`, an `env!`, an `include_str!`, or another
/// const. **Every one of those is fixed before the program runs**, which is the
/// property that is load-bearing — and it is not the same as "source": an
/// `include_str!` of a path a `build.rs` wrote reads the build machine, and a
/// `Literal` was measured carrying this machine's hostname that way. That is
/// still not a *dynamic* string and still cannot be user data, because the value
/// exists before any user does. Saying "source" would overstate it, and an
/// overstated sentence about the wall is the kind this crate guards against.
///
/// There is no const expression that carries a
/// request body, a database row, or a user's name — not because a rule forbids
/// it, but because the value does not exist yet when the const is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Literal(&'static str);

impl Literal {
    /// Builds one from a [`Source`]. **Written as [`crate::literal!`].**
    ///
    /// Reachable from anywhere, and that is fine: the only thing it can read is
    /// a `const`, and a `const` cannot hold anything that did not exist at
    /// compile time. Contrast a `from_macro(&'static str)`, which would be
    /// reachable *and* take a value.
    #[must_use]
    pub const fn of<S: Source>() -> Self {
        Self(S::TEXT)
    }

    /// What it holds.
    #[must_use]
    pub const fn get(self) -> &'static str {
        self.0
    }
}

/// Where a [`Literal`] or a [`Variant`] gets its text.
///
/// **Not sealed, on purpose.** [`crate::literal!`] implements it on a type it declares
/// inside the caller's crate, so a sealed trait could not be implemented there
/// and the macro could not work. That is not a weakness: the only thing an
/// implementer controls is a `const`, and a `const` is compile-time. Writing
/// this impl by hand is a legitimate way to name a literal — it is what the
/// macro does.
///
/// **No gate flags a hand-written one.** `no_formatting_at_a_logging_call`
/// returns zero findings for an `impl Source` block, measured, and the
/// guarantee does not need it — a `const` cannot hold a run-time value, whoever
/// writes the impl — so a doc comment asserting a gate behaviour that does not
/// exist would be the shape this crate guards against.
pub trait Source {
    /// The text, which must be a constant expression.
    const TEXT: &'static str;
}

/// A literal, as a log field.
///
/// ```
/// # use nc_log::literal;
/// let table = literal!("access_records");
/// assert_eq!(table.get(), "access_records");
/// ```
///
/// A value that is not a literal token does not match this rule and does not
/// compile. That is **not** the whole of the wall: a macro rule is got past
/// when `Literal::from_macro` is reachable from the caller's site, which is
/// what a macro expansion needs —
/// and `crates/nc-log/fixtures/a_leaked_string_cannot_reach_a_literal.rs` is
/// that attack. What holds is const evaluation, sixty lines above: the
/// constructor takes no value at all. This macro is the *convenient* door, not
/// the load-bearing one.
#[macro_export]
macro_rules! literal {
    ($text:literal) => {{
        struct Written;
        impl $crate::Source for Written {
            const TEXT: &'static str = $text;
        }
        $crate::Literal::of::<Written>()
    }};
}

impl LogValue for Literal {
    fn to_field(&self) -> Field {
        Field::Static(self.0)
    }
}

/// An optional field is optional, and nothing else changes.
impl<T: LogValue> LogValue for Option<T> {
    fn to_field(&self) -> Field {
        self.as_ref().map_or(Field::Static(""), LogValue::to_field)
    }
}

#[cfg(test)]
mod tests {
    /// Every variant of [`Field`] is `#[non_exhaustive]`, not just the one with
    /// a fixture.
    ///
    /// **The claim is held for all
    /// eight.** `a_field_cannot_be_built_from_outside.rs`
    /// constructs `Field::Id`, and its own header says *"Every variant is
    /// `#[non_exhaustive]`"* — so with the fixture alone, seven of the
    /// eight attributes could be deleted with the crate green and the
    /// compile-fail step still printing its fixture count. A second fixture per
    /// variant is eight more compiles for one rule; this is the same question
    /// asked of the source, which is where the attribute is.
    ///
    /// The needle is assembled because this test is in the file it reads.
    #[test]
    fn every_variant_of_a_field_is_non_exhaustive() {
        let here = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/value.rs"))
            .expect("this file is readable");
        let opens = here
            .find("pub enum Field")
            .expect("the enum is in this file");
        let body = &here[opens..here[opens..].find("\n}").expect("it closes") + opens];
        let variants: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.ends_with(',')
                    && line
                        .chars()
                        .next()
                        .is_some_and(|first| first.is_ascii_uppercase())
            })
            .collect();
        assert_eq!(
            variants.len(),
            8,
            "`Field` has {} variant(s) and the claim below is about eight: {variants:?}",
            variants.len()
        );
        // **On the variant, not somewhere in the enum.** Counted over the whole
        // body, any line carrying those characters would pay for a variant that
        // has lost it: a `///` line quoting it, and with `///` lines stripped,
        // a `//` one. A total is the wrong instrument for a property that is
        // about a place, and stripping one spelling of prose at a time is the
        // shape this tree keeps finding.
        //
        // Each variant answers for itself: the attribute has to be in the lines
        // immediately above it, and comments are not attributes.
        let sealed = format!("#[{}]", "non_exhaustive");
        let unsealed: Vec<&str> = variants
            .iter()
            .copied()
            .filter(|variant| {
                let at = body
                    .find(&format!("\n    {variant}"))
                    .unwrap_or_else(|| panic!("`{variant}` is a variant"));
                !body[..at]
                    .lines()
                    .rev()
                    .take_while(|line| {
                        let trimmed = line.trim_start();
                        trimmed.starts_with("///") || trimmed.starts_with("#[")
                    })
                    .any(|line| line.trim() == sealed)
            })
            .collect();
        assert!(
            unsealed.is_empty(),
            "{unsealed:?} carry no `{sealed}` of their own — a variant without it can be \
             built from outside this crate, and `String::leak` is what makes that the \
             attack this module's header names"
        );
    }

    use super::{Field, LogValue, Variant};
    use core::time::Duration;
    use nc_catalog::ErrorCode;
    use nc_ids::OpaqueId;

    /// A variant and a literal reach the store as one value.
    ///
    /// **The newtype does not keep an enum state and a source literal
    /// distinguishable *in the log*:** [`Field::to_canonical`] renders
    /// `Code`, `Static` and `Variant` in a single arm. A test comparing two
    /// `Field`s would hold a distinction the process keeps and the store never
    /// sees.
    ///
    /// This asserts what is true, so that making it false — carrying the
    /// distinction onto the line — fails here and sends whoever does it to the
    /// paragraph that has to change with it.
    #[test]
    fn a_variant_and_a_literal_reach_the_store_as_one() {
        let state = Field::Variant("dormant");
        let literal = Field::Static("dormant");
        assert_ne!(state, literal, "the two are different values in this crate");
        assert_eq!(
            state.to_canonical(),
            literal.to_canonical(),
            "they reach the store as one, and the paragraph on `Variant` says so"
        );
    }

    #[test]
    fn every_kind_the_registry_can_declare_has_an_implementation() {
        // The registry's `FieldType` and this trait are two halves of one rule,
        // and a kind the registry can name but nothing implements would fail in
        // generated source rather than here.
        assert_eq!(7_u64.to_field(), Field::Unsigned(7));
        assert_eq!(7_u32.to_field(), Field::Unsigned(7));
        assert_eq!((-7_i64).to_field(), Field::Signed(-7));
        // **And `i32`, which no registry field declares today.** Every impl is
        // named here; the registry can declare the kind, so an impl nothing
        // calls is the one that would fail in generated source.
        assert_eq!((-7_i32).to_field(), Field::Signed(-7));
        assert_eq!(true.to_field(), Field::Bool(true));
        assert_eq!(Duration::from_millis(9).to_field(), Field::Millis(9));
        // **And the arm past what a number holds.** Without this,
        // `unwrap_or(0)` there leaves the whole of `nc-log` green and
        // `check passed`: a duration that overflows would then be logged as *no
        // time at all*, which is the collapse the comment beside it says it is
        // not doing. Unreachable in a running process — 584 million years — and
        // one line to state here.
        assert_eq!(Duration::MAX.to_field(), Field::Millis(u64::MAX));
        // **And the bottom, which fourteen lines of argument about the top left
        // undecided.** `as_millis` truncates, so the whole sub-millisecond band
        // renders as `0`. Milliseconds is the resolution, and that is the
        // decision — written down here so a reader meeting `"waited":"0"` knows
        // it is the unit rather than a bug, and so that changing it is a change
        // to this line.
        //
        // **No argument about what the field can hold.**
        // `ids.sequence_exhausted` — *"This millisecond ran out of sequence
        // numbers and minting waited"* — is not "a wait bounded above by one
        // millisecond by construction": `Minter::mint` tries `WAIT_ATTEMPTS` =
        // 1,000 waits of `YIELDS_PER_WAIT` = 10,000 yields each, and
        // `snowflake.rs`'s own test doc records that loop costing **5.36 s per
        // `mint`**. What is bounded at one millisecond is the *clock advance*
        // being waited for, not the time the wait takes: 11 ms is an ordinary
        // elapsed value on a loaded or briefly descheduled host. Nothing in
        // this workspace emits that event yet, so the only place it is built is
        // the integration test.
        //
        // The pins below are about the rendering, which is the thing this file
        // decides, and they need no argument about the field.
        assert_eq!(
            Duration::from_micros(999).to_field(),
            Field::Millis(0),
            "the resolution is milliseconds, so anything under one renders as none"
        );
        // Both sides of the boundary and the boundary itself: at 1,000 µs the
        // unit is exactly one, which is the value a truncation off by one in
        // either direction moves.
        assert_eq!(Duration::from_micros(1_000).to_field(), Field::Millis(1));
        assert_eq!(Duration::from_micros(1_001).to_field(), Field::Millis(1));
        assert_eq!(
            ErrorCode::DbUnreachable.to_field(),
            Field::Code(ErrorCode::DbUnreachable.code())
        );
        assert_eq!(
            OpaqueId::new("device-1").expect("valid").to_field(), // allow-logging: OpaqueId::new here is this test proving the conversion, over a literal in this file
            Field::Id("device-1".to_owned())
        );
        assert_eq!(
            crate::literal!("a literal").to_field(),
            Field::Static("a literal")
        );
        // **Every kind the registry can declare**, `enum` among them, as this
        // test's own name promises — so collapsing `Variant` onto `Static`,
        // which `Variant`'s doc calls load-bearing, is caught.
        assert_eq!(
            Variant("dormant").to_field(),
            Field::Variant("dormant"),
            "an enum state and a source literal have to stay distinguishable"
        );
        assert_ne!(
            Variant("dormant").to_field(),
            crate::literal!("dormant").to_field(),
            "collapsing the two makes an operator unable to filter on a state"
        );
        // **And `snowflake`, which the registry declares.** The impl is three
        // lines: with its body replaced by `Field::Unsigned(0)`, every field of
        // that kind — `journal.appended`'s `work_unit`, and the `unit` of the
        // `kernel.*` events that carry one — would ship as `"0"` instead of the
        // identifier. The integration test builds `journal.appended` with
        // `work_unit: None`, so the `Some` arm never runs there.
        //
        // It matters more than one field: `Field::Id`'s own doc says
        // `work_unit` is typed `snowflake` *specifically* to steer callers off
        // the free-text constructor, so the recommendation rests on this impl.
        let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P51").expect("a valid identifier"); // allow-logging: from_crockford here is this test proving the conversion, over a literal in this file
        assert_eq!(
            unit.to_field(),
            Field::Id("01J8XK2M9P51".to_owned()),
            "a snowflake reaches the line as the text it is written as, not as a number"
        );
    }

    /// Every arm of the renderer writes what it is supposed to write.
    ///
    /// **`to_canonical` is what puts a value into the log line**,
    /// and `a_value_cannot_end_a_line_in_the_log` asserts one arm of
    /// it, `Field::Static`. Measured without this, each replaced by a constant,
    /// workspace green: `Signed`, `Bool` and `Id`. The third is the one that
    /// ships — `Field::Id` rendering as `""` blanks every identifier in every
    /// log line — and it is reachable from the shipped registry,
    /// through `journal.appended.work_unit`.
    ///
    /// Written out arm by arm rather than looped, because what each arm has to
    /// produce is the point: a `u64` as text (JSON has no `u64`), an `i64` as a
    /// number, a bool as a bool, and an identifier as the text it is written as.
    #[test]
    fn every_kind_renders_as_what_it_is() {
        let expected_code = format!("\"{}\"", ErrorCode::DbUnreachable.code());
        for (field, expected) in [
            (Field::Unsigned(7), "\"7\""),
            (Field::Millis(9), "\"9\""),
            (Field::Signed(-7), "-7"),
            (Field::Signed(0), "0"),
            (Field::Bool(true), "true"),
            (Field::Bool(false), "false"),
            // Asked of the registry rather than written out: `§15.1` refuses a
            // code as a literal outside the catalog, and asking is also the
            // stronger assertion — it is what the field is supposed to carry.
            (
                Field::Code(ErrorCode::DbUnreachable.code()),
                expected_code.as_str(),
            ),
            (Field::Static("a literal"), "\"a literal\""),
            (Field::Variant("Denied"), "\"Denied\""),
            (Field::Id("01J8XK2M9P51".to_owned()), "\"01J8XK2M9P51\""),
        ] {
            assert_eq!(
                field.to_canonical().to_canonical(),
                expected,
                "{field:?} does not reach the line as {expected}"
            );
        }
    }

    #[test]
    fn a_value_cannot_end_a_line_in_the_log() {
        // The one thing a field must never be able to do. Rendering goes through
        // `nc-canon`, which escapes control characters — so even a literal, the
        // closest the wall comes to arbitrary text, cannot forge a record.
        let field = Field::Static("one\ntwo");
        assert_eq!(field.to_canonical().to_canonical(), "\"one\\ntwo\"");
    }

    #[test]
    fn an_absent_optional_field_is_empty_rather_than_missing() {
        let absent: Option<u64> = None;
        assert_eq!(absent.to_field(), Field::Static(""));
        assert_eq!(Some(3_u64).to_field(), Field::Unsigned(3));
    }
}
