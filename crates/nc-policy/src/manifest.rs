//! A pack's manifest, and the pin a realm writes against it.
//!
//! The manifest is `policy/packs/<profile>/pack.toml`. `NOCTUA.md §14.4`: a
//! realm's `profile` selects a versioned pack, a realm may pin the version, and
//! updating a pack is a reviewed migration rather than a silent deploy. The
//! version is written here, as one key, and the pin is `<profile>@<version>`.
//! [`Manifest::pin`] is the one place that spelling is built, so the gate
//! holding `policy/world.json` to it and a decision point comparing a realm's
//! pin with the pack it carries cannot spell it two ways.
//!
//! # What the gate holds
//!
//! * a pack directory that holds a policy holds a manifest, and a manifest sits
//!   only beside a policy — so "a pack" means one thing, whichever file is asked;
//! * a manifest is `version = <u32>` and nothing else, because a key nothing
//!   reads is a setting somebody believes is in force;
//! * every realm in the world pins a version of the pack its own profile
//!   selects, and one whose profile selects a pack this tree carries pins
//!   exactly that pack's version;
//! * every `.toml` under `policy/` is a case file or a manifest, so a file of
//!   either shape in the wrong place is refused rather than read by nothing;
//! * every `.cedar` under `policy/` sits in the core or in a pack's own
//!   directory, because a decision point compiles the core and its realm's
//!   pack and nothing else — a rule in neither passes every case here and is
//!   enforced nowhere.
//!
//! # What it answers, and what it does not (`NOCTUA.md §2.4`)
//!
//! It answers a realm running a pack it was not reviewed against: a pin that
//! names another profile's pack, or another version of its own, is refused in
//! the world the cases decide against, and a decision point comparing the
//! realm's profile and pin with the pack it carries denies the realm.
//!
//! It does **not** answer whether a version was bumped when a policy changed.
//! There is no history here to compare against (`NOCTUA.md §5.3`), so a pack
//! edited in place keeps its number, and that is a reviewer's question. Nor does
//! it judge the version a realm pins when its profile selects no pack this tree
//! carries: `office` pins `org@0.1.0` and `org` holds no policy, so there is
//! nothing on disk to hold that version to — and a decision point denies such a
//! realm, because it carries no pack for that profile.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cedar_policy::{Entities, Entity, EvalResult};
use serde::Deserialize;

use crate::Problem;

/// The name a pack's manifest has, in the pack's own directory.
pub const FILE: &str = "pack.toml";

/// The directory, inside `policy/`, that holds one directory per profile.
const PACKS: &str = "packs";

/// The directory, inside `policy/`, that holds what every pack shares.
const CORE: &str = "core";

/// The entity type that selects a pack by `profile` and pins it by `policy_pack`.
const REALM: &str = "Realm";

/// A pack's manifest, as written: its version, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The version a realm pins.
    version: u32,
}

impl Manifest {
    /// The manifest that declares `version`, for a reader holding the number.
    ///
    /// [`crate::Pack::versions`] hands out numbers, and a pin is built from one
    /// here rather than spelled a second time by whoever holds it.
    #[must_use]
    pub const fn new(version: u32) -> Self {
        Self { version }
    }

    /// Reads a manifest's text.
    ///
    /// # Errors
    ///
    /// Returns TOML's own account of what is wrong: text that is not TOML, a
    /// `version` that is missing, is not an integer or does not fit in a `u32`,
    /// or any key beside it.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| error.message().to_owned())
    }

    /// The version a realm pins.
    #[must_use]
    pub const fn version(self) -> u32 {
        self.version
    }

    /// The pin a realm of `profile` writes for this version: `family@1`.
    #[must_use]
    pub fn pin(self, profile: &str) -> String {
        format!("{profile}@{}", self.version)
    }
}

/// What the packs' manifests declare, and what the world pins against them.
#[derive(Debug, Default)]
pub(crate) struct Carried {
    /// Each pack's version, by profile.
    pub(crate) versions: BTreeMap<String, u32>,
    /// Each realm held to a carried pack, by id, and the pin it writes.
    pub(crate) pins: BTreeMap<String, String>,
}

/// Judges every manifest, every realm's pin, and where every `.toml` and `.cedar` sits.
pub(crate) fn check(root: &Path, world: &Entities, problems: &mut Vec<Problem>) -> Carried {
    let policy = root.join(crate::POLICY);
    every_toml_is_read(root, &policy, problems);
    every_policy_is_in_a_pack(root, &policy, problems);
    let versions = versions(root, &policy, problems);
    let pins = pins(world, &versions, problems);
    Carried { versions, pins }
}

/// Each pack's version, by profile, with a problem for every pack without one.
///
/// A pack is without one when its manifest is missing, stands beside no policy,
/// cannot be read, or is not one version.
fn versions(root: &Path, policy: &Path, problems: &mut Vec<Problem>) -> BTreeMap<String, u32> {
    let mut versions = BTreeMap::new();
    // A tree with no packs directory carries no pack, and there is nothing
    // under it to judge: every realm's pin is then one this tree cannot hold to
    // anything, which `Pack::pins` reports as empty rather than as agreement.
    let mut packs: Vec<PathBuf> = std::fs::read_dir(policy.join(PACKS))
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    packs.retain(|path| path.is_dir());
    packs.sort();
    for pack in packs {
        let profile = pack
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let manifest = pack.join(FILE);
        let relative = crate::relative_to(root, &manifest);
        let mut holds_a_policy = false;
        crate::walk(&pack, &mut |file| {
            if file.extension().is_some_and(|kind| kind == "cedar") {
                holds_a_policy = true;
            }
        });
        if !manifest.exists() {
            if holds_a_policy {
                problems.push(Problem {
                    file: relative,
                    line: 0,
                    what: format!(
                        "the `{profile}` pack holds policies and no `{FILE}`, so it has no \
                         version a realm could pin"
                    ),
                    fix: "a realm pins its pack as `<profile>@<version>` (NOCTUA.md §14.4), and \
                          the version is the one key of the pack's own manifest. Write \
                          `version = 1` there for a new pack, and pin every realm of that \
                          profile in policy/world.json to it"
                        .to_owned(),
                    check: "pack-manifest-missing",
                });
            }
            continue;
        }
        if !holds_a_policy {
            problems.push(Problem {
                file: relative,
                line: 0,
                what: format!(
                    "a version of the `{profile}` pack is declared, and the pack holds no \
                     policy"
                ),
                fix: "a pack is a directory that holds policies, and its manifest is their \
                      version: a version of nothing is a pin a realm could satisfy while no \
                      decision point carries such a pack. Take the manifest out, and write it \
                      in the change that adds the pack's first policy"
                    .to_owned(),
                check: "pack-manifest-without-policies",
            });
            continue;
        }
        let text = match std::fs::read_to_string(&manifest) {
            Ok(text) => text,
            Err(error) => {
                problems.push(Problem {
                    file: relative,
                    line: 0,
                    what: format!("the `{profile}` pack's manifest could not be read: {error}"),
                    fix: "without its version the pack cannot be pinned, and every realm of \
                          its profile is then held to nothing. It is a file of one line, \
                          readable by whoever runs the gate"
                        .to_owned(),
                    check: "pack-manifest-unreadable",
                });
                continue;
            }
        };
        match Manifest::parse(&text) {
            Ok(read) => {
                versions.insert(profile, read.version());
            }
            Err(why) => problems.push(Problem {
                file: relative,
                line: 0,
                what: format!("the `{profile}` pack's manifest is not one version: {why}"),
                fix: "a manifest is `version = <n>`, an unsigned 32-bit integer, and nothing \
                      else — a key nothing reads is a setting somebody believes is in force, \
                      and a version written as text is a pin that compares as text"
                    .to_owned(),
                check: "pack-manifest-malformed",
            }),
        }
    }
    versions
}

/// Every realm held to a carried pack, by id, with the pin it writes.
///
/// A realm pinning another profile's pack, or another version of its own, is
/// a problem, and is left out of what this returns.
fn pins(
    world: &Entities,
    versions: &BTreeMap<String, u32>,
    problems: &mut Vec<Problem>,
) -> BTreeMap<String, String> {
    // Sorted, because `Entities` does not iterate in any order this can promise
    // and a refusal list that reorders itself between runs reads as a change.
    let mut realms: Vec<(String, String, String)> = world
        .iter()
        .filter(|entity| entity.uid().type_name().to_string() == REALM)
        .map(|entity| {
            (
                entity.uid().id().unescaped().to_owned(),
                text_of(entity, "profile"),
                text_of(entity, "policy_pack"),
            )
        })
        .collect();
    realms.sort();
    let mut pins = BTreeMap::new();
    for (realm, profile, pinned) in realms {
        // Before the version, because it holds whether or not this tree
        // carries the pack: a realm whose profile selects no carried pack could
        // otherwise pin one that is carried, and a decision point comparing
        // only the pin with the pack it carries would let that realm through.
        if !pins_its_own_profile(&pinned, &profile) {
            problems.push(Problem {
                file: format!("{}/world.json", crate::POLICY),
                line: 0,
                what: format!(
                    "`Realm::\"{realm}\"` selects the `{profile}` profile and pins `{pinned}`, \
                     which is not a version of the `{profile}` pack"
                ),
                fix: "a realm's profile selects its pack, and its pin says which version of \
                      that pack it was reviewed against (NOCTUA.md §14.4) — a pin naming \
                      another profile's pack is a realm decided by rules nobody chose for it. \
                      Pin `<profile>@<version>` with the realm's own profile, or change the \
                      profile in the change that migrates the realm"
                    .to_owned(),
                check: "realm-pins-another-profile",
            });
            continue;
        }
        let Some(version) = versions.get(&profile) else {
            continue;
        };
        let carried = Manifest::new(*version).pin(&profile);
        if pinned != carried {
            problems.push(Problem {
                file: format!("{}/world.json", crate::POLICY),
                line: 0,
                what: format!(
                    "`Realm::\"{realm}\"` pins `{pinned}`, and the `{profile}` pack this \
                     tree carries is `{carried}`"
                ),
                fix: "a realm's pin is the version it was reviewed against, and updating a \
                      pack is a migration rather than a silent deploy (NOCTUA.md §14.4). Pin \
                      the realm to the version the pack's manifest declares, or change that \
                      version in the change that migrates every realm of the profile"
                    .to_owned(),
                check: "realm-pins-another-version",
            });
            continue;
        }
        pins.insert(realm, pinned);
    }
    pins
}

/// Whether `pinned` is a version of `profile`'s pack: `<profile>@…`.
///
/// Read against the spelling [`Manifest::pin`] builds, profile first, so a pin
/// is its own profile's only when that profile is the whole of what comes
/// before the `@` — `organic@1` is no pin of `org`.
fn pins_its_own_profile(pinned: &str, profile: &str) -> bool {
    pinned
        .strip_prefix(profile)
        .is_some_and(|version| version.starts_with('@'))
}

/// A string attribute of an entity, or nothing.
///
/// The schema declares both of a realm's as required strings and the world is
/// loaded against it, so the empty answer is for an attribute it would not
/// have loaded — which names no profile, and no pin is a version of that.
fn text_of(entity: &Entity, attribute: &str) -> String {
    match entity.attr(attribute) {
        Some(Ok(EvalResult::String(text))) => text,
        _ => String::new(),
    }
}

/// Every `.toml` under `policy/` is a case file or a pack's manifest.
///
/// The kind rule in `xtask/src/check.rs` lets `policy/` hold `.toml` files at
/// all; this is what says where. A case file placed outside every `tests/`
/// directory is run by nothing, and a manifest placed anywhere but directly in
/// a pack's own directory versions nothing — and each reads, to whoever opens
/// it, exactly like one that is in force.
fn every_toml_is_read(root: &Path, policy: &Path, problems: &mut Vec<Problem>) {
    crate::walk(policy, &mut |path| {
        if path.extension().is_none_or(|kind| kind != "toml") {
            return;
        }
        if crate::cases::is_a_case_file(policy, path) || is_a_manifest(policy, path) {
            return;
        }
        problems.push(Problem {
            file: crate::relative_to(root, path),
            line: 0,
            what: "a `.toml` that nothing reads: it is under no `tests/` directory, and it \
                   is not a manifest directly inside a pack's own directory"
                .to_owned(),
            fix: "every `.toml` under policy/ is a case file, under a `tests/` directory, or \
                  a pack's manifest, at policy/packs/<profile>/pack.toml. Anywhere else it is \
                  a case or a setting that reads as in force and is not — move it, or take \
                  it out"
                .to_owned(),
            check: "toml-read-by-nothing",
        });
    });
}

/// Whether `path`, inside `policy`, is `packs/<profile>/pack.toml`.
fn is_a_manifest(policy: &Path, path: &Path) -> bool {
    let inside = crate::relative_to(policy, path);
    let parts: Vec<&str> = inside.split('/').collect();
    matches!(parts.as_slice(), [PACKS, _, FILE])
}

/// Every `.cedar` under `policy/` sits in the core or in a pack's own directory.
///
/// [`crate::load`] compiles every policy file under `policy/`, because the
/// gate holds every pack at once, while a decision point compiles the core and
/// the one pack its realm's profile selects. A file in neither — beside the
/// packs, or beside the core — is compiled, cased and counted by the gate and
/// loaded by no decision point, which is a rule that passes and protects
/// nothing.
fn every_policy_is_in_a_pack(root: &Path, policy: &Path, problems: &mut Vec<Problem>) {
    crate::walk(policy, &mut |path| {
        if path.extension().is_none_or(|kind| kind != "cedar") {
            return;
        }
        if is_in_a_pack(policy, path) {
            return;
        }
        problems.push(Problem {
            file: crate::relative_to(root, path),
            line: 0,
            what: "a policy in no pack: the gate compiles and tests it, and a decision point \
                   compiles only the core and its realm's pack"
                .to_owned(),
            fix: "every `.cedar` under policy/ is in policy/core/, which every pack shares, \
                  or in policy/packs/<profile>/, which the realms of that profile select \
                  (NOCTUA.md §14.4). Anywhere else it passes its cases and is enforced \
                  nowhere — move it into one of the two"
                .to_owned(),
            check: "policy-in-no-pack",
        });
    });
}

/// Whether `path`, inside `policy`, is under `core/` or `packs/<profile>/`.
fn is_in_a_pack(policy: &Path, path: &Path) -> bool {
    let inside = crate::relative_to(policy, path);
    let parts: Vec<&str> = inside.split('/').collect();
    matches!(parts.as_slice(), [CORE, _, ..] | [PACKS, _, _, ..])
}

#[cfg(test)]
mod tests {
    use super::Manifest;

    /// A manifest is one unsigned version, and nothing beside it.
    ///
    /// Every refusal here is a shape somebody would plausibly write: the version
    /// quoted, as `Cargo.toml` writes one; negative; a fraction; past `u32`;
    /// missing; a second key; and text that is not TOML at all.
    #[test]
    fn a_manifest_is_one_unsigned_version_and_nothing_else() {
        let read = Manifest::parse("# the family pack\nversion = 1\n").expect("one version");
        assert_eq!(read.version(), 1);
        for (text, why) in [
            ("version = \"1\"\n", "a version written as text"),
            ("version = -1\n", "a negative version"),
            ("version = 1.5\n", "a fractional version"),
            ("version = 4294967296\n", "a version past u32"),
            ("\n", "no version at all"),
            (
                "version = 1\nprofile = \"family\"\n",
                "a key beside the version",
            ),
            ("version = = 1\n", "text that is not TOML"),
        ] {
            let refused = Manifest::parse(text);
            assert!(
                refused.is_err(),
                "{why} was read as a manifest: {refused:?}"
            );
            assert!(
                !refused.unwrap_err().trim().is_empty(),
                "{why} was refused with nothing to say about it"
            );
        }
    }

    /// A pin is the profile, `@`, and the version: what `policy/world.json` writes.
    #[test]
    fn a_pin_is_the_profile_at_the_version() {
        let read = Manifest::parse("version = 7\n").expect("one version");
        assert_eq!(read.pin("family"), "family@7");
    }
}
