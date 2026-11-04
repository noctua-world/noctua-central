//! What one entry holds, and how it is hashed.
//!
//! Every field `NOCTUA.md §14.9` and `reference/identity-and-authorization.md
//! §11` name is here from the first line of code, and the ones no code in this
//! stage sets are the important ones — see the crate docs for why a field added
//! later invalidates every hash before it.

use nc_canon::Value;
use nc_ids::{OpaqueId, Snowflake};

/// What a caller hands the journal. Everything except the chaining.
///
/// Chaining is the journal's, not the caller's: a `Record` cannot carry a
/// sequence number or a hash, so no caller can place an entry anywhere but the
/// end or claim a link it did not compute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Which realm this happened in.
    ///
    /// `NOCTUA.md §10.3`: **every entity belongs to exactly one realm**, and the
    /// indirection exists from the first commit. It is also what a per-realm
    /// retention policy needs — `catalog/limits.toml` bounds this table from the
    /// policy pack, and a pruner cannot apply a per-realm policy to a row that
    /// does not name its realm.
    pub realm: OpaqueId,
    /// Who acted. An opaque identifier, and the type is the guarantee: this is
    /// the audit log, and a `String` here would let an email address in.
    pub principal: OpaqueId,
    /// On whose behalf, outermost first.
    ///
    /// **Delegation, never impersonation** (`§14.6`): the log shows both the
    /// agent and the operator. `§14.6` limits the depth to two, and this does
    /// **not** enforce that — the kernel refuses a deeper dispatch before it
    /// happens, and a journal that refused to *record* one would erase the
    /// evidence of the thing the limit exists to catch.
    pub actor_chain: Vec<OpaqueId>,
    /// The Cedar action name.
    pub action: String,
    /// What was acted on, as the command's resource selector extracted it.
    pub resource: Value,
    /// How it ended.
    pub result: Outcome,
    /// Which policy decided. Set by the policy decision point, from the kernel on.
    pub deciding_policy: Option<String>,
    /// The version of the policy pack that decided.
    pub policy_pack: Option<String>,
    /// The authorization epoch the decision was made at (`§14.8`).
    pub policy_epoch: Option<u64>,
    /// The grant that permitted it.
    pub grant_used: Option<OpaqueId>,
    /// The work unit this belongs to, when it is inside one.
    pub work_unit: Option<Snowflake>,
    /// When, in milliseconds since the Unix epoch.
    pub recorded_at: u64,
}

impl Record {
    /// A record with only what this stage can supply, and every authorization
    /// field left where a later stage fills it.
    ///
    /// **It takes no realm**, and that is deliberate: the journal supplies its
    /// own on every append, so a realm here would be an argument every call site
    /// passes and nothing reads.
    #[must_use]
    pub fn new(
        principal: OpaqueId,
        action: impl Into<String>,
        resource: Value,
        result: Outcome,
        recorded_at: u64,
    ) -> Self {
        Self {
            // Overwritten by `Journal::append`, which is what makes a journal
            // one realm's rather than each caller's.
            realm: principal.clone(),
            principal,
            actor_chain: Vec::new(),
            action: action.into(),
            resource,
            result,
            deciding_policy: None,
            policy_pack: None,
            policy_epoch: None,
            grant_used: None,
            work_unit: None,
            recorded_at,
        }
    }
}

/// How a dispatch ended.
///
/// **A denial is recorded, not dropped** (`NOCTUA.md §15.5`): every authorization
/// denial is an attack-surface event, and recording is not optional and is not
/// sampled. A journal that only held successes would answer "what happened" with
/// half the answer — which is why entries inside a work unit are written as they
/// happen rather than buffered until the unit commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The policy decision point allowed it and it succeeded.
    Allowed,
    /// The policy decision point denied it.
    Denied,
    /// It was allowed and then failed.
    Failed,
    /// A work unit opened. The record's resource carries its label.
    UnitBegun,
    /// A work unit committed. Everything between it and its `UnitBegun` stands.
    UnitCommitted,
    /// A work unit was abandoned. Everything between it and its `UnitBegun` was
    /// reverted by whoever could revert it.
    UnitAborted,
}

impl Outcome {
    /// How it appears in the hashed form and in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Failed => "failed",
            Self::UnitBegun => "unit-begun",
            Self::UnitCommitted => "unit-committed",
            Self::UnitAborted => "unit-aborted",
        }
    }

    /// Every outcome there is, and the only list of them.
    ///
    /// **`parse` reads this list, not a second copy nothing
    /// compares.** `as_str` above is a match the compiler forces to be
    /// complete, so a variant added to this enum must be given a spelling — and
    /// a hand-written array of six in `parse`, and in both of the tests written
    /// over it, would not follow. Measured that way: a seventh variant compiles
    /// and leaves `cargo xtask check` at `check passed` while being **writable
    /// and unreadable**. The writer spells it, the row lands,
    /// and `Postgres::entry_from` cannot read it back — so `verify`
    /// answers `EntryIsNotReadable` about a row nobody touched, which is the
    /// sentence that means somebody with write access altered the table.
    ///
    /// `the_list_of_outcomes_holds_every_variant_the_compiler_knows_about` is
    /// what keeps this complete, by counting the arms of the match above.
    pub const ALL: [Self; 6] = [
        Self::Allowed,
        Self::Denied,
        Self::Failed,
        Self::UnitBegun,
        Self::UnitCommitted,
        Self::UnitAborted,
    ];

    /// Reads one back.
    ///
    /// Named `parse` rather than `from_str` on purpose: `FromStr::from_str`
    /// returns a `Result` and is reached through `str::parse`, and an inherent
    /// method shadowing it with a different signature is a trap for a reader who
    /// knows the trait.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|outcome| outcome.as_str() == text)
    }
}

/// One entry, chained.
///
/// **Sealed against construction, because its fields undo `Record`'s
/// guarantee.** This file's own header says a `Record` "cannot carry a sequence
/// number or a hash, so no caller can place an entry anywhere but the end or
/// claim a link it did not compute" — and `Entry` carries both, `pub`,
/// while `Store::append` takes
/// an `Entry`. Unsealed, `Entry { sequence: 9_999, previous: Some([7; 32]), hash: [0; 32] }`
/// compiles, and the only thing between it and a table is `readable`, called
/// under a trait contract the trait cannot enforce.
///
/// The fields stay readable — every consumer of a history reads them — and
/// [`Entry::chain`] is the only way to make one. A test that needs a forged
/// entry has [`Entry::forged_for_tests`], which says what it is in its name.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its position. Dense and gapless: a gap would be indistinguishable from a
    /// deletion.
    pub sequence: u64,
    /// What was recorded.
    pub record: Record,
    /// The hash of the entry before it. `None` only at sequence zero.
    pub previous: Option<[u8; 32]>,
    /// This entry's hash, over its canonical form.
    pub hash: [u8; 32],
}

/// The domain the entry hash is separated by.
///
/// Versioned, and the version is part of the domain rather than a field: if the
/// hashed shape ever changes, every old entry keeps verifying under the old
/// domain and every new one under the new, instead of the whole history becoming
/// unverifiable at once.
pub const DOMAIN: &str = "noctua-central/journal/entry/v1";

impl Entry {
    /// Chains a record onto `previous` at `sequence`.
    #[must_use]
    pub fn chain(sequence: u64, previous: Option<[u8; 32]>, record: Record) -> Self {
        let mut entry = Self {
            sequence,
            record,
            previous,
            hash: [0; 32],
        };
        entry.hash = entry.compute_hash();
        entry
    }

    /// The hash this entry's contents produce.
    ///
    /// Compared against the stored `hash` by verification. Everything that makes
    /// the entry what it is goes in, including the sequence number and the link
    /// — otherwise two entries with the same contents at different positions
    /// would hash the same, and one could be moved onto the other's link.
    #[must_use]
    pub fn compute_hash(&self) -> [u8; 32] {
        nc_canon::hash(DOMAIN, &self.hashed_form())
    }

    fn hashed_form(&self) -> Value {
        // **Destructured exhaustively, with no `..`.** A field added to `Record`
        // and not added below is a field no test can see: it would not change any
        // hash, so every tamper test still passes while the new value can be
        // edited in the table invisibly. The crate header calls that out as the
        // hazard; this makes it a compile error at the one site that has to
        // know, rather than something whoever adds the field must remember.
        let Record {
            realm,
            principal,
            actor_chain,
            action,
            resource,
            result,
            deciding_policy,
            policy_pack,
            policy_epoch,
            grant_used,
            work_unit,
            recorded_at,
        } = &self.record;
        // **And `Entry`'s own fields, for the same reason.** The paragraph
        // above is about `Record`, and the two fields read through `self.` —
        // `sequence` and `previous` — are read by field access, so without this
        // `Entry` would have no such compile error. Measured:
        // `pub decided_by: String` added to `Entry`, initialised in `chain`, in
        // `forged_for_tests` and in `postgres.rs`'s row reader so it
        // round-trips through the shipped store, gives `check passed` at the
        // baseline — and rewriting it leaves the digest unmoved. The kernel
        // populates the authorization fields and `§14.9` adds more, which is
        // where that bites.
        //
        // `hash` is excluded by name: it is the output of this function, and a
        // hash over itself is not a hash.
        let Self {
            sequence,
            record: _,
            previous,
            hash: _,
        } = self;
        Value::object([
            // **Every `u64` goes in as text.**
            // `i64::try_from(x).unwrap_or(i64::MAX)` collapses every value at
            // or above 2^63 onto one — so two entries differing only above that
            // boundary would hash identically, and a tamper of `recorded_at` or
            // `policy_epoch` verify clean. A saturating cast inside a hash is a
            // collision by construction, and two of these fields carry values
            // that come from outside this crate.
            ("sequence", Value::text(sequence.to_string())),
            (
                "previous",
                previous.map_or(Value::Null, |hash| Value::text(nc_canon::to_hex(&hash))),
            ),
            ("realm", Value::text(realm.to_string())),
            ("principal", Value::text(principal.to_string())),
            (
                "actor_chain",
                Value::Array(
                    actor_chain
                        .iter()
                        .map(|actor| Value::text(actor.to_string()))
                        .collect(),
                ),
            ),
            ("action", Value::text(action.clone())),
            ("resource", resource.clone()),
            ("result", Value::text(result.as_str())),
            ("deciding_policy", optional(deciding_policy.as_deref())),
            ("policy_pack", optional(policy_pack.as_deref())),
            (
                "policy_epoch",
                policy_epoch.map_or(Value::Null, |epoch| Value::text(epoch.to_string())),
            ),
            (
                "grant_used",
                grant_used
                    .as_ref()
                    .map_or(Value::Null, |grant| Value::text(grant.to_string())),
            ),
            (
                "work_unit",
                work_unit.map_or(Value::Null, |unit| Value::text(unit.to_crockford())),
            ),
            ("recorded_at", Value::text(recorded_at.to_string())),
        ])
    }

    /// An entry with a hash somebody else chose. **For tests that forge one.**
    ///
    /// `Entry` is `#[non_exhaustive]`, so a struct literal outside this crate
    /// does not compile — which is the point: a hash a caller supplies is a
    /// link it did not compute. The tamper suite exists to prove such an entry
    /// is refused, and this is the door it goes through, named so that a reader
    /// of a call site can see what is happening.
    ///
    /// **Behind the feature.** This crate's manifest says `test-support` is
    /// "off by default, so a shipped library cannot be tampered with through
    /// its own public surface" — and this door is the one that walks straight
    /// past `#[non_exhaustive]`, taking the hash as an argument,
    /// while `Store::append` takes an `Entry`. Measured with a `const _:`
    /// binding under `#[cfg(not(any(test, feature = "test-support")))]`:
    /// without the gate, `cargo build -p nc-journal` compiles, and the sentence
    /// in the manifest is false. The gate that holds it is
    /// in `every_hatch_carries_its_reason`, and it asks the question of every
    /// crate that declares such a feature rather than of this door alone.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[must_use]
    pub const fn forged_for_tests(
        sequence: u64,
        previous: Option<[u8; 32]>,
        record: Record,
        hash: [u8; 32],
    ) -> Self {
        Self {
            sequence,
            record,
            previous,
            hash,
        }
    }

    /// The hash as lowercase hex, which is how it appears in a database row.
    #[must_use]
    pub fn hash_hex(&self) -> String {
        nc_canon::to_hex(&self.hash)
    }
}

fn optional(value: Option<&str>) -> Value {
    value.map_or(Value::Null, Value::text)
}

/// Every edit that must change an entry's hash, in one place.
///
/// **Behind the `test-support` feature, and shared rather than copied.** The
/// tamper suite is an integration test, so it cannot reach a list inside
/// a `#[cfg(test)]` module — and a second copy claiming in its own header that
/// the two cannot drift would be a claim that drifts. A list a test can
/// actually import is the only version of that claim that is true.
///
/// Adding a field to [`Record`] means adding it here, and both suites then cover
/// it without either being edited.
#[cfg(any(test, feature = "test-support"))]
pub mod tamper {
    use super::{Entry, Outcome};
    use nc_canon::Value;
    use nc_ids::OpaqueId;

    /// One named edit to an entry, and what it is called.
    pub type Edit = (&'static str, fn(&mut Entry));

    fn id(text: &str) -> OpaqueId {
        OpaqueId::new(text).expect("a test identifier")
    }

    /// The same text with every letter's case flipped, and nothing else moved.
    ///
    /// **What every edit below would otherwise miss.** Each replaces a value
    /// with a *different* value, so a `hashed_form` that normalised the field
    /// before hashing it would still be injective on every input this list
    /// owns: measured, `Value::text(principal.to_string().to_lowercase())`
    /// in `hashed_form` passes the rest of `nc-journal`'s tests and
    /// prints `check passed` byte-identical to a clean run,
    /// while `OpaqueId::new("Operator-1")` and `"operator-1"` hash the same
    /// and `verify` answers `Ok` over the swap. Seven weakenings of that family
    /// survive without it — `realm`, `principal`, `actor_chain`,
    /// `deciding_policy`, `policy_pack` and `grant_used` lower-cased,
    /// and `realm` and `principal` truncated to twelve characters — because
    /// every identifier this suite writes is already lower-case and short, so
    /// every normalising rendering is the identity on every input.
    ///
    /// `nc-ids` accepts `[A-Za-z0-9-_.:]` verbatim and normalises nothing, so
    /// `Realm-1` and `realm-1` are two realms and a stored one can be respelled
    /// to a victim's with the digest unmoved. An edit that moves only the case
    /// is the one input on which a case-folding rendering is not the identity.
    ///
    /// ASCII, because that is the whole of what `OpaqueId::new` admits.
    fn recased(text: &str) -> String {
        text.chars()
            .map(|character| {
                if character.is_ascii_uppercase() {
                    character.to_ascii_lowercase()
                } else {
                    character.to_ascii_uppercase()
                }
            })
            .collect()
    }

    /// The same text with a space on the end.
    ///
    /// **The identifier fields cannot carry one and three fields
    /// can.** `realm`, `principal`, `actor_chain` and `grant_used`
    /// are `OpaqueId`, whose grammar `[A-Za-z0-9-_.:]` excludes whitespace — so
    /// trimming those genuinely is the identity, and [`recased`] is the whole
    /// story for them. `action`, `deciding_policy` and `policy_pack` are
    /// unconstrained `String`s, and every other value this suite writes into
    /// them is already trimmed ASCII.
    ///
    /// Measured with neither these edits nor the whitespace shapes of
    /// `crates/nc-journal/tests/tampering.rs`: `hashed_form` rendering those
    /// three through `str::trim` — `("action", Value::text(action.trim()))` and
    /// the two `Option`s through `map(str::trim)` — gives `check passed` at the
    /// baseline verdict line for line, while two records differing only in a
    /// trailing space on `action` land on one digest. An earlier entry's action
    /// can then be rewritten from `Delete` to `Delete ` and `verify`
    /// answers `Ok`.
    ///
    /// The property is (field × normalisation class), not field
    /// alone; `the_corpus_can_tell_a_normalisation_from_the_identity`
    /// in `crates/nc-journal/tests/tampering.rs` walks the product.
    fn padded(text: &str) -> String {
        format!("{text} ")
    }

    /// The same edit at the other end.
    ///
    /// **A class needs both ends.** [`padded`] appends, and without this so
    /// would every whitespace-carrying value in
    /// `crates/nc-journal/tests/tampering.rs` — so a `hashed_form` that trimmed
    /// only the *front*, `action.trim_start()`, would be the identity on every
    /// record and every edit there, and two records differing by a leading
    /// space on `action` would land on one digest with the whole suite green
    /// and `check passed` at its baseline. That corpus's own normalisation
    /// class is `str::trim`, which is symmetric and cannot tell the two ends
    /// apart either.
    fn led(text: &str) -> String {
        format!(" {text}")
    }

    /// Every field of a record, and one edit that changes it.
    ///
    /// **This list carries criterion 3's "any mutation of any field" clause**,
    /// and its floor carries the list. Emptied, it would leave the whole
    /// of `nc-journal` green: `changing_any_field_changes_the_hash`
    /// and `changing_any_field_of_any_entry_breaks_verification` both loop over
    /// it, so an empty list means zero iterations and two tests that assert
    /// nothing. Dropping a single field — `policy_epoch` — would be green too.
    ///
    /// `hashed_form` is protected by exhaustive destructuring, which is a
    /// compile error when a field is added. This is protected by
    /// `every_field_of_a_record_has_an_edit` in this file's own `#[cfg(test)]`
    /// module, which is a test failure when one is dropped. Both halves are
    /// needed: the first says a new field reaches the hash, the second says
    /// somebody proved it can be noticed.
    ///
    /// **Named and not linked**, because the target is a `#[cfg(test)]` item
    /// and rustdoc can never resolve it: as an intra-doc link it is
    /// an `unresolved link` error the moment anything documents this module,
    /// and `cargo doc` with no features does not, because `tamper` is
    /// behind `test-support`.
    ///
    /// # Panics
    ///
    /// When one of its own fixed test values is malformed, which is a defect in
    /// this function rather than in anything a caller did.
    #[must_use] // allow-logging: `edits` hands back a list of field names written in this file, one per row
    pub fn edits() -> Vec<Edit> {
        vec![
            ("realm", |entry| entry.record.realm = id("realm-2")),
            ("principal", |entry| entry.record.principal = id("attacker")),
            ("action", |entry| {
                "Delete".clone_into(&mut entry.record.action);
            }),
            ("resource", |entry| {
                entry.record.resource = Value::text("something-else");
            }),
            ("result", |entry| {
                // Toggled, not set: the history alternates outcomes, so
                // assigning a constant is a no-op at every position that already
                // held it — and a no-op edit makes a test pass for having
                // changed nothing.
                entry.record.result = match entry.record.result {
                    Outcome::Allowed => Outcome::Denied,
                    _ => Outcome::Allowed,
                };
            }),
            ("actor_chain", |entry| {
                entry.record.actor_chain.push(id("agent-x"));
            }),
            ("deciding_policy", |entry| {
                entry.record.deciding_policy = Some("core.deny_by_default".to_owned());
            }),
            ("policy_pack", |entry| {
                entry.record.policy_pack = Some("family-3".to_owned());
            }),
            ("policy_epoch", |entry| entry.record.policy_epoch = Some(9)),
            ("grant_used", |entry| {
                entry.record.grant_used = Some(id("grant-4"));
            }),
            ("work_unit", |entry| {
                entry.record.work_unit = Some(
                    nc_ids::Snowflake::from_crockford("00000000000J").expect("a test identifier"),
                );
            }),
            ("recorded_at", |entry| entry.record.recorded_at += 1),
            // The one a saturating cast cannot catch at all: every `u64` going
            // into the hash through one, anything at or above 2^63 collapses
            // onto a single value.
            ("recorded_at past the signed range", |entry| {
                entry.record.recorded_at = u64::MAX;
            }),
            // **Every identifier field, respelled in another case and in
            // nothing else.** See `recased` for the seven weakenings this
            // family exists to kill: each of the edits above moves a field to a
            // *different* value, which a case-folding or truncating rendering
            // still distinguishes, so none of them can tell an injective
            // rendering from a normalising one.
            ("realm in another case", |entry| {
                entry.record.realm = id(&recased(entry.record.realm.as_str()));
            }),
            ("principal in another case", |entry| {
                entry.record.principal = id(&recased(entry.record.principal.as_str()));
            }),
            ("actor_chain in another case", |entry| {
                for actor in &mut entry.record.actor_chain {
                    *actor = id(&recased(actor.as_str()));
                }
            }),
            ("deciding_policy in another case", |entry| {
                entry.record.deciding_policy = entry.record.deciding_policy.as_deref().map(recased);
            }),
            ("policy_pack in another case", |entry| {
                entry.record.policy_pack = entry.record.policy_pack.as_deref().map(recased);
            }),
            ("grant_used in another case", |entry| {
                entry.record.grant_used = entry
                    .record
                    .grant_used
                    .as_ref()
                    .map(|grant| id(&recased(grant.as_str())));
            }),
            // **And the three fields that are not identifiers**, whose values
            // an `OpaqueId`'s grammar does not bound. See `padded`: a rendering
            // that trims these is otherwise the identity on every value this
            // suite holds.
            ("action with its whitespace moved", |entry| {
                entry.record.action = padded(&entry.record.action);
            }),
            ("deciding_policy with its whitespace moved", |entry| {
                entry.record.deciding_policy = entry.record.deciding_policy.as_deref().map(padded);
            }),
            ("policy_pack with its whitespace moved", |entry| {
                entry.record.policy_pack = entry.record.policy_pack.as_deref().map(padded);
            }),
            // And the same three at the front, because "whitespace" that only
            // ever means "trailing whitespace" is half a class. See [`led`].
            ("action with whitespace led onto it", |entry| {
                entry.record.action = led(&entry.record.action);
            }),
            ("deciding_policy with whitespace led onto it", |entry| {
                entry.record.deciding_policy = entry.record.deciding_policy.as_deref().map(led);
            }),
            ("policy_pack with whitespace led onto it", |entry| {
                entry.record.policy_pack = entry.record.policy_pack.as_deref().map(led);
            }),
            // **And a character outside ASCII**, which no other edit here
            // moves. Every other value this list writes is ASCII, so a
            // rendering that keeps only ASCII would be the identity on all of
            // them: `action` rendered through `chars().filter(char::is_ascii)`
            // would pass every other edit here with `"Delete"` colliding with
            // `"Delete\u{2028}"`.
            // `recased`'s own doc says ASCII is the whole of what
            // `OpaqueId::new` admits — and these three fields are `String`s
            // that admit anything.
            ("action outside ASCII", |entry| {
                entry.record.action.push('\u{2028}');
            }),
            ("deciding_policy outside ASCII", |entry| {
                entry.record.deciding_policy = entry
                    .record
                    .deciding_policy
                    .as_deref()
                    .map(|policy| format!("{policy}\u{2028}"));
            }),
            ("policy_pack outside ASCII", |entry| {
                entry.record.policy_pack = entry
                    .record
                    .policy_pack
                    .as_deref()
                    .map(|pack| format!("{pack}\u{2028}"));
            }),
        ]
    }
}

/// Where a string literal starting at `at` ends, if one starts there.
///
/// Raw and not: `#[doc = r"…"]` is a doc comment written the other way, and it
/// is how anyone writes one whose text holds a quote.
///
/// **Two of its clauses cannot be told apart by any input**, and here is why,
/// so that a survivor is not mistaken for one nobody looked at.
///
/// **The closing hashes.** Returning `cursor + 1` instead of
/// `cursor + hashes + 1` leaves the reader inside the literal's terminator —
/// and a terminator is `#` characters only, which this scanner steps over
/// exactly as it steps over any other character that is not a quote, a
/// backslash or a brace. Every input that reaches the shorter return reaches
/// the same state one character later.
///
/// **An unterminated literal.** The final `Some(characters.len())` answers for
/// text that opens a string and never closes it. The only caller is
/// [`the_body_of_the_struct`], which is handed the source of this file between
/// a `pub struct … {` and its matching brace — text rustc has already accepted,
/// in which no literal is unterminated. Written `None` the whole crate stays
/// green, and it stays green because the arm is unreachable rather than because
/// nothing asks.
#[cfg(any(test, feature = "test-support"))]
fn past_a_literal(characters: &[char], at: usize) -> Option<usize> {
    let raw = characters.get(at) == Some(&'r');
    let mut hashes = 0_usize;
    let mut cursor = at;
    if raw {
        cursor += 1;
        while characters.get(cursor) == Some(&'#') {
            hashes += 1;
            cursor += 1;
        }
    }
    if characters.get(cursor) != Some(&'"') {
        return None;
    }
    cursor += 1;
    while cursor < characters.len() {
        match characters[cursor] {
            '\\' if !raw => cursor += 2,
            '"' if (1..=hashes).all(|step| characters.get(cursor + step) == Some(&'#')) => {
                return Some(cursor + hashes + 1);
            }
            _ => cursor += 1,
        }
    }
    Some(characters.len())
}

/// The text between a struct's own braces, with those braces matched.
///
/// **`split_once("\n}")` is not that: it cuts the struct short.** The body
/// would end at the first newline followed by a brace at column zero — and
/// a `}` at column zero is what the *inside* of a multi-line string literal
/// holds whenever somebody writes one. Measured, a `source_address` field added
/// to [`Record`] under
///
/// ```text
/// #[doc = "Where the dispatch came from. The shape of a source address is
/// }
///     one of a small set."]
/// ```
///
/// and threaded through `Record::new`, the exhaustive destructure
/// as `source_address: _` and `postgres.rs`'s row reader, so it round-trips
/// through the shipped store: through that cut, every other test
/// of `nc-journal` is green, including `every_field_of_a_record_has_an_edit`,
/// whose whole job is to say that every field of a record has an edit that
/// proves a change to it is noticed. The control — the same field under an
/// ordinary `///` line — fails that test at once. The field is stored, is read
/// back, and is hashed by nothing, which is acceptance criterion 3 false for
/// it.
///
/// A `//` line, a `/* … */` comment and a string literal are all spent here
/// rather than counted, so a brace inside any of them is not a brace. That
/// leaves nothing in a struct body that can hold an unmatched one. The block
/// form is the second spelling of this defect: see the comment on that branch
/// for the field it would hide past a green suite.
///
/// # Panics
///
/// When the struct is not in this file, or does not close in it — both defects
/// in this function or its caller rather than in anything the tree does.
#[cfg(any(test, feature = "test-support"))]
fn the_body_of_the_struct(source: &str, named: &str) -> String {
    let after = source
        .split_once(&format!("pub struct {named} {{"))
        .expect("the struct is in this file")
        .1;
    let characters: Vec<char> = after.chars().collect();
    let mut depth = 1_usize;
    let mut at = 0_usize;
    let mut body = String::new();
    while at < characters.len() {
        let here = characters[at];
        if here == '/' && characters.get(at + 1) == Some(&'/') {
            while at < characters.len() && characters[at] != '\n' {
                body.push(characters[at]);
                at += 1;
            }
            continue;
        }
        // **And the block form, which is the same hole respelled.** A `/* … */`
        // that is not spent like a `//` line lets a `}` inside one close the
        // struct at `depth == 0`, and every field below it leaves the list. The
        // doc gate does not cover it either: `opens_a_block_doc_comment`
        // refuses `/**` and `/*!` and leaves a plain `/*` alone.
        //
        // Measured, a `source_address: String` added to [`Record`] under
        //
        // ```text
        // /* Filled in by the kernel from the dispatch envelope; the shape of a
        // } source address is one of a small set the kernel decides.
        // */
        // ```
        //
        // and threaded through `Record::new`, `postgres.rs`'s row reader and
        // the exhaustive destructure as `source_address: _`: without the block
        // form spent, `cargo fmt` clean, clippy clean, **`nc-journal`'s tests
        // green** and `cargo xtask check` printing `check passed` with every
        // semantic number identical to a clean run. The field is stored, read
        // back through the shipped store, and hashed by nothing.
        //
        // Nested, because Rust nests block comments and rustc counts them.
        if here == '/' && characters.get(at + 1) == Some(&'*') {
            //
            // **Spent, and not kept.** The `//` branch above keeps its text and
            // the field reader drops any line opening with `//`; a block
            // comment's inner lines open with whatever the author wrote, so
            // keeping them would let `/* note: a remark */` read as a field
            // called `note`. Only the newlines are kept, so the body still has
            // the shape of the source.
            let mut comments = 0_usize;
            while at < characters.len() {
                if characters[at] == '/' && characters.get(at + 1) == Some(&'*') {
                    comments += 1;
                    at += 2;
                } else if characters[at] == '*' && characters.get(at + 1) == Some(&'/') {
                    comments -= 1;
                    at += 2;
                    if comments == 0 {
                        break;
                    }
                } else {
                    if characters[at] == '\n' {
                        body.push('\n');
                    }
                    at += 1;
                }
            }
            assert!(
                comments == 0,
                "`{named}` holds a block comment that does not close, so its fields \
                 cannot be read"
            );
            continue;
        }
        // An `r` that continues a word is part of a name, not a raw literal.
        let inside_a_word =
            at > 0 && (characters[at - 1].is_alphanumeric() || characters[at - 1] == '_');
        let opens = (here == '"' || (here == 'r' && !inside_a_word))
            .then(|| past_a_literal(&characters, at))
            .flatten();
        if let Some(past) = opens {
            body.extend(&characters[at..past]);
            at = past;
            continue;
        }
        if here == '{' {
            depth += 1;
        } else if here == '}' {
            depth -= 1;
            if depth == 0 {
                return body;
            }
        }
        body.push(here);
        at += 1;
    }
    panic!("`{named}` does not close in this file, so its fields cannot be read")
}

/// The names of a struct's fields, read out of this file.
///
/// The struct is the authority on what a record holds; a hand list beside it is
/// a second copy, and two hand lists agree about a field neither knows. Read
/// with a scanner rather than a macro because the fields carry doc comments and
/// attributes, and a derive would have to be written into the shipped type to
/// serve a test.
///
/// **Behind `test-support` rather than `#[cfg(test)]`, for the reason
/// [`tamper`] gives.** The tamper suite is an integration test and could not
/// reach a `#[cfg(test)]` item, which would leave `tampering.rs`'s field
/// comparator — `every_field_and_whether_it_differs` — a *third* hand list,
/// pinned against `MUST_HOLD`'s union and against the derived `PartialEq` and
/// against nothing that reads `Record`. Measured that way: the four `epoch …`
/// shapes deleted with their four `MUST_HOLD` rows and the comparator's
/// one `policy_epoch` line, and `policy_epoch` put back through a saturating
/// cast — every assertion in that file holds, `cargo xtask check` is
/// byte-identical to a clean run, and `verify` answers `Ok` for
/// any `policy_epoch` respelled at or above 2^63. The sentence four paragraphs
/// up names the trap: two hand lists agree about a field neither knows.
///
/// # Panics
///
/// When `named` is not a struct declared in this file, or when the scan reads
/// so few fields out of it that the list it feeds would prove nothing — both of
/// which are defects in this function or in its caller rather than in anything
/// the tree does.
#[cfg(any(test, feature = "test-support"))]
#[must_use]
pub fn fields_of_the_struct(named: &str) -> Vec<String> {
    let source = include_str!("entry.rs");
    let body = the_body_of_the_struct(source, named);
    let body = body.as_str();
    // **Every field, whatever its visibility.** `strip_prefix("pub ")` misses
    // `pub(crate) source_address: String`, and a private field too. Such a
    // field is stored, is bound by the exhaustive destructure below (as
    // `source_address: _`, which warns about nothing), and is hashed by
    // nothing: measured under that prefix, added to `Record` and initialised in
    // `Record::new`, `cargo xtask check` prints `check passed`,
    // `every_field_of_a_record_has_an_edit` passes, the tamper suite passes —
    // and rewriting that field leaves the digest unmoved, so `verify` reports a
    // forged history clean. That is acceptance criterion 3 turned false by one
    // word of visibility.
    //
    // A field is a line inside the struct body that binds a name to a type. An
    // attribute, a doc comment and a nested `#[cfg]` are not.
    let found: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//") && !line.starts_with('#'))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, _)| {
            name.rsplit(|character: char| character.is_whitespace())
                .next()
                .unwrap_or(name)
                .to_owned()
        })
        .filter(|name| {
            // **A raw identifier is a name.** `pub r#type: String` is rustfmt's
            // own spelling — `type` is a keyword and this is how Rust writes a
            // field called that — and `#` is not alphanumeric, so read as one
            // the field would be dropped here, stored, bound by the exhaustive
            // destructure as `r#type: _`, and hashed by nothing. Measured that
            // way: added to `Record` and initialised,
            // `every_field_of_a_record_has_an_edit` passes, the tamper suite
            // passes, and `cargo xtask check` is green. Unlike the
            // two-fields-on-one-line and same-line-`#[cfg]` variants, this one
            // survives `cargo fmt --check`, which is what makes it the one
            // worth closing here rather than leaving to the formatter.
            let bare = name.strip_prefix("r#").unwrap_or(name);
            !bare.is_empty()
                && bare
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_')
        })
        .collect();
    assert!(
        found.len() > 3,
        "only {} field(s) were read out of `{named}`, so this scanner has stopped \
         working and the list it feeds proves nothing: {found:?}",
        found.len()
    );
    found
}

#[cfg(test)]
mod tests {

    /// Nothing a struct body may hold can close it early.
    ///
    /// **The comment forms are the same defect three
    /// times.** `split_once("\n}")` cuts the body at a `}` inside a string
    /// literal; a `#[doc = "…}…"]` attribute does it again once literals are
    /// spent; and a plain `/* … */` does it a third time for a scanner that
    /// spends `//` lines and knows nothing of the block form —
    /// and `opens_a_block_doc_comment` in the doc gate refuses `/**` and `/*!`
    /// while leaving a plain `/*` alone.
    ///
    /// Measured for the block form: a `source_address: String` added to
    /// [`super::Record`] under a `/* … } … */` comment, threaded
    /// through `Record::new`, `postgres.rs`'s row reader and the exhaustive
    /// destructure as `source_address: _` — `cargo fmt` clean, clippy clean,
    /// the rest of **`nc-journal`'s tests green** and `cargo xtask check`
    /// printing `check passed` with every semantic number identical to a clean
    /// run. The field is stored, is read back through the shipped store, and is
    /// hashed by nothing: acceptance criterion 3 false for it.
    ///
    /// And the other direction, which spending block comments could itself
    /// break: a comment is *spent*, so nothing written inside one is read as a field.
    #[test]
    fn nothing_a_struct_may_hold_closes_it_early() {
        for (how, hidden) in [
            ("a line comment", "    // }\n"),
            ("a block comment", "    /* }\n    */\n"),
            ("a nested block comment", "    /* /* } */ }\n    */\n"),
            ("a doc attribute", "    #[doc = \"}\"]\n"),
            ("a raw doc attribute", "    #[doc = r#\"}\"#]\n"),
            // **A raw literal holding the quote that would end an ordinary
            // one**, which is the only shape the raw branch decides. The row
            // above it is spent by the plain-literal path anyway — measured,
            // the whole raw branch written `let raw = false;` leaves every
            // other test green, that row included. Here the inner `"` closes a
            // plain literal early, the `}` after it is then read as the
            // struct's, and every field below leaves the list: stored,
            // destructured as `_`, and hashed by nothing.
            (
                "a raw doc attribute holding a quote",
                "    #[doc = r#\"a\"}\"#]\n",
            ),
            (
                "a raw doc attribute with two hashes",
                "    #[doc = r##\"a\"#}\"##]\n",
            ),
            // **The escape, which only a plain literal has.** `\\"` inside one
            // does not close it, and stepping one character instead of two
            // reads the quote it escapes as the end — after which the `}` is
            // the struct's. In a *raw* literal there are no escapes, so a
            // trailing backslash must not swallow the closing quote either.
            (
                "an escaped quote in a doc attribute",
                "    #[doc = \"a\\\"}\"]\n",
            ),
            (
                "a raw doc attribute ending in a backslash",
                "    #[doc = r#\"a\\\"#]\n    // }\n",
            ),
            // A two-hash literal, for the shape rather than for a clause:
            // counting the closing hashes short is an *equivalent*, and
            // `past_a_literal`'s own doc says why — the characters it then
            // re-reads are `#`, which this scanner steps over like any other.
            // The row is here so the shape is in the corpus, not because it
            // decides anything.
            (
                "a raw doc attribute whose hashes must be stepped over",
                "    #[doc = r##\"a\"##]\n    #[doc = \"}\"]\n",
            ),
            ("a literal in a default", "    // let it = \"}\";\n"),
        ] {
            let source = format!("pub struct Probe {{\n{hidden}    pub last: String,\n}}\n");
            let body = super::the_body_of_the_struct(&source, "Probe");
            assert!(
                body.contains("pub last: String"),
                "`{how}` closed the struct early, so every field below it left the \
                 list: such a field is stored, is bound by the exhaustive destructure \
                 as `_`, and is hashed by nothing"
            );
        }

        // And a comment is spent: what is written inside one is not a field.
        let source = "pub struct Probe {\n    /* note: a remark */\n    pub last: String,\n}\n";
        let body = super::the_body_of_the_struct(source, "Probe");
        assert!(
            !body.contains("note"),
            "the text of a comment survived into the struct body, so `note: a remark` \
             would be read as a field this record does not have"
        );
    }

    use super::{Entry, Outcome, Record};
    use nc_canon::Value;
    use nc_ids::OpaqueId;

    fn id(text: &str) -> OpaqueId {
        OpaqueId::new(text).expect("a test identifier")
    }

    /// The record every test in this module edits.
    ///
    /// **Every optional identifier field is filled in.** `Record::new` leaves
    /// `actor_chain` empty and the four optionals `None`, and an edit that
    /// respells a field it is *named* for cannot respell an absent one — a
    /// `deciding_policy` case edit over `None` sets a value instead, which any
    /// rendering distinguishes from `Null`, so it would prove nothing about the
    /// rendering. The case edits in `tamper::edits()` are the whole of what
    /// separates an injective `hashed_form` from a normalising one, and they
    /// need something to move.
    fn record() -> Record {
        let mut record = Record::new(
            id("user-1"),
            "Read",
            Value::text("content-7"),
            Outcome::Allowed,
            1_700_000_000_000,
        );
        record.actor_chain = vec![id("agent-1")];
        record.deciding_policy = Some("core.allow_read".to_owned());
        record.policy_pack = Some("family-1".to_owned());
        record.grant_used = Some(id("grant-1"));
        record
    }

    #[test]
    fn an_entry_hashes_its_own_contents() {
        let entry = Entry::chain(0, None, record());
        assert_eq!(entry.hash, entry.compute_hash());
    }

    /// `sequence` is hashed as the whole number it is, not a prefix of it.
    ///
    /// **Every rendering floor this crate owns is scoped
    /// to `Record`.** `CARRIES_TEXT` is `fields_of_the_struct("Record")`
    /// minus `result`, and the truncation walk, the variant walk, the key walk
    /// and the nesting walk are all indexed by those names — so `Entry`'s own
    /// two hashed fields sit outside all of it. `previous` survives that gap
    /// because the test below pins its rendered value; without this `sequence`
    /// has no value-level pin at all and is fed nothing but a single digit,
    /// plus two extremes that differ in their first one.
    ///
    /// Measured: rendering it `sequence.to_string().chars().take(3)` — the
    /// truncation class the corpus already walks at every length for all
    /// eleven `Record` fields — leaves `cargo xtask check`
    /// printing `check passed`, with all of `tampering.rs`, which *is*
    /// acceptance criterion 3, green, while `Entry::chain(1003, …)`
    /// and `Entry::chain(1007, …)` over the same record land on one digest. The
    /// sentence `hashed_form` states — two entries with the same contents at
    /// different positions do not hash the same — would be decided only
    /// on `{0, 1, 2, i64::MAX, u64::MAX}`.
    #[test]
    fn the_position_is_hashed_as_the_whole_number_it_is() {
        for sequence in [0_u64, 1, 1003, 1007, 20_250_917, u64::MAX] {
            let entry = Entry::chain(sequence, Some([7; 32]), record());
            let Value::Object(pairs) = &entry.hashed_form() else {
                panic!("the hashed form of an entry is an object");
            };
            assert_eq!(
                pairs.get("sequence"),
                Some(&Value::text(sequence.to_string())),
                "`sequence` is hashed as text, and a rendering that loses part of \
                 the number lets two positions share a digest"
            );
        }
        // The consequence, so the pin above is not the only thing that says it:
        // two positions that agree in their first three digits.
        assert_ne!(
            Entry::chain(1003, Some([7; 32]), record()).hash,
            Entry::chain(1007, Some([7; 32]), record()).hash,
            "the same contents at two positions landed on one digest"
        );
    }

    /// `prev_hash` is excused because `previous` carries its **value**.
    ///
    /// **The excuse is a claim, so it is checked.** Its list says, per column,
    /// the hashed key the value reaches the digest under — and an assertion
    /// asking only whether the hashed form has *a* key of that name lets any
    /// name do: measured, the shipped pair changed
    /// to `("prev_hash", "principal")` leaves such a test `ok`. Carried through
    /// — a `decided_by` column added to `Entry`, the destructure, the `INSERT`,
    /// the row reader and a migration — `nc-journal`'s whole suite would then
    /// go green, including the tampering tests that *are* acceptance
    /// criterion 3, while that column is rewritten in place through the shipped
    /// store with `verify` clean.
    ///
    /// So the one excuse that names a key is proved rather than asserted, and
    /// the list is closed: a third entry needs its own proof.
    #[test]
    fn the_link_is_hashed_under_the_name_it_is_excused_by() {
        // **Thirty-two bytes that differ, because thirty-two that do not prove
        // nothing about a rendering.** Against `Some([7; 32])`, every rendering
        // that is the identity on a constant array but lossy on a real digest
        // walks through: measured, rendering the link as
        // `to_hex(&[hash[0]; 32])` throws away thirty-one of its thirty-two
        // bytes and such an assertion — the one named for the excuse — stays
        // green, along with the unit tests and all but one of the tampering
        // tests, whose link-width loop flips bytes 1, 2, 4, 16 and 31 and so
        // never touches the one that survives.
        let link: [u8; 32] = core::array::from_fn(|byte| {
            u8::try_from(byte)
                .unwrap_or(0)
                .wrapping_mul(7)
                .wrapping_add(3)
        });
        let linked = super::Entry::chain(1, Some(link), record());
        let form = linked.hashed_form();
        let Value::Object(pairs) = &form else {
            panic!("the hashed form of an entry is an object");
        };
        assert_eq!(
            pairs.get("previous"),
            Some(&Value::text(nc_canon::to_hex(
                &linked.previous.expect("a link")
            ))),
            "`prev_hash` is excused because `previous` carries its value, and `previous` \
             does not carry it"
        );
    }

    /// One excused column, proved by the value the digest carries.
    ///
    /// **Not whether *a* key of that name exists**: asked that
    /// way, `("prev_hash", "principal")` passes — and so does a third entry
    /// with any hashed key's name beside it, which is the whole distance
    /// between this list and a column the store persists outside the digest. A
    /// check that hardcodes `previous` and never reads the list stands next to
    /// the excuse rather than proving it.
    fn the_excuse_is_carried_by_value(column: &str, hashed_as: &str) {
        // value, because a name beside the column is not a proof.
        let linked = super::Entry::chain(1, Some([7; 32]), record());
        let form = linked.hashed_form();
        let Value::Object(under) = &form else {
            panic!("the hashed form of an entry is an object");
        };
        let carried = match column {
            "prev_hash" => linked
                .previous
                .map(|hash| Value::text(nc_canon::to_hex(&hash))),
            // **Loud, not `None`.** A `None` arm would invert the assertion
            // below: for any column but `prev_hash` it would require the hashed
            // form *not* to carry a key of that name — the inverse of the
            // sentence it prints — so a third excuse naming a real hashed key
            // would fail and one naming a key that does not exist pass. Loud, a
            // third entry cannot be added without writing its proof here.
            other => panic!(
                "`{other}` is excused by a name, and this function knows how to prove \
                 only `prev_hash`. Write the proof beside the excuse"
            ),
        };
        assert_eq!(
            under.get(hashed_as),
            carried.as_ref(),
            "`{column}` is excused because the hashed form carries its value as \
             `{hashed_as}`, and `{hashed_as}` does not carry it. A name beside the \
             column is not a proof: any hashed key's name would satisfy one"
        );
    }

    /// Every column the store's `SELECT` list puts back into an `Entry`.
    ///
    /// **The `INSERT` is not the only list.** `postgres.rs` names columns in
    /// two places: `const COLUMNS` is the read list behind `tail()`
    /// and `from()`, and it decides what a row becomes. Measured with
    /// the `INSERT` scanned alone: a `decided_by` added to `COLUMNS`, to the
    /// row reader and to a migration but *not* to the `INSERT`
    /// leaves `nc-journal` green — including the tampering tests that are
    /// acceptance criterion 3 —
    /// while `ALTER TABLE … ALTER COLUMN … TYPE text USING` rewrites the column
    /// in every row with `verify` clean.
    fn every_column_the_store_reads_is_hashed(
        store: &str,
        present: &[&str],
        written_but_not_a_hashed_key: &[&str],
    ) {
        let read_list = store
            .split_once("const COLUMNS: &str = \"")
            .expect("the store names the columns it reads")
            .1
            .split_once("\";")
            .expect("the read list closes")
            .0;
        let mut selected = 0_usize;
        for item in read_list.split(',') {
            let item = item.replace('\\', " ");
            let item = item.split(" AS ").next().unwrap_or_default();
            let Some(column) = item.split_whitespace().next_back() else {
                continue;
            };
            let column = column.split("::").next().unwrap_or(column);
            if column.is_empty() {
                continue;
            }
            selected += 1;
            if written_but_not_a_hashed_key.contains(&column) {
                continue;
            }
            assert!(
                present.contains(&column),
                "the store reads `{column}` and the hashed form does not carry it, so \
                 that column can be rewritten in the table with every digest unmoved"
            );
        }
        assert!(
            selected > 12,
            "{selected} column(s) were read out of the store's SELECT list, so this is \
             not reading it and the cross-check above is over an empty list"
        );
    }

    /// Every field of `Entry` is hashed, or is named here as not hashed.
    ///
    /// **`Record` has two guards, and this gives `Entry` one.** `hashed_form`
    /// destructures the record exhaustively
    /// and `every_field_of_a_record_has_an_edit` reads the struct out of the
    /// source; `Entry`'s own fields, read through `self.`, are not a compile
    /// error when a field is added. Measured without
    /// this, `pub decided_by: String` added to `Entry` and threaded
    /// through `chain`, `forged_for_tests` and `postgres.rs`'s row reader so it
    /// round-trips through the shipped store: `check passed` at the exact
    /// baseline, with the field editable in the table and the digest
    /// unmoved. That is acceptance criterion 3 — "*any* mutation of *any*
    /// earlier entry breaks verification" — false for that field.
    ///
    /// The exhaustive `let Self { .. }` in `hashed_form` is the compile-time
    /// half. This is the other half: it says which fields the destructure is
    /// allowed to discard, so `record: _` and `hash: _` are a decision written
    /// down rather than two underscores nobody reads.
    #[test]
    fn every_field_of_an_entry_is_hashed_or_named() {
        // `record` is hashed field by field through its own destructure, and
        // `hash` is what this function produces.
        const NOT_HASHED_DIRECTLY: [&str; 2] = ["record", "hash"];

        let declared = super::fields_of_the_struct("Entry");
        let hashed = super::Entry::chain(0, None, record()).hashed_form();
        let Value::Object(pairs) = &hashed else {
            panic!("the hashed form of an entry is an object");
        };
        let present: Vec<&str> = pairs.keys().map(String::as_str).collect();
        let mut missing: Vec<&String> = Vec::new();
        for field in &declared {
            if NOT_HASHED_DIRECTLY.contains(&field.as_str()) {
                continue;
            }
            if !present.contains(&field.as_str()) {
                missing.push(field);
            }
        }
        assert!(
            missing.is_empty(),
            "`Entry` declares {missing:?}, which the hashed form does not carry and \
             `NOT_HASHED_DIRECTLY` does not excuse. A field outside the hash can be \
             rewritten in the table with every tamper test green, because no digest \
             moves — which is acceptance criterion 3 false for that field. Hash it, \
             or say here why it is not hashed"
        );
        // The excuses are excuses for something: a name here that `Entry` does
        // not declare is a stale exemption, and an exemption nobody reads is how
        // the next field gets added to it.
        for excused in NOT_HASHED_DIRECTLY {
            assert!(
                declared.iter().any(|field| field == excused),
                "`{excused}` is excused from the hash and `Entry` has no such field"
            );
        }

        // **And the excuse list is read by the store, not only by itself.** The
        // list above is two names in an array, and adding a third is one token
        // — measured without this, `pub decided_by: String` added to `Entry`,
        // threaded through `chain`, `forged_for_tests` and `postgres.rs`'s row
        // reader, bound as `decided_by: _` in the destructure and named here:
        // `check passed`, and the pinned golden digests still matching, which
        // is itself the proof the field is outside the hash while being `pub`,
        // stored, and read back.
        //
        // What makes a field dangerous is that the store persists it, so the
        // store's own `INSERT` is the second reader: a column it names is a
        // column somebody can rewrite in the table, and it has to be under the
        // digest. `sequence` and `prev_hash` are `Entry`'s two hashed fields
        // under the names the schema gives them; `hash` is the digest itself.
        // **The column, and the hashed key it reaches the digest under.** A
        // bare pair of names with none of the guards its sibling
        // `NOT_HASHED_DIRECTLY` has — no stale-exemption check, no floor, no
        // count — lets one token undo the whole cross-check above it: the same
        // `pub decided_by`, threaded through `chain`, `forged_for_tests`, the
        // row reader and the `INSERT`, and then added here, would pass this
        // crate and the tampering suite that *is* acceptance criterion 3. An
        // excuse list is only as good as what reads it.
        //
        // So each entry says what makes it safe, and the assertion below decides
        // it: a column is excused only if the value it carries is hashed under
        // the name beside it, or if it is the digest itself. `prev_hash` is
        // `previous`; `hash` is the output of this function, and a hash over
        // itself is not a hash.
        let written_under_another_name = [("prev_hash", "previous"), ("hash", "")];
        for (column, hashed_as) in written_under_another_name {
            if hashed_as.is_empty() {
                assert_eq!(
                    column, "hash",
                    "`{column}` is excused with no hashed key beside it, and `hash` is \
                     the only column that may be — it is what this function produces"
                );
                continue;
            }
            the_excuse_is_carried_by_value(column, hashed_as);
        }
        // A third entry needs its own proof, above — so adding one is a change
        // in two places that somebody reads.
        assert_eq!(
            written_under_another_name.len(),
            2,
            "the excuse list grew and the loop above knows how to prove two columns"
        );
        let written_but_not_a_hashed_key: Vec<&str> = written_under_another_name
            .iter()
            .map(|(column, _)| *column)
            .collect();
        let store = include_str!("postgres.rs");
        // Anchored on the first column name, because the paragraph above the
        // statement writes `INSERT INTO journal_entries (…)` as prose.
        let after = store
            .split_once("INSERT INTO journal_entries (sequence")
            .expect("the store writes an INSERT naming its columns")
            .1;
        let columns = after.split_once(')').expect("the column list closes").0;
        let mut persisted = 1_usize;
        for column in columns.split(',').skip(1) {
            // The literal is written across lines with `\` continuations, so a
            // column name arrives with the continuation, the newline and the
            // next line's indentation around it.
            let column = column
                .replace('\\', " ")
                .split_whitespace()
                .next_back()
                .unwrap_or_default()
                .to_owned();
            if column.is_empty() {
                continue;
            }
            let column = column.as_str();
            persisted += 1;
            if written_but_not_a_hashed_key.contains(&column) {
                continue;
            }
            assert!(
                present.contains(&column),
                "the store writes `{column}` and the hashed form does not carry it, so \
                 that column can be rewritten in the table with every digest unmoved. \
                 `NOT_HASHED_DIRECTLY` cannot excuse a column the store persists — the \
                 excuse and the danger are the same thing"
            );
        }
        every_column_the_store_reads_is_hashed(store, &present, &written_but_not_a_hashed_key);

        assert!(
            persisted > 12,
            "{persisted} column(s) were read out of the store's INSERT, so this is not \
             reading it and the cross-check above is over an empty list"
        );
    }

    #[test]
    fn every_field_of_a_record_has_an_edit() {
        // **The floor `edits()` needs.** Without it, emptying the list leaves
        // every test that uses it green, because they loop over it — so the
        // clause criterion 3 is written in ("*any* mutation of *any* field")
        // would be carried by a list nothing counted.
        //
        // The names are written out here rather than derived, so a field
        // renamed in `edits()` and not here is a failure rather than a silent
        // agreement.
        const FIELDS: [&str; 12] = [
            "realm",
            "principal",
            "actor_chain",
            "action",
            "resource",
            "result",
            "deciding_policy",
            "policy_pack",
            "policy_epoch",
            "grant_used",
            "work_unit",
            "recorded_at",
        ];

        // **Read from the struct, not only written here.** `FIELDS` and
        // `tamper::edits()` are two hand lists, so they agree by construction
        // about any field *neither* knows: adding a real hashed field to
        // `Record` — struct, `Record::new`, and the exhaustive destructure in
        // `hashed_form` — would leave a test of those two green, and the tamper
        // suite green with it. The only thing that fires then is the golden
        // vector, whose message points the author at the hashed shape and not
        // at the missing proof:
        //
        // ```text
        // the hashed shape of an entry changed, and every entry already written
        // is now unverifiable
        //   left:  18a52a01…   right: ec9aa6c2…
        // ```
        //
        // The natural repair — paste the new hex — leaves the new field hashed
        // with no edit that changes it, and criterion 3's "*any* mutation of
        // *any* field" then covers twelve fields of thirteen. `entry.rs`'s own
        // paragraph says the destructure "makes it a compile error at the one
        // site that has to know": it makes the *hashing* a compile error, not
        // the *proof*.
        //
        // The kernel populates the authorization fields and `§14.9` adds more,
        // so this is precisely where it bites.
        let declared = super::fields_of_the_struct("Record");
        for field in &declared {
            assert!(
                FIELDS.contains(&field.as_str()),
                "`Record` holds `{field}` and this list does not name it, so nothing \
                 proves a change to it would be noticed"
            );
        }
        assert_eq!(
            declared.len(),
            FIELDS.len(),
            "`Record` holds {} field(s) and this list names {}: {declared:?}",
            declared.len(),
            FIELDS.len()
        );

        let edits = super::tamper::edits();
        for field in FIELDS {
            assert!(
                edits.iter().any(|(name, _)| *name == field),
                "`{field}` is hashed and no edit changes it, so nothing proves a change \
                 to it would be noticed"
            );
        }
        // One field may carry more than one edit — `recorded_at` has a second
        // at the far end of the signed range — so the count is a floor and the
        // *names* are the set.
        let names: std::collections::BTreeSet<&str> = edits
            .iter()
            .map(|(name, _)| name.split(' ').next().unwrap_or(name))
            .collect();
        assert_eq!(
            names,
            FIELDS
                .into_iter()
                .collect::<std::collections::BTreeSet<&str>>(),
            "`edits()` and this list disagree about what a record holds"
        );
        assert!(
            edits.len() >= FIELDS.len(),
            "fewer edits than fields, so one is untested"
        );

        // **And each edit changes the field it is named for.** Everything above
        // compares *names* — `FIELDS` against the struct, `FIELDS` against the
        // labels `edits()` carries — and this ties a label to what its closure
        // does. Measured without it: the edit labelled `grant_used` changed to
        // edit `record.action` instead leaves the whole workspace green,
        // because `changing_any_field_of_any_entry_breaks_verification` and
        // `changing_any_field_changes_the_hash` both only require that
        // *something* changed. "Any mutation of any field" would be proved as
        // "some mutation of some field".
        //
        // Read from the canonical form, which is where a hashed field is
        // visible by name: apply the edit to a fresh record and require the
        // value under that key to be the one that moved.
        for (label, edit) in &edits {
            let name = label.split(' ').next().unwrap_or(label);
            let before = Entry::chain(1, Some([7; 32]), record());
            let mut after = Entry::chain(1, Some([7; 32]), record());
            edit(&mut after);
            let (was, now) = (shape_of(&before), shape_of(&after));
            let moved: Vec<&String> = was
                .iter()
                .zip(now.iter())
                .filter(|((_, old), (_, new))| old != new)
                .map(|((key, _), _)| key)
                .collect();
            assert_eq!(
                moved.len(),
                1,
                "the edit labelled `{label}` changed {} field(s) of the hashed form, and \
                 an edit that changes two proves nothing about either: {moved:?}",
                moved.len()
            );
            assert_eq!(
                moved[0], name,
                "the edit labelled `{label}` changes `{}`. Every test that loops over \
                 `edits()` only asks whether *something* changed, so a label and its \
                 closure could name different fields and the suite would say `any field \
                 at any position` about eleven of twelve",
                moved[0]
            );
        }
    }

    /// The hashed form of an entry, as pairs of key and rendered value.
    ///
    /// The canonical form is where a hashed field is visible *by name*, which is
    /// what lets an edit be checked against the field it claims to change.
    fn shape_of(entry: &Entry) -> Vec<(String, String)> {
        let rendered = entry.hashed_form().to_canonical();
        // The canonical form is one object with the field names as keys; read
        // as text so a value of any shape compares the same way.
        rendered
            .trim_start_matches('{')
            .trim_end_matches('}')
            .split_inclusive(',')
            .filter_map(|pair| pair.split_once(':'))
            .map(|(key, value)| (key.trim_matches('"').to_owned(), value.to_owned()))
            .collect()
    }

    #[test]
    fn changing_any_field_changes_the_hash() {
        // Field by field, because a hashed form that forgot one would still pass
        // a test that only changed the action.
        let base = Entry::chain(1, Some([7; 32]), record());
        for (field, edit) in super::tamper::edits() {
            let mut changed = base.clone();
            edit(&mut changed);
            let rehashed = Entry::chain(changed.sequence, changed.previous, changed.record.clone());
            assert_ne!(
                base.hash, rehashed.hash,
                "`{field}` is not covered by the hash, so it can be edited invisibly"
            );
        }
        assert_ne!(
            base.hash,
            Entry::chain(2, Some([7; 32]), record()).hash,
            "sequence"
        );
        assert_ne!(
            base.hash,
            Entry::chain(1, Some([8; 32]), record()).hash,
            "previous"
        );
    }

    #[test]
    fn a_value_past_the_signed_range_still_changes_the_hash() {
        // Through a saturating cast to `i64`, every `u64` at or above 2^63
        // collapses onto one value and a tamper in that range verifies clean.
        // Two of these fields carry values that come from outside this crate.
        let mut high = record();
        high.recorded_at = i64::MAX as u64;
        let mut higher = record();
        higher.recorded_at = u64::MAX;
        assert_ne!(
            Entry::chain(0, None, high).hash,
            Entry::chain(0, None, higher).hash
        );
        assert_ne!(
            Entry::chain(i64::MAX as u64, None, record()).hash,
            Entry::chain(u64::MAX, None, record()).hash
        );
    }

    #[test]
    fn the_same_contents_at_two_positions_hash_differently() {
        // Otherwise an entry could be lifted from one position onto another's
        // link and the chain would still verify.
        let one = Entry::chain(1, Some([7; 32]), record());
        let two = Entry::chain(2, Some([7; 32]), record());
        assert_ne!(one.hash, two.hash);
    }

    /// Every spelling an outcome is written as, by hand.
    ///
    /// **Three of the six are in no other assertion and in no
    /// migration.** `migrations/0001_journal.sql` writes `result TEXT NOT NULL`
    /// with no `CHECK`, so nothing on disk records the
    /// vocabulary; `unit-committed` and `unit-aborted` are written out in the
    /// crate's PostgreSQL tests and `allowed` is in the golden vector,
    /// and `denied`, `failed` and `unit-begun` are held only here. Without
    /// this, each could be respelled with the whole suite green.
    ///
    /// `an_outcome_round_trips_through_its_text`
    /// asserts `parse(as_str(x)) == Some(x)` — an expectation derived from the
    /// thing it tests, and true of any injective mapping, `"zzz-1"` included.
    ///
    /// What a respelling costs is written in `unit.rs`, about the sibling
    /// constants: **there is no migration for a hashed column.** `denied`
    /// renamed makes every denial already written unverifiable and invisible to
    /// every reader grepping `result = 'denied'` — and `NOCTUA.md §15.5` names
    /// denials as the events that must never be lost. `unit-begun` renamed
    /// makes `unit_opened` miss brackets already in the table, so an open unit
    /// reads as never opened and a **second** opening bracket is accepted,
    /// which is the one thing that guard exists to refuse.
    #[test]
    fn every_outcome_is_written_the_way_the_table_already_holds_it() {
        for (outcome, spelled) in [
            (Outcome::Allowed, "allowed"),
            (Outcome::Denied, "denied"),
            (Outcome::Failed, "failed"),
            (Outcome::UnitBegun, "unit-begun"),
            (Outcome::UnitCommitted, "unit-committed"),
            (Outcome::UnitAborted, "unit-aborted"),
        ] {
            assert_eq!(
                outcome.as_str(),
                spelled,
                "a hashed column has no migration: every entry already written with \
                 `{spelled}` becomes unverifiable, and every query naming it stops \
                 matching"
            );
            assert_eq!(Outcome::parse(spelled), Some(outcome));
        }
    }

    /// `ALL` holds every variant the compiler knows about.
    ///
    /// **The only complete list of an enum's variants is a match the compiler
    /// forces**, and `as_str` is one. Everything else here — `ALL`, and the
    /// written spellings below — is a hand list, so the two are compared: the
    /// arms of that match are counted and held against the length of the list
    /// `parse` reads. A variant added to the enum gives the compiler an error
    /// in `as_str` and nothing else; without this, giving it a spelling there
    /// would be enough to make the whole workspace green with the outcome
    /// unreadable.
    #[test]
    fn the_list_of_outcomes_holds_every_variant_the_compiler_knows_about() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/entry.rs"))
            .expect("this file is a file in this crate");
        let opens = "pub const fn as_str(self) -> &'static str {";
        let at = source.find(opens).expect("the match this test counts");
        let body = &source[at..];
        let body = &body[..body.find("\n    }\n").expect("the function ends")];
        let arms = body.matches("=> \"").count();
        // The floor: a scan that read no arm would agree with any list at all.
        assert!(
            arms > 0,
            "no arm was read out of `as_str`, so this test compares nothing"
        );
        assert_eq!(
            arms,
            Outcome::ALL.len(),
            "`as_str` spells {arms} outcome(s) and `ALL` names {}. The compiler forces \
             the first list and nothing forces the second, so the difference is an \
             outcome a writer can spell and no reader can read back",
            Outcome::ALL.len()
        );
        // And no two of them are one outcome, which a count cannot see.
        let spellings: std::collections::BTreeSet<&str> = Outcome::ALL
            .iter()
            .map(|outcome| outcome.as_str())
            .collect();
        assert_eq!(
            spellings.len(),
            Outcome::ALL.len(),
            "two entries of `ALL` spell the same outcome, so it stands for fewer than \
             it appears to"
        );
    }

    #[test]
    fn an_outcome_round_trips_through_its_text() {
        for outcome in Outcome::ALL {
            assert_eq!(Outcome::parse(outcome.as_str()), Some(outcome));
            // **And no other spelling of the same outcome**, which is the
            // fourth member of a family whose other three are guarded in
            // `postgres.rs`. The argument there is written out once and applies
            // here unchanged: *the chain hashes a value and the table stores its
            // text, so a reader that accepts more spellings than the writer
            // emits is a row that can be respelled without moving the digest.*
            //
            // Measured with `parse` made case-insensitive and nothing here: the
            // whole workspace stays green, and
            // `ALTER TABLE journal_entries ALTER COLUMN result TYPE text USING
            // upper(result)` then rewrites the heap, fires no row trigger — 0001
            // gives the column no `CHECK`, and this file says so — and every
            // reader of `WHERE result = 'denied'`, the query `§15.5` exists for,
            // finds nothing while `verify` answers `Ok`.
            for respelling in [
                outcome.as_str().to_uppercase(),
                outcome.as_str().to_ascii_uppercase(),
                format!(" {}", outcome.as_str()),
                format!("{} ", outcome.as_str()),
            ] {
                if respelling == outcome.as_str() {
                    continue;
                }
                assert_eq!(
                    Outcome::parse(&respelling),
                    None,
                    "`{respelling}` reads as `{}`, so the column can be respelled without \
                     moving the digest",
                    outcome.as_str()
                );
            }
        }
        assert_eq!(Outcome::parse("something else"), None);
    }
}
