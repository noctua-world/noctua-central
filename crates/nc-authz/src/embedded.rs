//! The schema, the core and the packs this crate carries, embedded when it is built.
//!
//! `NOCTUA.md §14.2` makes the decision point a library in the kernel, and
//! `§14.4` has a realm's profile select a versioned pack. So a kernel carries
//! the core and every pack it can decide for, in its own binary: what decides is
//! what was built, and nothing on the machine it runs on can swap a rule under
//! it. `cargo xtask check` holds the files on disk to their cases, and a test
//! beside this list holds the list to the files on disk in both directions —
//! one policy the list forgot would be a rule the gate passed and nothing
//! enforces, and one it kept after its file went would be a rule enforced that
//! no case holds. It holds each list to its own directory, not their union:
//! [`CORE`] is compiled into every pack carried, so a pack's policy listed
//! there would decide for packs whose cases never ran it.
//!
//! **What this does not answer** is a machine whose binary is replaced: a
//! same-user attacker is outside what the kernel defends (`§11.1`), and the
//! pack travels inside the binary they would replace.

/// A file under the repository root, as its path and the text it held when built.
///
/// The path is written once: it is both what `nc_policy::compile` judges a
/// policy's place by and what the text is read from, so the two cannot name
/// different files.
macro_rules! embed {
    ($path:literal) => {
        ($path, include_str!(concat!("../../../", $path)))
    };
}

/// The schema every policy is validated against and every request is built on.
pub(crate) const SCHEMA: (&str, &str) = embed!("policy/schema.cedarschema");

/// The invariant core, which every pack shares (`NOCTUA.md §14.9`).
pub(crate) const CORE: [(&str, &str); 7] = [
    embed!("policy/core/agent_authority.cedar"),
    embed!("policy/core/chain_depth.cedar"),
    embed!("policy/core/fail_closed.cedar"),
    embed!("policy/core/grants_expire.cedar"),
    embed!("policy/core/no_self_escalation.cedar"),
    embed!("policy/core/provenance.cedar"),
    embed!("policy/core/quorum.cedar"),
];

/// The `family` pack's policies.
const FAMILY: [(&str, &str); 3] = [
    embed!("policy/packs/family/accounts.cedar"),
    embed!("policy/packs/family/permissions.cedar"),
    embed!("policy/packs/family/realm.cedar"),
];

/// A pack this crate carries: the profile that selects it, its policies and its manifest.
#[derive(Debug)]
pub(crate) struct Carried {
    /// The realm profile that selects it.
    pub(crate) profile: &'static str,
    /// Each policy file, by its path under the repository root.
    pub(crate) files: &'static [(&'static str, &'static str)],
    /// Its `pack.toml`, which holds the version a realm pins.
    pub(crate) manifest: (&'static str, &'static str),
}

/// Every pack this crate carries.
///
/// A profile whose pack holds no policy — `org` and `collective` today — is
/// absent, and a realm of that profile is denied rather than decided by the
/// core alone.
pub(crate) const CARRIED: [Carried; 1] = [Carried {
    profile: "family",
    files: &FAMILY,
    manifest: embed!("policy/packs/family/pack.toml"),
}];

/// The pack a profile selects, when this crate carries one.
pub(crate) fn carried(profile: &str) -> Option<&'static Carried> {
    CARRIED.iter().find(|pack| pack.profile == profile)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use super::{CARRIED, CORE, SCHEMA, carried};

    /// Files by the directory whose list holds them: directory, then path, then text.
    ///
    /// Keyed by directory rather than merged, because which list a file is in
    /// is part of what decides: `CORE` is compiled into every pack this crate
    /// carries, and a pack's list into its own alone.
    type Lists = BTreeMap<String, BTreeMap<String, String>>;

    /// The repository root, which every embedded path is relative to.
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Every file under `directory` whose name ends in `kind`, by path relative to the root.
    fn files_under(directory: &Path, kind: &str, found: &mut BTreeMap<String, String>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
            .expect("a directory the policy tree holds")
            .map(|entry| entry.expect("a readable entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                files_under(&path, kind, found);
            } else if path.to_string_lossy().ends_with(kind) {
                let relative = path
                    .strip_prefix(root())
                    .expect("a path under the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).expect("a readable policy file");
                found.insert(relative, text);
            }
        }
    }

    /// What the policy tree holds, list by list: the schema, the core and each pack with a policy.
    fn on_disk() -> Lists {
        let policy = root().join("policy");
        let mut lists = Lists::new();
        let schema =
            std::fs::read_to_string(policy.join("schema.cedarschema")).expect("the schema");
        lists.insert(
            "policy/".to_owned(),
            BTreeMap::from([("policy/schema.cedarschema".to_owned(), schema)]),
        );
        let mut core = BTreeMap::new();
        files_under(&policy.join("core"), ".cedar", &mut core);
        lists.insert("policy/core/".to_owned(), core);
        let mut packs: Vec<PathBuf> = std::fs::read_dir(policy.join("packs"))
            .expect("the packs directory")
            .map(|entry| entry.expect("a readable entry").path())
            .collect();
        packs.sort();
        for pack in packs {
            let mut policies = BTreeMap::new();
            files_under(&pack, ".cedar", &mut policies);
            if !policies.is_empty() {
                let directory = format!(
                    "policy/packs/{}/",
                    pack.file_name().expect("a named pack").to_string_lossy()
                );
                // A pack with policies and no manifest is the gate's to refuse;
                // listed here as unreadable, it fails this test as well.
                let text = std::fs::read_to_string(pack.join("pack.toml")).unwrap_or_default();
                policies.insert(format!("{directory}pack.toml"), text);
                lists.insert(directory, policies);
            }
        }
        lists
    }

    /// What this crate embeds, by the same keys: `SCHEMA`, `CORE`, and each `CARRIED` pack by its profile.
    fn embedded() -> Lists {
        let listed = |files: Vec<&(&str, &str)>| -> BTreeMap<String, String> {
            let mut list = BTreeMap::new();
            for (path, text) in files {
                let earlier = list.insert((*path).to_owned(), (*text).to_owned());
                assert!(earlier.is_none(), "`{path}` is embedded twice in one list");
            }
            list
        };
        let mut lists = Lists::new();
        lists.insert("policy/".to_owned(), listed(vec![&SCHEMA]));
        lists.insert("policy/core/".to_owned(), listed(CORE.iter().collect()));
        for pack in &CARRIED {
            let files = pack.files.iter().chain(std::iter::once(&pack.manifest));
            let earlier = lists.insert(
                format!("policy/packs/{}/", pack.profile),
                listed(files.collect()),
            );
            assert!(earlier.is_none(), "`{}` is carried twice", pack.profile);
        }
        // And across lists: one file in two lists decides in both.
        let every: Vec<&String> = lists.values().flat_map(BTreeMap::keys).collect();
        let distinct: BTreeSet<&String> = every.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            every.len(),
            "a file is embedded in two lists: {every:?}"
        );
        lists
    }

    #[test]
    fn the_embedded_files_are_the_files_on_disk_in_both_directions() {
        // **List by list, not their union.** Found by review: this compared one
        // map merged from every list, so `realm.cedar` moved from the family's
        // list into `CORE` left every test green — the family decides over the
        // same set either way — while `CORE` is compiled into every pack this
        // crate carries, and the next pack carried would have been decided by
        // the family's guardian permits.
        let disk = on_disk();
        let held = embedded();
        let empty = BTreeMap::new();
        let directories: BTreeSet<&String> = disk.keys().chain(held.keys()).collect();
        for directory in directories {
            let there = disk.get(directory).unwrap_or(&empty);
            let listed = held.get(directory).unwrap_or(&empty);
            let forgotten: Vec<&String> = there
                .keys()
                .filter(|path| !listed.contains_key(*path))
                .collect();
            let elsewhere: Vec<&String> = listed
                .keys()
                .filter(|path| !there.contains_key(*path))
                .collect();
            assert!(
                forgotten.is_empty(),
                "on disk under `{directory}` and not in its embedded list, so the gate \
                 passes them and nothing enforces them where they belong: {forgotten:?}"
            );
            assert!(
                elsewhere.is_empty(),
                "in the embedded list for `{directory}` and not on disk there, so they \
                 decide where no case holds them: {elsewhere:?}"
            );
        }
        // Two walks that both stopped early would agree on nothing at all.
        let files: usize = disk.values().map(BTreeMap::len).sum();
        assert!(
            files > CORE.len() + 2,
            "{files} file(s) read from the policy tree, which is not this tree"
        );
        assert!(
            disk.len() > 2,
            "{} list(s) read from the policy tree, and it holds the schema, the core \
             and a pack",
            disk.len()
        );
    }

    #[test]
    fn every_embedded_text_is_the_text_on_disk() {
        let disk = on_disk();
        for (directory, listed) in embedded() {
            for (path, text) in listed {
                assert_eq!(
                    disk.get(&directory).and_then(|there| there.get(&path)),
                    Some(&text),
                    "`{path}`, in the list for `{directory}`, differs from the file there"
                );
            }
        }
    }

    #[test]
    fn a_profile_is_carried_only_when_its_pack_is_embedded() {
        assert_eq!(carried("family").map(|pack| pack.profile), Some("family"));
        for absent in ["org", "collective", "", "Family", "family ", "familyx"] {
            assert!(carried(absent).is_none(), "`{absent}` is not carried");
        }
    }
}
