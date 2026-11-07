//! The policy decision point, as a library the kernel embeds (`NOCTUA.md §14.2`).
//!
//! A [`Pdp`] holds the schema, the invariant core and the one pack a realm's
//! profile selects, compiled by `nc_policy::compile` from text embedded when
//! this crate was built. [`Pdp::decide`] answers one [`Question`] — a principal,
//! an action, a resource and the [`Facts`] the kernel vouches for — against a
//! [`Slice`] of the entities it names, which a [`Directory`] hands out. The
//! answer is a [`Decision`]: allowed or not, and what decided, which is what
//! the journal records (`§14.9`).
//!
//! **This is the decision point, not the enforcement point.** The enforcement
//! point is `commands.dispatch` in the kernel, and it decides the order —
//! budgets, schema, resource, then this — and journals every answer. Nothing
//! here refuses a dispatch or writes a record.
//!
//! # What it answers (`§2.4`)
//!
//! **Every failure denies.** A pack that does not load is a [`PdpError`], never
//! an empty pack that denies by accident. A request the schema refuses is
//! denied as [`sentinel::REQUEST_INVALID`] before any policy runs, because Cedar
//! skips a policy whose condition errs and answers from the rest, so a `forbid`
//! reading a context key the kernel forgot would allow. **And any evaluation
//! error denies**, even when Cedar answered `Allow`: an erring `forbid` is one
//! Cedar skipped, and a permit that won because the rule against it could not be
//! read is the failure `policy/core/README.md` records.
//!
//! **Packs are embedded, not read.** A kernel decides with what it was built
//! with, which the gate held to its cases; nothing on disk at run time is a
//! policy. A realm pinning a pack this build does not carry is denied
//! ([`Pdp::carries`]), comparing the realm's profile as well as its pin.
//!
//! **An entity the slice does not hold is denied, and named as little as it can
//! be.** A principal it does not hold is denied as if nothing permitted it,
//! without asking Cedar: the principal is the one asking. A resource it does not
//! hold is denied whatever Cedar answers, and named by the forbids that fired,
//! as a held one would be: every permit reading an attribute of a missing entity
//! errs, and a denial named for the error — or one naming no permit where a held
//! resource meets a forbid that never reads it — would tell whoever asked that
//! the entity does not exist. A forbid that does read the resource still fires
//! by what the resource holds, and a missing one holds nothing, so a caller
//! shown the deciding name can tell the two apart for such a rule; whether the
//! caller is shown it is the enforcement point's to decide.
//!
//! # What it deliberately does not answer
//!
//! Whether the kernel asked at all, and in what order, is the enforcement
//! point's. An agent's rights intersected with its operator's (`§14.6`) are two
//! questions the enforcement point asks and intersects; one question here is one
//! principal. Whether a directory's grants and epochs are current is the
//! directory's, and a stale epoch is what `core.fail_closed` and `§14.8` are for.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

use std::str::FromStr as _;

use cedar_policy::{
    AuthorizationError, Authorizer, Context, Decision as Verdict, Effect, EntityId, EntityTypeName,
    EntityUid, Request, Response,
};
use nc_catalog::ErrorCode;
use nc_policy::{Problem, manifest::Manifest};
pub use nc_schema::selector::EntityRef;

mod directory;
mod embedded;

pub use directory::{
    Directory, DirectoryError, GrantFacts, MemoryDirectory, Principal, PrincipalKind, RealmFacts,
    Slice,
};

/// Declares the sentinel module, and lists every sentinel it declares in its `ALL`.
///
/// The module's body is this macro's grammar — `NAME = "value";` under its doc
/// — so `ALL` is the one place a sentinel can be declared: a `pub const`
/// written beside them does not compile. With a hand-written list, a sentinel
/// declared outside it under a prefix a pack's `@id` may also use passes every
/// test, and that is the confusion the prefix exists to end.
macro_rules! sentinels {
    (
        $(#[$outer:meta])*
        pub mod $module:ident {
            $($(#[$doc:meta])* $name:ident = $value:literal;)+
        }
    ) => {
        $(#[$outer])*
        pub mod $module {
            $($(#[$doc])* pub const $name: &str = $value;)+

            /// Every sentinel, so a test can hold them all to the reserved prefix.
            pub const ALL: &[&str] = &[$($name),+];
        }
    };
}

sentinels! {
    /// What the decision point records as having decided, when no policy did.
    ///
    /// Each begins with `nc_policy::RESERVED_PREFIX`, which no policy's `@id`
    /// may, so an auditor reading the journal can tell a verdict the decision
    /// point wrote from a rule the pack holds by the first word alone.
    pub mod sentinel {
        /// Nothing permitted the request and no forbid fired, or it named an entity the directory does not hold.
        NO_PERMIT_MATCHED = "kernel.no_permit_matched";
        /// A policy erred while it was evaluated; the ids that erred follow a `:`.
        EVALUATION_ERROR = "kernel.evaluation_error";
        /// The pack could not be loaded, so nothing could be decided.
        PDP_UNAVAILABLE = "kernel.pdp_unavailable";
        /// The directory did not answer for the realm, the grant or the entities.
        DIRECTORY_UNAVAILABLE = "kernel.directory_unavailable";
        /// The realm pins a pack this build does not carry.
        PACK_NOT_CARRIED = "kernel.pack_not_carried";
        /// The request is not one the schema allows.
        REQUEST_INVALID = "kernel.request_invalid";
    }
}

/// Why a decision point could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdpError {
    /// The schema, a policy or the pack's manifest is not text of its kind.
    Unreadable(Vec<Problem>),
    /// Everything parsed and kept the rules, and Cedar's validator refuses the pack against the schema.
    Invalid(Vec<Problem>),
    /// Everything parsed, and the pack breaks a rule packs are held to: an `@id`, a template, a floor, an empty pack.
    RuleBroken(Vec<Problem>),
    /// No pack this build carries is selected by the profile named.
    ProfileNotCarried(String),
}

impl PdpError {
    /// The code a refusal built on this error carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Unreadable(_) => ErrorCode::PolicyUnreadable,
            Self::Invalid(_) => ErrorCode::PolicyInvalid,
            Self::RuleBroken(_) => ErrorCode::PolicyRuleBroken,
            Self::ProfileNotCarried(_) => ErrorCode::PolicyPackNotCarried,
        }
    }
}

impl core::fmt::Display for PdpError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unreadable(problems) | Self::Invalid(problems) | Self::RuleBroken(problems) => {
                write!(formatter, "[{}] the pack was refused:", self.code())?;
                for problem in problems {
                    write!(formatter, " {problem};")?;
                }
                Ok(())
            }
            // Debug-quoted: the profile is whatever the caller handed in.
            Self::ProfileNotCarried(profile) => write!(
                formatter,
                "[{}] no pack this build carries is selected by the profile {profile:?}",
                self.code()
            ),
        }
    }
}

impl std::error::Error for PdpError {}

/// The checks `nc_policy::compile` names when text is not of its kind at all.
///
/// Everything else it refuses was read and judged, which is invalid or a
/// broken rule rather than unreadable.
const UNREADABLE: [&str; 2] = ["schema-malformed", "policy-malformed"];

/// The checks `nc_policy::compile` names when Cedar's validator refuses the pack.
///
/// These alone are NC-POL-0002, which the ledger records as a pack that does
/// not validate against the schema. Every other refusal of a parsed pack is a
/// broken rule, one this repository holds packs to, and carries NC-POL-0007.
const VALIDATION: [&str; 2] = ["pack-invalid", "pack-warns"];

/// A compiled pack and the pin it answers to: the one thing that decides.
#[derive(Debug)]
pub struct Pdp {
    /// The schema and the policies, keyed by `@id`.
    compiled: nc_policy::Compiled,
    /// The profile that selects this pack: `family`.
    profile: String,
    /// The pin a realm writes for it: `family@1`.
    pack: String,
}

impl Pdp {
    /// The decision point for `profile`, from the core and the pack embedded in this build.
    ///
    /// # Errors
    ///
    /// Returns [`PdpError::ProfileNotCarried`] for a profile whose pack this
    /// build does not carry, and [`PdpError::Unreadable`],
    /// [`PdpError::Invalid`] or [`PdpError::RuleBroken`] for what
    /// `nc_policy::compile` or the manifest refuses — which the gate that built
    /// this crate would already have refused, so any of them is a build nobody
    /// checked.
    pub fn embedded(profile: &str) -> Result<Self, PdpError> {
        let pack = embedded::carried(profile)
            .ok_or_else(|| PdpError::ProfileNotCarried(profile.to_owned()))?;
        let files: Vec<(&str, &str)> = embedded::CORE
            .iter()
            .chain(pack.files.iter())
            .copied()
            .collect();
        Self::compile(embedded::SCHEMA.1, &files, pack.profile, pack.manifest)
    }

    /// A decision point over policy text a test supplies. **For tests only.**
    ///
    /// A test that needs a pack the tree does not ship — a rule that errs when
    /// evaluated, a file that does not parse — builds it here, through the same
    /// compiler and the same manifest reader as `Pdp::embedded`. Each file is
    /// its path under the repository root beside its text, because the path is
    /// part of the rule `nc_policy::compile` holds it to; `manifest` is the
    /// `pack.toml` text the pin is read from.
    ///
    /// # Errors
    ///
    /// Returns `PdpError::Unreadable`, `PdpError::Invalid` or
    /// `PdpError::RuleBroken` for what `nc_policy::compile` or the manifest
    /// refuses.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn from_sources_for_tests(
        schema: &str,
        files: &[(&str, &str)],
        profile: &str,
        manifest: &str,
    ) -> Result<Self, PdpError> {
        Self::compile(schema, files, profile, ("pack.toml", manifest))
    }

    /// Compiles the files against the schema, and reads the pin from the manifest.
    fn compile(
        schema: &str,
        files: &[(&str, &str)],
        profile: &str,
        manifest: (&str, &str),
    ) -> Result<Self, PdpError> {
        let version = Manifest::parse(manifest.1).map_err(|why| {
            PdpError::Unreadable(vec![Problem {
                file: manifest.0.to_owned(),
                line: 0,
                what: format!("the manifest does not read: {why}"),
                fix: "a pack's manifest is `version = <n>` and nothing else".to_owned(),
                check: "pack-manifest-malformed",
            }])
        })?;
        let owned: Vec<(String, String)> = files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect();
        let compiled = nc_policy::compile(schema, &owned).map_err(|problems| {
            if problems
                .iter()
                .any(|problem| UNREADABLE.contains(&problem.check))
            {
                PdpError::Unreadable(problems)
            } else if problems
                .iter()
                .all(|problem| VALIDATION.contains(&problem.check))
            {
                PdpError::Invalid(problems)
            } else {
                PdpError::RuleBroken(problems)
            }
        })?;
        Ok(Self {
            compiled,
            profile: profile.to_owned(),
            pack: version.pin(profile),
        })
    }

    /// The pin a realm writes for this pack: `family@1`.
    #[must_use]
    pub fn pack(&self) -> &str {
        &self.pack
    }

    /// The profile that selects this pack: `family`.
    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Whether this pack is the one the realm selects and pins.
    ///
    /// Both halves, because the pin alone is not the realm's choice: a realm of
    /// profile `collective` pinning `family@1` names a pack it was never
    /// reviewed against, and a directory is not the world the gate judged.
    #[must_use]
    pub fn carries(&self, realm: &RealmFacts) -> bool {
        realm.profile == self.profile && realm.pinned_pack == self.pack
    }

    /// Every policy id this decision point holds, in order.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.compiled.ids()
    }

    /// Every action the schema declares, sorted.
    #[must_use]
    pub fn actions(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .compiled
            .schema()
            .actions()
            .map(|action| action.id().unescaped().to_owned())
            .collect();
        names.sort();
        names
    }

    /// The entity types the schema lets `action` apply to as a resource, sorted.
    ///
    /// Empty for an action the schema does not declare, which is how the kernel
    /// refuses a plugin command naming one at `activate`.
    #[must_use]
    pub fn resources_for(&self, action: &str) -> Vec<String> {
        let Some(uid) = uid("Action", action) else {
            return Vec::new();
        };
        let mut named: Vec<String> = self
            .compiled
            .schema()
            .resources_for_action(&uid)
            .into_iter()
            .flatten()
            .map(ToString::to_string)
            .collect();
        named.sort();
        named
    }

    /// Decides one question against the entities the slice holds.
    ///
    /// The request is built against the schema, and a request the schema refuses
    /// is denied before any policy runs. A principal the slice does not hold is
    /// denied as if no permit matched it, and a resource it does not hold is
    /// denied whatever Cedar answers, named by the forbids that fired as a held
    /// one would be. Otherwise Cedar decides, and **any evaluation error
    /// denies** whatever Cedar answered; an allow names the permits that
    /// allowed, a denial the forbids that fired or, when none did,
    /// [`sentinel::NO_PERMIT_MATCHED`].
    #[must_use]
    pub fn decide(&self, question: &Question<'_>, slice: &Slice) -> Decision {
        let Some(request) = self.request(question) else {
            return Decision::denied(
                sentinel::REQUEST_INVALID.to_owned(),
                ErrorCode::PolicyEvaluationFailed,
            );
        };
        // The principal is the one asking, so its absence tells it nothing.
        if !slice.holds(question.principal) {
            return Decision::denied(
                sentinel::NO_PERMIT_MATCHED.to_owned(),
                ErrorCode::PolicyDenied,
            );
        }
        let response =
            Authorizer::new().is_authorized(&request, self.compiled.policies(), slice.entities());
        if !slice.holds(question.resource) {
            return self.absent(&response);
        }
        let erring = joined(response.diagnostics().errors().map(|error| match error {
            AuthorizationError::PolicyEvaluationError(error) => error.policy_id().to_string(),
        }));
        if !erring.is_empty() {
            return Decision::denied(
                format!("{}:{erring}", sentinel::EVALUATION_ERROR),
                ErrorCode::PolicyEvaluationFailed,
            );
        }
        let reasons = joined(response.diagnostics().reason().map(ToString::to_string));
        match response.decision() {
            Verdict::Allow => Decision {
                allowed: true,
                deciding: reasons,
                code: None,
            },
            Verdict::Deny if reasons.is_empty() => Decision::denied(
                sentinel::NO_PERMIT_MATCHED.to_owned(),
                ErrorCode::PolicyDenied,
            ),
            Verdict::Deny => Decision::denied(reasons, ErrorCode::PolicyDenied),
        }
    }

    /// The denial of a question whose resource the slice does not hold, named as a held one's would be.
    ///
    /// Cedar was asked all the same, so a `forbid` that reads nothing of the
    /// resource fires here as it fires for one the directory holds, and the
    /// denial names it: asking to delete without a quorum is refused by the
    /// quorum rule whether or not the thing exists, and a caller who sees the
    /// name learns nothing from it. Every rule reading an attribute of the
    /// missing resource errs, and those errors are expected and not reported;
    /// a permit that matched is not an allow, because there is nothing to act
    /// on. **What stays told apart:** a `forbid` that reads the resource fires
    /// by what the resource holds, and a missing one holds nothing — the
    /// encrypted-content floor fires for a missing `Content` as for one with no
    /// label, and not for a held one labelled `realm` — and a rule that errs
    /// whatever the resource names itself only where the resource is held.
    fn absent(&self, response: &Response) -> Decision {
        let policies = self.compiled.policies();
        let forbids = joined(
            response
                .diagnostics()
                .reason()
                .filter(|id| {
                    policies
                        .policy(id)
                        .is_some_and(|policy| policy.effect() == Effect::Forbid)
                })
                .map(ToString::to_string),
        );
        if forbids.is_empty() {
            Decision::denied(
                sentinel::NO_PERMIT_MATCHED.to_owned(),
                ErrorCode::PolicyDenied,
            )
        } else {
            Decision::denied(forbids, ErrorCode::PolicyDenied)
        }
    }

    /// The request a question is, validated against the schema, or nothing when the schema refuses it.
    fn request(&self, question: &Question<'_>) -> Option<Request> {
        let schema = self.compiled.schema();
        let principal = uid(&question.principal.type_name, &question.principal.id)?;
        let action = uid("Action", question.action)?;
        let resource = uid(&question.resource.type_name, &question.resource.id)?;
        let context =
            Context::from_json_value(question.facts.context()?, Some((schema, &action))).ok()?;
        Request::new(principal, action, resource, context, Some(schema)).ok()
    }
}

/// The uid of an entity, built from its type and id apart, or nothing for a type Cedar cannot name.
///
/// Never by concatenation: an id holding a quote cannot close the literal a
/// concatenated `Type::"id"` would have opened, because none is opened.
pub(crate) fn uid(type_name: &str, id: &str) -> Option<EntityUid> {
    let type_name = EntityTypeName::from_str(type_name).ok()?;
    Some(EntityUid::from_type_name_and_id(
        type_name,
        EntityId::new(id),
    ))
}

/// Policy ids, sorted, each once, joined by `,`.
fn joined(ids: impl Iterator<Item = String>) -> String {
    let mut ids: Vec<String> = ids.collect();
    ids.sort();
    ids.dedup();
    ids.join(",")
}

/// One question to the decision point.
#[derive(Debug, Clone, Copy)]
pub struct Question<'a> {
    /// Who asks.
    pub principal: &'a EntityRef,
    /// The Cedar action a command names: `Read`, `Write`, `Delete`…
    pub action: &'a str,
    /// What the command acts on, as its resource selector extracted it.
    pub resource: &'a EntityRef,
    /// What the kernel vouches for about this dispatch.
    pub facts: &'a Facts,
}

/// The context of a decision: what the kernel vouches for, never what a caller says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// Now, in milliseconds since the Unix epoch, from the kernel's clock.
    pub now_ms: u64,
    /// Whether an agent is acting.
    pub via_agent: bool,
    /// Whether a human confirmed this dispatch, as the kernel alone can issue it.
    pub confirmed_by_human: bool,
    /// How many agents stand between the principal and the dispatch.
    pub actor_chain_depth: u32,
    /// The realm's policy epoch, from the directory (`NOCTUA.md §14.8`).
    pub policy_epoch: u64,
    /// Whether a quorum the kernel minted approved this dispatch.
    pub quorum_met: bool,
    /// The grant the decision rests on, from the directory.
    pub grant: Option<EntityRef>,
}

impl Facts {
    /// The context record the schema declares, as Cedar's JSON.
    ///
    /// An epoch past what a Cedar `Long` holds is nothing rather than a
    /// wrapped number, and a moment past what its `datetime` holds is refused
    /// when Cedar reads the context: both are a request the schema refuses.
    fn context(&self) -> Option<serde_json::Value> {
        let epoch = i64::try_from(self.policy_epoch).ok()?;
        let mut context = serde_json::json!({
            "now": { "__extn": { "fn": "datetime", "arg": datetime(self.now_ms) } },
            "via_agent": self.via_agent,
            "confirmed_by_human": self.confirmed_by_human,
            "actor_chain_depth": i64::from(self.actor_chain_depth),
            "policy_epoch": epoch,
            "quorum_met": self.quorum_met,
        });
        if let Some(grant) = &self.grant {
            context["grant"] = serde_json::json!({
                "__entity": { "type": grant.type_name, "id": grant.id }
            });
        }
        Some(context)
    }
}

/// What the decision point answered, and what decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// The `@id`s that decided, sorted and joined by `,`, or a [`sentinel`].
    pub deciding: String,
    /// The code a denial carries; none for an allow.
    pub code: Option<ErrorCode>,
}

impl Decision {
    /// A denial, with what decided it and the code it carries.
    const fn denied(deciding: String, code: ErrorCode) -> Self {
        Self {
            allowed: false,
            deciding,
            code: Some(code),
        }
    }
}

/// Milliseconds in a day.
const DAY_MS: u64 = 86_400_000;

/// A moment in milliseconds since the Unix epoch, as RFC 3339 in UTC to the millisecond.
///
/// `datetime(1_787_702_400_000)` is `2026-08-26T00:00:00.000Z`, which is how
/// Cedar's `datetime` reads a moment with milliseconds. The calendar is Howard
/// Hinnant's days-from-civil, inverted, over unsigned days: no moment here
/// precedes the epoch. A year past 9999 is written with every digit it has,
/// so Cedar refuses it where a wrapped year would be read as another.
#[must_use]
pub fn datetime(ms: u64) -> String {
    let (year, month, day) = civil_from_days(ms / DAY_MS);
    let within = ms % DAY_MS;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1_000 % 60,
        within % 1_000
    )
}

/// The year, month and day `days` after 1970-01-01, in the proleptic Gregorian calendar.
///
/// Eras of 400 years, each 146 097 days, counted from 0000-03-01 so a leap
/// day falls at the end of its year, which is why the month is shifted by
/// three and the year moves on only for January and February.
const fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let of_era = shifted - era * 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::{DAY_MS, datetime, sentinel, uid};

    #[test]
    fn a_type_cedar_cannot_name_has_no_uid_rather_than_a_concatenated_one() {
        for unnamed in ["Content::", "", "1User", "User \"x\""] {
            assert!(uid(unnamed, "photo").is_none(), "{unnamed:?}");
        }
        // The id is kept whole, quote and all.
        let built = uid("User", "a\"b").expect("a type Cedar names");
        assert_eq!(built.id().unescaped(), "a\"b");
        assert_eq!(built.type_name().to_string(), "User");
    }

    #[test]
    fn the_epoch_is_the_first_millisecond_of_1970() {
        assert_eq!(datetime(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(datetime(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(datetime(DAY_MS - 1), "1970-01-01T23:59:59.999Z");
        assert_eq!(datetime(DAY_MS), "1970-01-02T00:00:00.000Z");
    }

    #[test]
    fn the_moment_the_cases_call_now_is_written_as_cedar_reads_it() {
        assert_eq!(datetime(1_787_702_400_000), "2026-08-26T00:00:00.000Z");
        assert_eq!(datetime(1_787_702_399_999), "2026-08-25T23:59:59.999Z");
    }

    #[test]
    fn a_leap_day_is_a_day_and_the_next_is_march() {
        assert_eq!(datetime(1_709_251_199_999), "2024-02-29T23:59:59.999Z");
        assert_eq!(datetime(1_709_251_200_000), "2024-03-01T00:00:00.000Z");
        // A century is a leap year only every fourth time: 2000 was, 2100 is not.
        assert_eq!(datetime(951_825_600_000), "2000-02-29T12:00:00.000Z");
        assert_eq!(
            datetime(4_107_542_400_000 - DAY_MS),
            "2100-02-28T00:00:00.000Z"
        );
        assert_eq!(datetime(4_107_542_400_000), "2100-03-01T00:00:00.000Z");
    }

    #[test]
    fn the_turn_of_a_year_moves_the_year_and_nothing_else() {
        assert_eq!(datetime(946_684_799_999), "1999-12-31T23:59:59.999Z");
        assert_eq!(datetime(946_684_800_000), "2000-01-01T00:00:00.000Z");
        assert_eq!(datetime(253_402_300_799_999), "9999-12-31T23:59:59.999Z");
    }

    #[test]
    fn the_largest_moment_is_written_with_every_digit_its_year_has() {
        // Past what Cedar's `datetime` holds, and written rather than wrapped,
        // so Cedar refuses it instead of reading another year.
        let written = datetime(u64::MAX);
        assert!(written.starts_with("584556019-04-"), "{written}");
        assert!(written.ends_with('Z'), "{written}");
    }

    #[test]
    fn every_sentinel_begins_with_the_prefix_no_policy_may_use() {
        for name in sentinel::ALL {
            assert!(
                name.starts_with(nc_policy::RESERVED_PREFIX),
                "`{name}` does not begin with `{}`",
                nc_policy::RESERVED_PREFIX
            );
        }
        let mut distinct = sentinel::ALL.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            sentinel::ALL.len(),
            "two sentinels are one name"
        );
    }
}
