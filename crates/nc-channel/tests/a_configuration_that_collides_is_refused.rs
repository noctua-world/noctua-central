//! A person's keybinding configuration overrides the catalog's defaults in its
//! channel, and one that collides is refused, naming the rule, while the
//! defaults stand (`NOCTUA.md §12.8`, `§17.1`).
//!
//! Every install here is made under a scratch root of its own and relocated
//! under a namespace, so nothing a test reads or writes is a real profile's:
//! each manifest is written where its channel's binaries read it, each
//! configuration where `nc_channel::keys` reads it, and the scratch is removed
//! however the test leaves. Nothing here starts a process.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;
use nc_channel::checkouts::{CHANNELS_MAX, PROFILES_MAX};
use nc_channel::found::{self, Found};
use nc_channel::keys::{self, Change, Configuration, Effective, KeysRefused, Part, Unwritten};
use nc_channel::{Bases, Channel, Checkout, Install, Namespace, layout};
use nc_schema::keys::{Binding, Chord, Key, Leader, LeaderPart, LeaderRule, Modifier, Warning};

/// The namespace every install of this suite is relocated under.
const HERE: &str = "k7";

/// A scratch root, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        // A base whose entries a test hid is listed again before it goes.
        for base in ["c", "s"] {
            let _ = nc_platform::testing::make_private(&self.0.join(base));
        }
        let _ = nc_platform::testing::make_private(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// A private directory of its own, named for this process and `tag`.
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-channel-keys-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch directory");
        Self(root)
    }

    /// The four bases every install of this scratch shares.
    fn bases(&self) -> Bases {
        Bases::new(
            self.0.join("d"),
            self.0.join("c"),
            self.0.join("k"),
            self.0.join("s"),
        )
        .expect("plain absolute bases")
    }

    /// `channel`'s install under these bases, relocated under `namespace`.
    fn install(&self, channel: Channel, namespace: &str) -> Install {
        Install {
            channel,
            bases: self.bases(),
            namespace: Some(Namespace::parse(namespace).expect("a namespace")),
            log_endpoint: None,
        }
    }

    /// `channel` installed under `namespace`: its manifest where its binaries
    /// read it, beside a stand-in executable, read as a binary reads it.
    fn installed(&self, channel: Channel, namespace: &str) -> Found {
        let install = self.install(channel, namespace);
        let names = layout(&install, "scratch");
        self.made(&names.install);
        write_private(&names.manifest, install.to_canonical().as_bytes());
        let binary = names.install.join("noctua");
        if !binary.exists() {
            nc_platform::create_private(&binary).expect("a stand-in binary");
        }
        found::beside(&binary).expect("the install is read")
    }

    /// `channel`'s manifest taken away, as an uninstall leaves its profile.
    fn uninstalled(&self, channel: Channel, namespace: &str) {
        let names = layout(&self.install(channel, namespace), "scratch");
        std::fs::remove_file(names.manifest).expect("the manifest is removed");
    }

    /// Every directory from the scratch root down to `directory`, each made
    /// private where it is missing.
    fn made(&self, directory: &Path) {
        let below = directory
            .strip_prefix(&self.0)
            .expect("a path under the scratch root");
        let mut at = self.0.clone();
        for part in below.components() {
            at.push(part);
            if !at.exists() {
                nc_platform::create_private_directory(&at).expect("a private directory");
            }
        }
    }

    /// `text` as `channel`'s configuration under `namespace`, private as a
    /// writer leaves one, and where it is.
    fn configure(&self, channel: Channel, namespace: &str, text: &str) -> PathBuf {
        let names = layout(&self.install(channel, namespace), "scratch");
        self.made(&names.config);
        let _ = std::fs::remove_file(&names.keybindings);
        write_private(&names.keybindings, text.as_bytes());
        names.keybindings
    }

    /// Every file and directory under the scratch root: its path, what it
    /// holds and whether it is private.
    fn every_entry(&self) -> BTreeMap<PathBuf, (Option<Vec<u8>>, bool)> {
        let mut entries = BTreeMap::new();
        let mut left = vec![self.0.clone()];
        while let Some(directory) = left.pop() {
            for entry in std::fs::read_dir(&directory).expect("a scratch directory is listed") {
                let path = entry.expect("an entry").path();
                let private = nc_platform::testing::is_private(&path);
                if path.is_dir() {
                    left.push(path.clone());
                    entries.insert(path, (None, private));
                } else {
                    let bytes = std::fs::read(&path).expect("a scratch file is read");
                    entries.insert(path, (Some(bytes), private));
                }
            }
        }
        entries
    }
}

/// `bytes` written to a new private file at `path`.
fn write_private(path: &Path, bytes: &[u8]) {
    let _ = std::fs::remove_file(path);
    let mut file = nc_platform::create_private(path).expect("a private file");
    file.write_all(bytes).expect("written");
}

/// A development checkout, by its digits.
fn development(digits: &str) -> Channel {
    Channel::Development(Checkout::parse(digits).expect("eight digits"))
}

/// A chord as the grammar writes it.
fn chord(text: &str) -> Chord {
    Chord::parse(text).expect("a chord")
}

/// A binding as the grammar writes it.
fn binding(text: &str) -> Binding {
    Binding::parse(text).expect("a binding")
}

/// Each of `texts`, as the grammar reads a binding.
fn bound(texts: &[&str]) -> Vec<Binding> {
    texts.iter().map(|text| binding(text)).collect()
}

/// A configuration of the Leader `leader` alone.
fn leader_of(leader: &str) -> String {
    format!(r#"{{"format":1,"leader":"{leader}"}}"#)
}

/// The tables the catalog this build carries makes on its own.
fn the_catalogs_tables() -> nc_schema::keys::Tables {
    let catalog = keys::catalog_bindings();
    let read: Vec<(&str, Vec<Binding>)> = catalog
        .iter()
        .map(|(command, bound)| (command.as_str(), bound.clone()))
        .collect();
    nc_schema::keys::tables(&read, &[]).expect("the catalog's tables")
}

/// The keys in force are the channel's defaults: its Leader and its chord
/// modifier, and the catalog's bindings.
fn assert_the_defaults(effective: &Effective) {
    assert_eq!(
        effective.leader,
        effective.channel.leader(),
        "{effective:?}"
    );
    assert_eq!(effective.chord, effective.channel.chord_modifier());
    assert!(!effective.leader_configured, "{effective:?}");
    assert!(!effective.bindings_configured, "{effective:?}");
    assert_eq!(effective.tables, the_catalogs_tables());
}

/// The keys in force are the configured Leader `leader`, with Shift.
fn assert_the_leader(effective: &Effective, leader: &str) {
    assert!(
        effective.refused.is_empty(),
        "{:?}",
        effective
            .refused
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert!(effective.leader_configured, "{effective:?}");
    assert_eq!(effective.leader.chord(), chord(leader));
    assert_eq!(effective.chord, Modifier::Shift);
}

/// The one refusal `effective` met, which carries `code` and says it first.
fn the_refusal(effective: &Effective, code: ErrorCode) -> &KeysRefused {
    assert_eq!(
        effective.refused.len(),
        1,
        "{:?}",
        effective
            .refused
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let refused = &effective.refused[0];
    assert_eq!(refused.code(), code, "{refused}");
    assert!(
        refused
            .to_string()
            .starts_with(&format!("[{}] ", code.code())),
        "{refused}"
    );
    refused
}

/// The configured Leader part is refused for `rule`, against `by`, and the
/// channel's default Leader and chord modifier stand.
fn assert_the_leader_refused(effective: &Effective, rule: LeaderRule, by: Option<Channel>) {
    let code = match rule {
        LeaderRule::HoldsTheChordModifier => ErrorCode::ChannelLeaderHoldsTheChordModifier,
        LeaderRule::WithoutAModifier => ErrorCode::ChannelLeaderWithoutAModifier,
        LeaderRule::HeldWithTheChordIsAnothers => {
            ErrorCode::ChannelLeaderHeldWithTheChordIsAnothers
        }
        LeaderRule::SharedWithAnother => ErrorCode::ChannelLeaderOfAnotherChannel,
    };
    let refused = the_refusal(effective, code);
    assert!(
        matches!(refused, KeysRefused::Leader { rule: broken, by: met, .. } if *broken == rule && *met == by),
        "{refused:?}"
    );
    assert_eq!(refused.part(), Part::Leader);
    assert!(
        refused.to_string().contains(&rule.to_string()),
        "the refusal does not name the rule: {refused}"
    );
    assert!(
        refused
            .to_string()
            .ends_with("and the channel's default Leader and chord modifier stand"),
        "{refused}"
    );
    assert_eq!(effective.leader, effective.channel.leader());
    assert_eq!(effective.chord, effective.channel.chord_modifier());
    assert!(!effective.leader_configured);
}

/// No file is the channel's defaults, and so is a file naming its format
/// alone: nothing refused, and the warnings the defaults raise.
#[test]
fn no_file_is_the_defaults() {
    let scratch = Scratch::new("none");
    let staging = scratch.installed(Channel::Staging, HERE);
    let effective = keys::effective(&staging);
    assert_the_defaults(&effective);
    assert!(effective.refused.is_empty(), "{effective:?}");
    assert_eq!(effective.file, staging.layout.keybindings);
    let part = LeaderPart {
        leader: Channel::Staging.leader(),
        chord: Modifier::Shift,
    };
    assert_eq!(
        effective
            .warnings
            .iter()
            .map(|warning| warning.warning)
            .collect::<Vec<_>>(),
        nc_schema::keys::warnings(part, &effective.tables)
    );
    scratch.configure(Channel::Staging, HERE, r#"{"format":1}"#);
    let effective = keys::effective(&staging);
    assert_the_defaults(&effective);
    assert!(effective.refused.is_empty(), "{effective:?}");
}

/// The warnings are those of the keys in force.
///
/// A configured Leader part that is accepted is warned of, with every binding
/// pressed with it; one that is refused is not, and the warnings of the
/// default it reverts to stand in its place.
#[test]
fn the_warnings_are_those_of_the_keys_in_force() {
    let scratch = Scratch::new("warned");
    let staging = scratch.installed(Channel::Staging, HERE);
    let default = LeaderPart {
        leader: Channel::Staging.leader(),
        chord: Modifier::Shift,
    };
    let warned = |effective: &Effective| -> Vec<Warning> {
        effective
            .warnings
            .iter()
            .map(|warning| warning.warning)
            .collect()
    };
    // **Windows keeps both chords this presses** (`SYSTEM_SHORTCUTS`): the
    // Leader itself, and its modifiers held with `Digit1`. `Leader+Space` is
    // held on the Leader's own key, which no hand presses.
    scratch.configure(
        Channel::Staging,
        HERE,
        r#"{"bindings":{"kernel.redo":["Leader+Space"],"kernel.undo":["Leader+Digit1"]},"format":1,"leader":"Control+Super+Space"}"#,
    );
    let accepted = keys::effective(&staging);
    assert_the_leader(&accepted, "Control+Super+Space");
    assert_eq!(
        warned(&accepted),
        nc_schema::keys::warnings(accepted.part(), &accepted.tables)
    );
    assert_ne!(
        warned(&accepted),
        nc_schema::keys::warnings(default, &accepted.tables)
    );
    assert!(
        accepted
            .warnings
            .iter()
            .all(|warning| warning.part == accepted.part()),
        "{accepted:?}"
    );
    assert!(
        warned(&accepted).iter().any(|warning| matches!(
            warning,
            Warning::TheSystemUses { binding: None, shortcut } if shortcut.system == "Windows"
        )),
        "{accepted:?}"
    );
    assert!(
        warned(&accepted).iter().any(|warning| matches!(
            warning,
            Warning::TheSystemUses { binding: Some(held), .. } if *held == binding("Leader+Digit1")
        )),
        "{accepted:?}"
    );
    assert!(
        warned(&accepted).iter().any(|warning| matches!(
            warning,
            Warning::Unpressable { binding: held } if *held == binding("Leader+Space")
        )),
        "{accepted:?}"
    );
    // **Each with its own code**, which a log is queried by and help heads
    // the warning with: a chord a system keeps is one, and a key no hand
    // presses is the other.
    for warning in &accepted.warnings {
        let code = match warning.warning {
            Warning::TheSystemUses { .. } => ErrorCode::ChannelKeyTheSystemUses,
            Warning::Unpressable { .. } => ErrorCode::ChannelKeyUnpressable,
        };
        assert_eq!(warning.code(), code, "{warning}");
        assert!(
            warning
                .to_string()
                .starts_with(&format!("[{}] ", code.code())),
            "{warning}"
        );
    }
    // **Refused, it raises nothing**, though Windows keeps it too: what is
    // pressed is the default.
    scratch.configure(Channel::Staging, HERE, &leader_of("Shift+Super+Space"));
    let refused = keys::effective(&staging);
    assert_the_leader_refused(&refused, LeaderRule::HoldsTheChordModifier, None);
    let configured = LeaderPart {
        leader: Leader::of(chord("Shift+Super+Space")).expect("a Leader"),
        chord: Modifier::Shift,
    };
    assert_eq!(
        warned(&refused),
        nc_schema::keys::warnings(default, &refused.tables)
    );
    assert_ne!(
        warned(&refused),
        nc_schema::keys::warnings(configured, &refused.tables)
    );
    assert!(
        refused
            .warnings
            .iter()
            .all(|warning| warning.part == default),
        "{refused:?}"
    );
}

/// A configuration overrides the catalog's bindings and the Leader in its own
/// channel alone.
///
/// Over a catalog that binds keys, a command's chosen list replaces its
/// defaults, `[]` unbinds it, and every other command keeps its own; the
/// channel beside it, under the same bases, keeps its defaults.
#[test]
fn a_configuration_overrides_the_catalogs_bindings_in_its_channel() {
    let scratch = Scratch::new("over");
    let staging = scratch.installed(Channel::Staging, HERE);
    let production = scratch.installed(Channel::Production, HERE);
    scratch.configure(
        Channel::Staging,
        HERE,
        r#"{"bindings":{"kernel.undo":["Leader+KeyU"]},"format":1,"leader":"Control+Super+Space"}"#,
    );
    let effective = keys::effective(&staging);
    assert_the_leader(&effective, "Control+Super+Space");
    assert!(effective.bindings_configured, "{effective:?}");
    assert_eq!(
        effective.tables.command(binding("Leader+KeyU")),
        Some("kernel.undo")
    );
    let theirs = keys::effective(&production);
    assert_the_defaults(&theirs);
    assert!(theirs.refused.is_empty(), "{theirs:?}");

    let catalog = vec![
        ("journal.verify".to_owned(), bound(&["Leader KeyV"])),
        ("kernel.redo".to_owned(), bound(&["Leader+Chord+KeyZ"])),
        ("kernel.undo".to_owned(), bound(&["Leader+KeyZ"])),
    ];
    let chosen = Configuration::parse(
        r#"{"bindings":{"kernel.redo":[],"kernel.undo":["Leader+KeyU"]},"format":1}"#,
    )
    .expect("a configuration");
    let judged = keys::judged(&staging, Some(&chosen), &catalog);
    assert!(judged.refused.is_empty(), "{judged:?}");
    assert!(judged.bindings_configured);
    assert_eq!(
        judged.tables.iter().collect::<Vec<_>>(),
        [
            (binding("Leader KeyV"), "journal.verify"),
            (binding("Leader+KeyU"), "kernel.undo"),
        ]
    );
    let unchosen = keys::judged(&staging, None, &catalog);
    assert!(!unchosen.bindings_configured);
    assert_eq!(
        unchosen.tables.iter().collect::<Vec<_>>(),
        [
            (binding("Leader KeyV"), "journal.verify"),
            (binding("Leader+KeyZ"), "kernel.undo"),
            (binding("Leader+Chord+KeyZ"), "kernel.redo"),
        ]
    );
}

/// Two checkouts share the development Leader until one is configured, and
/// the configuration that would meet the other is refused.
///
/// On their defaults neither is refused for the other (`NOCTUA.md §17.1`); a
/// checkout configured with the development chord while the other keeps it is
/// refused, and so is one configured with the Leader the checkout settled
/// before it chose, which keeps its own.
#[test]
fn two_checkouts_share_the_development_leader_until_one_is_configured_and_the_configuration_refuses_the_collision()
 {
    let scratch = Scratch::new("two");
    let (first, second) = (development("0123abcd"), development("89efcd45"));
    let one = scratch.installed(first, HERE);
    let other = scratch.installed(second, HERE);
    for found in [&one, &other] {
        let effective = keys::effective(found);
        assert_the_defaults(&effective);
        assert!(effective.refused.is_empty(), "{effective:?}");
    }
    assert_eq!(keys::effective(&one).leader, keys::effective(&other).leader);

    scratch.configure(second, HERE, &leader_of("Alt+Super+Space"));
    assert_the_leader_refused(
        &keys::effective(&other),
        LeaderRule::SharedWithAnother,
        Some(first),
    );
    scratch.configure(first, HERE, &leader_of("Alt+Super+Space"));
    assert_the_leader_refused(
        &keys::effective(&one),
        LeaderRule::SharedWithAnother,
        Some(second),
    );

    scratch.configure(first, HERE, &leader_of("Control+Super+Space"));
    scratch.configure(second, HERE, &leader_of("Control+Super+Space"));
    assert_the_leader(&keys::effective(&one), "Control+Super+Space");
    let refused = keys::effective(&other);
    assert_the_leader_refused(&refused, LeaderRule::SharedWithAnother, Some(first));
    assert!(
        refused.refused[0]
            .to_string()
            .contains("the development checkout 0123abcd"),
        "the refusal does not name the checkout it meets: {}",
        refused.refused[0]
    );
}

/// Production keeps its Leader when a checkout configures the same: it reads
/// nobody's file, and the checkout's configuration is the one refused.
///
/// On its default as much as configured, and a checkout's Leader that is
/// production's held with its chord is refused by that rule.
#[test]
fn production_keeps_its_leader_when_a_checkout_configures_the_same() {
    let scratch = Scratch::new("prod");
    let checkout = development("0123abcd");
    let production = scratch.installed(Channel::Production, HERE);
    let developed = scratch.installed(checkout, HERE);
    scratch.configure(Channel::Production, HERE, &leader_of("Control+Super+Space"));
    scratch.configure(checkout, HERE, &leader_of("Control+Super+Space"));
    assert_the_leader(&keys::effective(&production), "Control+Super+Space");
    assert_the_leader_refused(
        &keys::effective(&developed),
        LeaderRule::SharedWithAnother,
        Some(Channel::Production),
    );

    let production_file =
        layout(&scratch.install(Channel::Production, HERE), "scratch").keybindings;
    std::fs::remove_file(production_file).expect("production's configuration is removed");
    scratch.configure(checkout, HERE, &leader_of("Alt+Space"));
    assert_the_leader_refused(
        &keys::effective(&developed),
        LeaderRule::SharedWithAnother,
        Some(Channel::Production),
    );
    scratch.configure(
        checkout,
        HERE,
        r#"{"chord":"Control","format":1,"leader":"Alt+Shift+Space"}"#,
    );
    assert_the_leader_refused(
        &keys::effective(&developed),
        LeaderRule::HeldWithTheChordIsAnothers,
        Some(Channel::Production),
    );
    let effective = keys::effective(&production);
    assert_the_defaults(&effective);
    assert!(effective.refused.is_empty(), "{effective:?}");
}

/// Each rule a Leader part breaks is refused with its own code, and the
/// channel's default Leader and chord modifier stand.
///
/// A chord modifier alone is weighed against the default Leader it would be
/// held with.
#[test]
fn each_rule_a_leader_breaks_is_refused_with_its_own_code() {
    let scratch = Scratch::new("rules");
    scratch.installed(Channel::Production, HERE);
    let staging = scratch.installed(Channel::Staging, HERE);
    for (text, rule, by) in [
        (
            r#"{"chord":"Control","format":1}"#,
            LeaderRule::HoldsTheChordModifier,
            None,
        ),
        (
            r#"{"format":1,"leader":"KeyQ"}"#,
            LeaderRule::WithoutAModifier,
            None,
        ),
        (
            r#"{"chord":"Control","format":1,"leader":"Alt+Shift+Space"}"#,
            LeaderRule::HeldWithTheChordIsAnothers,
            Some(Channel::Production),
        ),
        (
            r#"{"format":1,"leader":"Alt+Space"}"#,
            LeaderRule::SharedWithAnother,
            Some(Channel::Production),
        ),
    ] {
        scratch.configure(Channel::Staging, HERE, text);
        assert_the_leader_refused(&keys::effective(&staging), rule, by);
    }
    scratch.configure(
        Channel::Staging,
        HERE,
        r#"{"chord":"Super","format":1,"leader":"Control+Alt+F12"}"#,
    );
    let effective = keys::effective(&staging);
    assert!(effective.refused.is_empty(), "{effective:?}");
    assert_eq!(effective.leader.chord(), chord("Control+Alt+F12"));
    assert_eq!(effective.chord, Modifier::Super);
    // **The rule that needs no other channel is judged before any is read**:
    // beside a file this build cannot judge, a part that breaks it is still
    // refused for that, and one that keeps it is refused for the file.
    scratch.configure(
        Channel::Production,
        HERE,
        r#"{"format":2,"leader":"Control+Super+Space"}"#,
    );
    for text in [
        r#"{"chord":"Control","format":1}"#,
        r#"{"format":1,"leader":"Alt+Shift+Space"}"#,
    ] {
        scratch.configure(Channel::Staging, HERE, text);
        assert_the_leader_refused(
            &keys::effective(&staging),
            LeaderRule::HoldsTheChordModifier,
            None,
        );
    }
    scratch.configure(Channel::Staging, HERE, &leader_of("Control+Alt+F12"));
    let unjudged = keys::effective(&staging);
    assert_eq!(
        the_refusal(&unjudged, ErrorCode::ChannelKeybindingsUnjudged).part(),
        Part::Leader
    );
}

/// A chord modifier configured alone and refused keeps both defaults, and
/// says so.
///
/// It is weighed with the channel's default Leader, which is no Leader the
/// person chose: the refusal names the part it judged and the rule it
/// breaks, the default Leader and chord modifier stand, and neither locale's
/// sentence for its code says a Leader was chosen. On staging it holds its
/// own chord modifier; on production, held with it, it is staging's Leader;
/// and on a second checkout it is the first's.
#[test]
fn a_chord_modifier_configured_alone_and_refused_keeps_both_defaults_and_names_the_rule() {
    let scratch = Scratch::new("alone");
    let (first, second) = (development("0123abcd"), development("89efcd45"));
    let production = scratch.installed(Channel::Production, HERE);
    let staging = scratch.installed(Channel::Staging, HERE);
    scratch.installed(first, HERE);
    let checkout = scratch.installed(second, HERE);
    for (channel, found, rule, by) in [
        (
            Channel::Staging,
            &staging,
            LeaderRule::HoldsTheChordModifier,
            None,
        ),
        (
            Channel::Production,
            &production,
            LeaderRule::HeldWithTheChordIsAnothers,
            Some(Channel::Staging),
        ),
        (
            second,
            &checkout,
            LeaderRule::SharedWithAnother,
            Some(first),
        ),
    ] {
        scratch.configure(channel, HERE, r#"{"chord":"Control","format":1}"#);
        let effective = keys::effective(found);
        assert_the_leader_refused(&effective, rule, by);
        let refused = &effective.refused[0];
        let part = format!(
            "the configured Leader part (Leader `{}`, chord modifier `Control`) is refused",
            channel.leader().chord()
        );
        assert!(refused.to_string().contains(&part), "{refused}");
        for (locale, chosen, kept) in [
            (
                nc_catalog::Locale::En,
                "you chose",
                "this installation's standard Leader and added key",
            ),
            (
                nc_catalog::Locale::PtBR,
                "você escolheu",
                "o Leader padrão desta instalação e a tecla padrão acrescentada a ele",
            ),
        ] {
            let message =
                nc_catalog::message(refused.code().message_key(), locale).expect("a message");
            assert!(
                !message.contains(chosen) && message.contains(kept),
                "{locale:?}: {message}"
            );
        }
        scratch.configure(channel, HERE, r#"{"format":1}"#);
    }
}

/// A channel is judged against the Leaders settled before it and the defaults
/// after it, and never refused for what a channel after it configured.
#[test]
fn a_channel_is_judged_against_the_leaders_settled_before_it_and_the_defaults_after_it() {
    let scratch = Scratch::new("order");
    let checkout = development("0123abcd");
    let production = scratch.installed(Channel::Production, HERE);
    let staging = scratch.installed(Channel::Staging, HERE);
    let developed = scratch.installed(checkout, HERE);
    scratch.configure(checkout, HERE, &leader_of("Control+Super+Space"));
    assert_the_leader(&keys::effective(&developed), "Control+Super+Space");
    scratch.configure(Channel::Staging, HERE, &leader_of("Control+Super+Space"));
    assert_the_leader(&keys::effective(&staging), "Control+Super+Space");
    assert_the_leader_refused(
        &keys::effective(&developed),
        LeaderRule::SharedWithAnother,
        Some(Channel::Staging),
    );
    // **Against staging's default, though staging configured another**:
    // production reads no file, so what it settles to holds whatever a
    // channel after it chose.
    scratch.configure(Channel::Production, HERE, &leader_of("Control+Alt+Space"));
    assert_the_leader_refused(
        &keys::effective(&production),
        LeaderRule::SharedWithAnother,
        Some(Channel::Staging),
    );
    // **Nor is a file after it read at all**: one of a later format, which this
    // build could not judge, refuses nothing of a channel before it.
    scratch.configure(
        Channel::Staging,
        HERE,
        r#"{"format":2,"leader":"Control+Super+Space"}"#,
    );
    scratch.configure(Channel::Production, HERE, &leader_of("Control+Alt+F12"));
    assert_the_leader(&keys::effective(&production), "Control+Alt+F12");
}

/// An uninstalled channel binds nothing: what it left in its configuration
/// directory is read only while its manifest is there.
#[test]
fn an_uninstalled_channel_binds_nothing() {
    let scratch = Scratch::new("gone");
    let checkout = development("0123abcd");
    let staging = scratch.installed(Channel::Staging, HERE);
    scratch.configure(Channel::Production, HERE, &leader_of("Control+Super+Space"));
    scratch.configure(Channel::Staging, HERE, &leader_of("Control+Super+Space"));
    assert_the_leader(&keys::effective(&staging), "Control+Super+Space");
    scratch.installed(Channel::Production, HERE);
    assert_the_leader_refused(
        &keys::effective(&staging),
        LeaderRule::SharedWithAnother,
        Some(Channel::Production),
    );
    scratch.uninstalled(Channel::Production, HERE);
    assert_the_leader(&keys::effective(&staging), "Control+Super+Space");
    scratch.configure(checkout, HERE, &leader_of("Control+Alt+F12"));
    scratch.configure(Channel::Staging, HERE, &leader_of("Alt+Super+Space"));
    assert_the_leader(&keys::effective(&staging), "Alt+Super+Space");
    scratch.installed(checkout, HERE);
    assert_the_leader_refused(
        &keys::effective(&staging),
        LeaderRule::SharedWithAnother,
        Some(checkout),
    );
}

/// A file another account can reach is refused whole, and the defaults stand;
/// so is one in a directory another account can reach, and each is left as it
/// was.
#[test]
fn a_file_another_account_can_reach_is_refused_and_the_defaults_stand() {
    let scratch = Scratch::new("open");
    let staging = scratch.installed(Channel::Staging, HERE);
    let file = scratch.configure(
        Channel::Staging,
        HERE,
        r#"{"bindings":{"kernel.undo":["Leader+KeyU"]},"format":1,"leader":"Control+Super+Space"}"#,
    );
    assert_the_leader(&keys::effective(&staging), "Control+Super+Space");
    for reached in [file.clone(), staging.layout.config.clone()] {
        let opened =
            nc_platform::testing::open_to_another_user(&reached).expect("opened to another user");
        let effective = keys::effective(&staging);
        let refused = the_refusal(&effective, ErrorCode::ChannelKeybindingsNotPrivate);
        assert_eq!(refused.part(), Part::Whole);
        assert!(refused.to_string().contains(&opened), "{opened}: {refused}");
        assert_the_defaults(&effective);
        assert!(
            !nc_platform::testing::is_private(&reached),
            "a path that was refused was changed"
        );
        nc_platform::testing::make_private(&reached).expect("private again");
    }
    assert_the_leader(&keys::effective(&staging), "Control+Super+Space");
}

/// A file that is a link is refused however it points, and so is a
/// configuration directory that is one, and the defaults stand.
#[test]
fn a_file_that_is_a_link_is_refused() {
    let scratch = Scratch::new("link");
    let staging = scratch.installed(Channel::Staging, HERE);
    let file = scratch.configure(Channel::Staging, HERE, &leader_of("Control+Super+Space"));
    let elsewhere = scratch.0.join("elsewhere.json");
    write_private(&elsewhere, leader_of("Control+Super+Space").as_bytes());
    std::fs::remove_file(&file).expect("the file is removed");
    nc_platform::testing::link_file(&elsewhere, &file).expect("a link");
    let effective = keys::effective(&staging);
    the_refusal(&effective, ErrorCode::ChannelKeybindingsNotPrivate);
    assert_the_defaults(&effective);

    let config = staging.layout.config.clone();
    let moved = scratch.0.join("moved");
    std::fs::remove_file(&file).expect("the link is removed");
    write_private(&file, leader_of("Control+Super+Space").as_bytes());
    std::fs::rename(&config, &moved).expect("the directory is moved");
    nc_platform::testing::link_directory(&moved, &config).expect("a link");
    let effective = keys::effective(&staging);
    the_refusal(&effective, ErrorCode::ChannelKeybindingsNotPrivate);
    assert_the_defaults(&effective);
}

/// Every file this build does not read, beside what its refusal says of it.
///
/// Each is `good`'s fields with one thing wrong, in each way a writer could get
/// one wrong.
fn the_files_this_build_does_not_read(good: &str) -> Vec<(String, String)> {
    let over = format!(
        r#"{{"bindings":{{"kernel.undo":["Leader+KeyU"]}},"format":1,"leader":"Control+Super+Space"{}}}"#,
        " ".repeat(usize::try_from(keys::BYTES_MAX).expect("a bound that fits"))
    );
    let files: Vec<(String, &str)> = vec![
        ("not json".to_owned(), "it is not JSON this build reads"),
        ("[1]".to_owned(), "it is not a JSON object"),
        (
            format!("{{{good},\"mouse\":true}}"),
            "`mouse`, a key this build does not read",
        ),
        (
            r#"{"format":1,"leader":"Alt+Space","leader":"Control+Space"}"#.to_owned(),
            "leader",
        ),
        (
            r#"{"bindings":{"kernel.undo":["C-x C-s"]},"format":1,"leader":"Control+Super+Space"}"#
                .to_owned(),
            "a binding of `kernel.undo` is none",
        ),
        (
            r#"{"bindings":{"kernel.undo":["Leader s"]},"format":1}"#.to_owned(),
            "a binding of `kernel.undo` is none",
        ),
        (
            r#"{"bindings":{"Kernel.Undo":[]},"format":1,"leader":"Control+Super+Space"}"#
                .to_owned(),
            "`Kernel.Undo`, which no command can be named",
        ),
        (
            r#"{"bindings":["Leader+KeyU"],"format":1}"#.to_owned(),
            "its `bindings` is not an object",
        ),
        (
            r#"{"bindings":{"kernel.undo":"Leader+KeyU"},"format":1}"#.to_owned(),
            "the bindings of `kernel.undo` are not a list",
        ),
        (
            r#"{"bindings":{"kernel.undo":[1]},"format":1}"#.to_owned(),
            "a binding of `kernel.undo` is not text",
        ),
        (
            r#"{"format":1,"leader":"alt+space"}"#.to_owned(),
            "its `leader` is no chord",
        ),
        (
            r#"{"format":1,"leader":1}"#.to_owned(),
            "its `leader` is not text",
        ),
        (
            r#"{"chord":"Hyper","format":1,"leader":"Control+Super+Space"}"#.to_owned(),
            "its `chord` is `Hyper`",
        ),
        (format!("{{{good},\"format\":2}}"), "format"),
        (
            r#"{"bindings":{},"format":2,"leader":"Control+Super+Space"}"#.to_owned(),
            "its format is 2, and this build reads format 1",
        ),
        (
            r#"{"format":0,"leader":"Control+Super+Space"}"#.to_owned(),
            "its format is 0",
        ),
        (
            r#"{"format":"1","leader":"Control+Super+Space"}"#.to_owned(),
            "its `format` is not a whole number",
        ),
        (
            r#"{"leader":"Control+Super+Space"}"#.to_owned(),
            "it has no `format`",
        ),
        (over, "it holds more than 16384 bytes"),
    ];
    files
        .into_iter()
        .map(|(text, said)| (text, said.to_owned()))
        .collect()
}

/// A file this build does not read is refused whole, naming what it could not
/// read, and every default stands: its Leader part and its bindings part too.
#[test]
fn a_malformed_file_is_refused_whole_naming_what_it_could_not_read() {
    let scratch = Scratch::new("bad");
    let staging = scratch.installed(Channel::Staging, HERE);
    let good =
        r#""bindings":{"kernel.undo":["Leader+KeyU"]},"format":1,"leader":"Control+Super+Space""#;
    for (text, said) in the_files_this_build_does_not_read(good) {
        scratch.configure(Channel::Staging, HERE, &text);
        let effective = keys::effective(&staging);
        let refused = the_refusal(&effective, ErrorCode::ChannelKeybindingsUnreadable);
        assert_eq!(refused.part(), Part::Whole, "{text}");
        assert!(
            refused.to_string().contains(&said),
            "{text}: the refusal does not say `{said}`: {refused}"
        );
        assert!(
            refused
                .to_string()
                .ends_with("and the channel's defaults stand"),
            "{refused}"
        );
        assert_the_defaults(&effective);
    }
    // At the bound, the same file is read.
    let at_the_bound = format!(
        "{{{good}{}}}",
        " ".repeat(usize::try_from(keys::BYTES_MAX).expect("a bound that fits") - good.len() - 2)
    );
    assert_eq!(
        u64::try_from(at_the_bound.len()).expect("a length"),
        keys::BYTES_MAX
    );
    scratch.configure(Channel::Staging, HERE, &at_the_bound);
    assert_the_leader(&keys::effective(&staging), "Control+Super+Space");
}

/// A choice past the file's bound is refused before it is written, and one a
/// writer judges is the file its reader reads.
///
/// The largest choice that fits is accepted by `judged`, and its canonical
/// form, written as the file, is read by `effective` as the same keys; one
/// binding more is refused by `Configuration::parse` as the reader refuses it.
#[test]
fn a_choice_past_the_files_bound_is_refused_before_it_is_written() {
    let scratch = Scratch::new("writer");
    let staging = scratch.installed(Channel::Staging, HERE);
    let bound = usize::try_from(keys::BYTES_MAX).expect("a bound that fits");
    let choice = |count: usize| {
        let mut named = vec![r#""kernel.undo":["Leader+KeyU"]"#.to_owned()];
        named.extend((0..count).map(|at| format!(r#""plugin.command-number-{at:04}":[]"#)));
        format!(
            r#"{{"bindings":{{{}}},"format":1,"leader":"Control+Super+Space"}}"#,
            named.join(",")
        )
    };
    let most = (0..1000)
        .take_while(|&count| choice(count).len() <= bound)
        .last()
        .expect("a choice fits");
    let proposed = Configuration::parse(&choice(most)).expect("a choice within the bound");
    let judged = keys::judged(&staging, Some(&proposed), &keys::catalog_bindings());
    assert!(judged.refused.is_empty(), "{:?}", judged.refused);
    assert!(judged.leader_configured && judged.bindings_configured);
    let written = proposed.to_canonical();
    assert!(written.len() <= bound);
    scratch.configure(Channel::Staging, HERE, &written);
    assert_eq!(keys::effective(&staging), judged);

    let past = choice(most + 1);
    assert!(past.len() > bound);
    let refused = Configuration::parse(&past).expect_err("a choice past the bound");
    assert_eq!(refused.code(), ErrorCode::ChannelKeybindingsUnreadable);
    assert_eq!(refused.part(), Part::Whole);
    assert!(
        refused
            .to_string()
            .contains("it holds more than 16384 bytes"),
        "{refused}"
    );
    scratch.configure(Channel::Staging, HERE, &past);
    let effective = keys::effective(&staging);
    assert_eq!(
        the_refusal(&effective, ErrorCode::ChannelKeybindingsUnreadable),
        &refused
    );
    assert_the_defaults(&effective);
}

/// What a refusal quotes of the file as it was written is escaped and cut
/// short, so no file forges a line of what refuses it.
///
/// A command's name is quoted so until it is one a command can have; once it
/// is, it is repeated whole, and it holds nothing that ends a line.
#[test]
fn a_refusal_quotes_the_file_escaped_and_cut() {
    let scratch = Scratch::new("quote");
    let staging = scratch.installed(Channel::Staging, HERE);
    let forged = format!("\u{1b}[2J\nerror[forged]{}", "x".repeat(100));
    for text in [
        format!(
            r#"{{"format":1,"{}":true}}"#,
            forged.replace('\n', "\\n").replace('\u{1b}', "\\u001b")
        ),
        format!(
            r#"{{"chord":"{}","format":1}}"#,
            forged.replace('\n', "\\n").replace('\u{1b}', "\\u001b")
        ),
        format!(
            r#"{{"format":1,"leader":"{}"}}"#,
            forged.replace('\n', "\\n").replace('\u{1b}', "\\u001b")
        ),
        format!(
            r#"{{"bindings":{{"kernel.undo":["Leader+{}"]}},"format":1}}"#,
            forged.replace('\n', "\\n").replace('\u{1b}', "\\u001b")
        ),
        format!(
            r#"{{"bindings":{{"kernel.{}":[]}},"format":1}}"#,
            forged.replace('\n', "\\n").replace('\u{1b}', "\\u001b")
        ),
    ] {
        scratch.configure(Channel::Staging, HERE, &text);
        let effective = keys::effective(&staging);
        let said = the_refusal(&effective, ErrorCode::ChannelKeybindingsUnreadable).to_string();
        assert!(!said.chars().any(char::is_control), "{text}: {said:?}");
        assert!(!said.contains(&"x".repeat(65)), "{text}: not cut: {said}");
        assert!(said.contains('…'), "{text}: no mark of the cut: {said}");
    }
    let longest = format!(
        "kernel.{}",
        "a".repeat(nc_schema::NAME_MAX - "kernel.".len())
    );
    assert!(nc_schema::command_name_is_well_formed(&longest));
    scratch.configure(
        Channel::Staging,
        HERE,
        &format!(r#"{{"bindings":{{"{longest}":"Leader+KeyU"}},"format":1}}"#),
    );
    let effective = keys::effective(&staging);
    let said = the_refusal(&effective, ErrorCode::ChannelKeybindingsUnreadable).to_string();
    assert!(
        said.contains(&format!("the bindings of `{longest}` are not a list")),
        "{said}"
    );
}

/// A refused bindings part keeps the configured Leader in force, and a
/// refused Leader part keeps the configured bindings.
#[test]
fn a_refused_bindings_part_keeps_the_configured_leader() {
    let scratch = Scratch::new("parts");
    let staging = scratch.installed(Channel::Staging, HERE);
    scratch.configure(
        Channel::Staging,
        HERE,
        r#"{"bindings":{"kernel.redo":["Leader+KeyZ"],"kernel.undo":["Leader+KeyZ"]},"format":1,"leader":"Control+Super+Space"}"#,
    );
    let effective = keys::effective(&staging);
    let refused = the_refusal(&effective, ErrorCode::ChannelKeyBoundTwice);
    assert_eq!(refused.part(), Part::Bindings);
    let said = refused.to_string();
    for named in ["kernel.redo", "kernel.undo", "Leader+KeyZ"] {
        assert!(said.contains(named), "{said}");
    }
    assert!(said.ends_with("and the catalog's bindings stand"), "{said}");
    assert!(effective.leader_configured);
    assert_eq!(effective.leader.chord(), chord("Control+Super+Space"));
    assert!(!effective.bindings_configured);
    assert_eq!(effective.tables, the_catalogs_tables());

    scratch.configure(
        Channel::Staging,
        HERE,
        r#"{"bindings":{"kernel.undo":["Leader+KeyU"]},"format":1,"leader":"Shift+Space"}"#,
    );
    let effective = keys::effective(&staging);
    assert_the_leader_refused(&effective, LeaderRule::HoldsTheChordModifier, None);
    assert!(effective.bindings_configured);
    assert_eq!(
        effective.tables.command(binding("Leader+KeyU")),
        Some("kernel.undo")
    );
}

/// A file before this channel that its own reader refuses is that channel's
/// defaults, as its reader takes it, and one this build cannot judge refuses
/// this channel's configured Leader.
///
/// Of a later format, the configured Leader is refused naming whose file it
/// is, its default stands, and the bindings part is judged as ever.
#[test]
fn a_checkout_whose_predecessor_cannot_be_judged_keeps_its_default_leader() {
    let scratch = Scratch::new("unjudged");
    let checkout = development("0123abcd");
    scratch.installed(Channel::Production, HERE);
    let developed = scratch.installed(checkout, HERE);
    let mine =
        r#"{"bindings":{"kernel.undo":["Leader+KeyU"]},"format":1,"leader":"Control+Super+Space"}"#;
    scratch.configure(checkout, HERE, mine);
    for theirs in [
        r#"{"format":1,"leader":"Control+Super+Space","mouse":true}"#,
        r#"{"format":1,"leader":"Space"}"#,
        r#"{"format":0,"leader":"Control+Super+Space"}"#,
    ] {
        scratch.configure(Channel::Production, HERE, theirs);
        assert_the_leader(&keys::effective(&developed), "Control+Super+Space");
    }
    let file = scratch.configure(Channel::Production, HERE, &leader_of("Control+Super+Space"));
    assert_the_leader_refused(
        &keys::effective(&developed),
        LeaderRule::SharedWithAnother,
        Some(Channel::Production),
    );
    nc_platform::testing::open_to_another_user(&file).expect("opened to another user");
    assert_the_leader(&keys::effective(&developed), "Control+Super+Space");

    scratch.configure(
        Channel::Production,
        HERE,
        r#"{"format":2,"leader":"Control+Super+Space","mouse":true}"#,
    );
    let effective = keys::effective(&developed);
    let refused = the_refusal(&effective, ErrorCode::ChannelKeybindingsUnjudged);
    assert_eq!(refused.part(), Part::Leader);
    assert!(
        refused
            .to_string()
            .contains("production's keybinding configuration is of format 2"),
        "{refused}"
    );
    assert_eq!(effective.leader, checkout.leader());
    assert!(effective.bindings_configured, "{effective:?}");
    std::fs::remove_file(
        layout(&scratch.install(Channel::Production, HERE), "scratch").keybindings,
    )
    .expect("production's configuration is removed");
    assert_the_leader(&keys::effective(&developed), "Control+Super+Space");
}

/// More installed checkouts than are read, or a state base that cannot be
/// listed, refuses the configured Leader, and profiles an uninstall left
/// refuse nothing.
///
/// Rather than judge it against some of the channels beside it, the default
/// stands. **Only an installed checkout counts**: every checkout ever installed
/// keeps its profile when it is uninstalled, and one whose manifest is gone
/// binds nothing, so as many of those as the bound and one more refuse nothing.
#[test]
fn checkouts_past_their_bound_or_a_base_that_will_not_list_refuse_the_configured_leader() {
    let scratch = Scratch::new("bound");
    let staging = scratch.installed(Channel::Staging, HERE);
    scratch.configure(Channel::Staging, HERE, &leader_of("Control+Alt+F12"));
    let mut leftovers = Vec::new();
    for digest in 1..=CHANNELS_MAX + 1 {
        let leftover = Checkout::parse(&format!("e{digest:07x}")).expect("eight digits");
        let names = layout(
            &scratch.install(Channel::Development(leftover), HERE),
            "scratch",
        );
        scratch.made(&names.state);
        leftovers.push(names.state);
    }
    assert_the_leader(&keys::effective(&staging), "Control+Alt+F12");
    for leftover in &leftovers {
        std::fs::remove_dir(leftover).expect("the leftover is removed");
    }
    for digest in 1..=CHANNELS_MAX {
        scratch.installed(development(&format!("f{digest:07x}")), HERE);
    }
    assert_the_leader(&keys::effective(&staging), "Control+Alt+F12");
    let one_more = development("f1000000");
    scratch.installed(one_more, HERE);
    let refused = keys::effective(&staging);
    assert!(
        the_refusal(&refused, ErrorCode::ChannelKeybindingsUnjudged)
            .to_string()
            .contains(&format!(
                "more than {CHANNELS_MAX} development checkouts are installed"
            )),
        "{refused:?}"
    );
    // **Rule 1 before the checkouts are listed**: a part that breaks it is
    // refused for that, past the bound too.
    scratch.configure(Channel::Staging, HERE, r#"{"chord":"Control","format":1}"#);
    assert_the_leader_refused(
        &keys::effective(&staging),
        LeaderRule::HoldsTheChordModifier,
        None,
    );
    scratch.configure(Channel::Staging, HERE, &leader_of("Control+Alt+F12"));
    scratch.uninstalled(one_more, HERE);
    assert_the_leader(&keys::effective(&staging), "Control+Alt+F12");

    nc_platform::testing::hide_its_entries(&scratch.0.join("s")).expect("the base is hidden");
    let unlisted = keys::effective(&staging);
    assert!(
        the_refusal(&unlisted, ErrorCode::ChannelKeybindingsUnjudged)
            .to_string()
            .contains("could not be listed"),
        "{unlisted:?}"
    );
    nc_platform::testing::make_private(&scratch.0.join("s")).expect("listed again");
}

/// Profiles past what a listing reads refuse the configured Leader, and
/// as many as it reads refuse nothing.
///
/// **The listing's own bound**, far past the installed checkouts' because
/// every checkout ever installed leaves a profile: each is asked whether its
/// manifest is there, and this bounds how many are asked.
#[test]
fn profiles_past_what_a_listing_reads_refuse_the_configured_leader() {
    let scratch = Scratch::new("profiles");
    let staging = scratch.installed(Channel::Staging, HERE);
    scratch.configure(Channel::Staging, HERE, &leader_of("Control+Alt+F12"));
    for digest in 1..=PROFILES_MAX {
        let leftover = Checkout::parse(&format!("d{digest:07x}")).expect("eight digits");
        let names = layout(
            &scratch.install(Channel::Development(leftover), HERE),
            "scratch",
        );
        scratch.made(&names.state);
    }
    assert_the_leader(&keys::effective(&staging), "Control+Alt+F12");
    let one_more = layout(&scratch.install(development("d1000000"), HERE), "scratch");
    scratch.made(&one_more.state);
    let refused = keys::effective(&staging);
    assert!(
        the_refusal(&refused, ErrorCode::ChannelKeybindingsUnjudged)
            .to_string()
            .contains(&format!("more than {PROFILES_MAX} development profiles")),
        "{refused:?}"
    );
}

/// An install never reads a configuration under another namespace, under the
/// same bases: a test's install never meets a real one's, nor another test's.
#[test]
fn an_install_never_reads_a_configuration_under_another_namespace() {
    let scratch = Scratch::new("spaces");
    let checkout = development("0123abcd");
    let staging = scratch.installed(Channel::Staging, HERE);
    scratch.installed(Channel::Production, "z9");
    scratch.installed(checkout, "z9");
    scratch.configure(Channel::Production, "z9", &leader_of("Control+Super+Space"));
    for leader in ["Control+Super+Space", "Alt+Space", "Alt+Super+Space"] {
        scratch.configure(Channel::Staging, HERE, &leader_of(leader));
        assert_the_leader(&keys::effective(&staging), leader);
    }
}

/// Nothing of another channel is written, moved or removed: every file and
/// directory under the bases is as it was, its bytes and its privacy, after
/// every channel's keys are read.
#[test]
fn nothing_of_another_channel_is_written_or_removed() {
    let scratch = Scratch::new("still");
    let checkout = development("0123abcd");
    let installs = [
        scratch.installed(Channel::Production, HERE),
        scratch.installed(Channel::Staging, HERE),
        scratch.installed(checkout, HERE),
    ];
    for (channel, leader) in [
        (Channel::Production, "Control+Super+Space"),
        (Channel::Staging, "Control+Super+Space"),
        (checkout, "Control+Super+Space"),
    ] {
        scratch.configure(channel, HERE, &leader_of(leader));
    }
    let before = scratch.every_entry();
    for found in &installs {
        let _ = keys::effective(found);
    }
    assert_eq!(scratch.every_entry(), before);
}

/// What `change` came to for `found` over `catalog`: written where the judge
/// took it, the file then holding `Ok`'s text and read back as what the
/// choice said it puts in force; or its refusal, the file left as it was.
fn set(
    found: &Found,
    change: &Change,
    catalog: &[(String, Vec<Binding>)],
) -> Result<String, KeysRefused> {
    let before = std::fs::read(&found.layout.keybindings).ok();
    match keys::proposed(found, change, catalog) {
        Ok(proposed) => {
            assert_eq!(
                std::fs::read(&found.layout.keybindings).ok(),
                before,
                "a choice was written before it was asked to be"
            );
            keys::written(found, &proposed).expect("written");
            let text = std::fs::read_to_string(&found.layout.keybindings).expect("the file");
            assert_eq!(text, proposed.text());
            assert!(nc_platform::testing::is_private(&found.layout.keybindings));
            let read = keys::judged(
                found,
                Some(&Configuration::parse(&text).expect("a file its reader reads")),
                catalog,
            );
            assert_eq!(
                &read,
                proposed.effective(),
                "what was written is not in force"
            );
            assert!(read.refused.is_empty(), "{:?}", read.refused);
            Ok(text)
        }
        Err(refused) => {
            assert_eq!(
                std::fs::read(&found.layout.keybindings).ok(),
                before,
                "a refused choice changed the file"
            );
            Err(refused)
        }
    }
}

/// A catalog of three commands a test's choices are made over.
fn a_catalog() -> Vec<(String, Vec<Binding>)> {
    vec![
        ("a.one".to_owned(), bound(&["Leader KeyA", "Leader+KeyA"])),
        ("a.two".to_owned(), bound(&["Leader KeyB"])),
        ("a.three".to_owned(), Vec::new()),
    ]
}

/// A setting changes its own value and the rest of the file stands, and a
/// choice that is the channel's default is written as no choice.
///
/// **From no file to no choice again**: the Leader alone, then the chord
/// modifier beside it, each written as the grammar writes it; the default
/// Leader takes the configured one out of the file, and the default chord
/// modifier leaves the format alone — the file a reader takes for the
/// defaults. Each write is private, is the text the choice said, and is read
/// back as the keys it said it puts in force.
#[test]
fn a_setting_changes_its_own_value_and_a_default_is_written_as_no_choice() {
    let scratch = Scratch::new("choice-set");
    let found = scratch.installed(Channel::Production, HERE);
    scratch.made(&found.layout.config);
    let catalog = keys::catalog_bindings();
    assert_eq!(
        set(&found, &Change::Leader(chord("Control+Alt+KeyK")), &catalog),
        Ok(leader_of("Control+Alt+KeyK"))
    );
    assert_the_leader(&keys::effective(&found), "Control+Alt+KeyK");
    assert_eq!(
        set(&found, &Change::Chord(Modifier::Super), &catalog).as_deref(),
        Ok(r#"{"chord":"Super","format":1,"leader":"Control+Alt+KeyK"}"#)
    );
    let effective = keys::effective(&found);
    assert_eq!(
        (effective.leader.chord(), effective.chord),
        (chord("Control+Alt+KeyK"), Modifier::Super)
    );
    assert_eq!(
        set(
            &found,
            &Change::Leader(Channel::Production.leader().chord()),
            &catalog
        )
        .as_deref(),
        Ok(r#"{"chord":"Super","format":1}"#),
        "the channel's own Leader was written as a choice"
    );
    assert_eq!(
        set(
            &found,
            &Change::Chord(Channel::Production.chord_modifier()),
            &catalog
        )
        .as_deref(),
        Ok(r#"{"format":1}"#),
        "the channel's own chord modifier was written as a choice"
    );
    assert_the_defaults(&keys::effective(&found));
}

/// The leader menu's keys replace the menu's table and no other, the chords
/// the two a held Leader has, and a command bound as the catalog binds it is
/// left out of the file.
#[test]
fn the_menus_keys_and_the_chords_each_replace_their_own_tables() {
    let scratch = Scratch::new("choice-tables");
    let found = scratch.installed(Channel::Production, HERE);
    scratch.made(&found.layout.config);
    let catalog = a_catalog();
    let menu = |keys: &[(&str, Key)]| {
        Change::Menu(
            keys.iter()
                .map(|(command, key)| ((*command).to_owned(), *key))
                .collect(),
        )
    };
    let held = |keys: &[(&str, Key, bool)]| {
        Change::Held(
            keys.iter()
                .map(|(command, key, chord)| ((*command).to_owned(), *key, *chord))
                .collect(),
        )
    };
    // The menu: `a.one` leaves it and keeps its chord, `a.two` moves, and a
    // command the catalog does not bind, and one it does not hold, join it.
    assert_eq!(
        set(
            &found,
            &menu(&[
                ("a.two", Key::KeyA),
                ("a.three", Key::KeyC),
                ("b.new", Key::KeyD)
            ]),
            &catalog
        )
        .as_deref(),
        Ok(
            r#"{"bindings":{"a.one":["Leader+KeyA"],"a.three":["Leader KeyC"],"a.two":["Leader KeyA"],"b.new":["Leader KeyD"]},"format":1}"#
        )
    );
    // The chords: the menu chosen stands, and `a.one`'s chord moves to the
    // table the chord modifier reaches.
    assert_eq!(
        set(
            &found,
            &held(&[("a.one", Key::KeyA, true), ("a.two", Key::KeyA, false)]),
            &catalog
        )
        .as_deref(),
        Ok(
            r#"{"bindings":{"a.one":["Leader+Chord+KeyA"],"a.three":["Leader KeyC"],"a.two":["Leader KeyA","Leader+KeyA"],"b.new":["Leader KeyD"]},"format":1}"#
        )
    );
    // The catalog's own chords: only the menu's choices are left.
    assert_eq!(
        set(&found, &held(&[("a.one", Key::KeyA, false)]), &catalog).as_deref(),
        Ok(
            r#"{"bindings":{"a.one":["Leader+KeyA"],"a.three":["Leader KeyC"],"a.two":["Leader KeyA"],"b.new":["Leader KeyD"]},"format":1}"#
        )
    );
    // And the catalog's own menu, in another order: nothing is a choice.
    assert_eq!(
        set(
            &found,
            &menu(&[("a.two", Key::KeyB), ("a.one", Key::KeyA)]),
            &catalog
        )
        .as_deref(),
        Ok(r#"{"format":1}"#),
        "a command bound as the catalog binds it was written as a choice"
    );
    // No chord at all unbinds the one the catalog has.
    assert_eq!(
        set(&found, &held(&[]), &catalog).as_deref(),
        Ok(r#"{"bindings":{"a.one":["Leader KeyA"]},"format":1}"#)
    );
}

/// A choice that collides is refused with the code of a choice, nothing is
/// written, and its sentence says the keys in force stand — never a default.
///
/// **Each rule, by the judge that reads the file**: a Leader holding the
/// chord modifier, one with no modifier, one another channel has, a chord
/// modifier the Leader holds, and one key of one table given twice — to two
/// commands and to one — each leaving the file as it was.
#[test]
fn a_choice_that_collides_is_refused_and_nothing_is_written() {
    let scratch = Scratch::new("choice-collides");
    let _production = scratch.installed(Channel::Production, HERE);
    let found = scratch.installed(Channel::Staging, HERE);
    scratch.configure(Channel::Production, HERE, &leader_of("Control+Alt+KeyP"));
    scratch.configure(Channel::Staging, HERE, &leader_of("Control+Alt+KeyK"));
    let catalog = a_catalog();
    let refused_as = |change: Change, code: ErrorCode, what: &str| {
        let refused = set(&found, &change, &catalog).expect_err("refused");
        assert_eq!(refused.code_of_a_choice(), code, "{refused}");
        let said = refused.of_a_choice();
        assert!(said.contains(what), "{said}");
        assert!(
            said.ends_with(", so nothing was written and the keys in force stand"),
            "{said}"
        );
        assert!(!said.contains("default"), "{said}");
        assert!(
            said.contains(&format!("[{}]", refused.code().code())),
            "{said}"
        );
    };
    let collides = ErrorCode::ChannelShortcutLeaderCollides;
    refused_as(
        Change::Leader(chord("Alt+Shift+KeyK")),
        collides,
        &LeaderRule::HoldsTheChordModifier.to_string(),
    );
    refused_as(
        Change::Leader(chord("KeyK")),
        collides,
        &LeaderRule::WithoutAModifier.to_string(),
    );
    refused_as(
        Change::Leader(chord("Control+Alt+KeyP")),
        collides,
        "it meets production's",
    );
    refused_as(
        Change::Chord(Modifier::Alt),
        collides,
        &LeaderRule::HoldsTheChordModifier.to_string(),
    );
    let twice = ErrorCode::ChannelShortcutKeyBoundTwice;
    refused_as(
        Change::Menu(vec![
            ("a.one".to_owned(), Key::KeyC),
            ("a.two".to_owned(), Key::KeyC),
        ]),
        twice,
        "`a.two` and `a.one` are both bound to `Leader KeyC`",
    );
    refused_as(
        Change::Held(vec![
            ("a.one".to_owned(), Key::KeyC, true),
            ("a.one".to_owned(), Key::KeyC, true),
        ]),
        twice,
        "`a.one` and `a.one` are both bound to `Leader+Chord+KeyC`",
    );
    // The same key in the two tables a held Leader has is two bindings.
    assert!(
        set(
            &found,
            &Change::Held(vec![
                ("a.one".to_owned(), Key::KeyC, true),
                ("a.two".to_owned(), Key::KeyC, false),
            ]),
            &catalog
        )
        .is_ok()
    );
    assert_the_leader(&keys::effective(&found), "Control+Alt+KeyK");
}

/// Nothing is written that its reader would refuse any part of: a choice
/// made over a part of the file already refused is refused for that part.
#[test]
fn a_choice_over_a_part_already_refused_is_refused_for_that_part() {
    let scratch = Scratch::new("choice-part");
    let found = scratch.installed(Channel::Production, HERE);
    scratch.configure(
        Channel::Production,
        HERE,
        r#"{"bindings":{"a.one":["Leader KeyB"]},"format":1}"#,
    );
    let refused = set(
        &found,
        &Change::Leader(chord("Control+Alt+KeyK")),
        &a_catalog(),
    )
    .expect_err("the bindings as they stand bind a key twice");
    assert_eq!(
        (refused.code_of_a_choice(), refused.part()),
        (ErrorCode::ChannelShortcutKeyBoundTwice, Part::Bindings),
        "{refused}"
    );
    // The chords leave the menu's collision where it is; the menu's own keys
    // mend it.
    assert!(set(&found, &Change::Held(Vec::new()), &a_catalog()).is_err());
    assert!(
        set(
            &found,
            &Change::Menu(vec![("a.one".to_owned(), Key::KeyA)]),
            &a_catalog()
        )
        .is_ok()
    );
    assert!(
        set(
            &found,
            &Change::Leader(chord("Control+Alt+KeyK")),
            &a_catalog()
        )
        .is_ok()
    );
}

/// A file its reader refuses is never written over, and a choice past the
/// file's bound is refused before it is written.
///
/// **Each as `[NC-CHAN-0027]`, the file left as it was**: one this build does
/// not read, a later format among them, one another account can reach, and a
/// directory another account can reach; and the chords of 146 commands of the
/// longest name, which the judge would take and the file cannot hold.
#[test]
fn a_file_its_reader_refuses_is_never_written_over() {
    let scratch = Scratch::new("choice-over");
    let found = scratch.installed(Channel::Production, HERE);
    let catalog = keys::catalog_bindings();
    let change = Change::Leader(chord("Control+Alt+KeyK"));
    let unsaved = |why: &str| {
        let refused = set(&found, &change, &catalog).expect_err(why);
        assert_eq!(
            (refused.code_of_a_choice(), refused.part()),
            (ErrorCode::ChannelShortcutsUnsaved, Part::Whole),
            "{why}: {refused}"
        );
    };
    for text in [
        r#"{"format":2,"leader":"Alt+KeyL"}"#,
        "not JSON",
        r#"{"format":1,"more":1}"#,
    ] {
        scratch.configure(Channel::Production, HERE, text);
        unsaved(text);
    }
    let file = scratch.configure(Channel::Production, HERE, &leader_of("Control+Alt+KeyL"));
    for reached in [file, found.layout.config.clone()] {
        nc_platform::testing::open_to_another_user(&reached).expect("opened to another user");
        unsaved("a path another account can reach");
        nc_platform::testing::make_private(&reached).expect("private again");
    }
    assert_eq!(
        set(&found, &change, &catalog),
        Ok(leader_of("Control+Alt+KeyK"))
    );
    let long = |at: usize| format!("{}{at:03}", "c".repeat(nc_schema::NAME_MAX - 3));
    let many: Vec<(String, Key, bool)> = Key::ALL
        .into_iter()
        .enumerate()
        .flat_map(|(at, key)| [(long(at), key, false), (long(at + 100), key, true)])
        .collect();
    assert_eq!(many.len(), 146);
    let refused = set(&found, &Change::Held(many), &[]).expect_err("past the bound");
    assert_eq!(
        refused,
        KeysRefused::Unreadable(format!("it holds more than {} bytes", keys::BYTES_MAX))
    );
    assert_eq!(
        refused.code_of_a_choice(),
        ErrorCode::ChannelShortcutsUnsaved
    );
}

/// A choice is written through a private file beside the configuration,
/// into a directory judged this user's alone, and never into one that is
/// not there.
///
/// **What a write that was cut short left** is removed where it is this
/// user's alone and refused where another account can reach it, the
/// configuration left as it was; a directory another account can reach, and
/// one that is not there, are refused before a byte is written; and each
/// refusal carries `[NC-CHAN-0027]` and says the keys in force stand.
#[test]
fn a_choice_is_written_whole_into_a_private_directory_or_not_at_all() {
    let scratch = Scratch::new("choice-written");
    let found = scratch.installed(Channel::Production, HERE);
    let catalog = keys::catalog_bindings();
    let proposed = keys::proposed(&found, &Change::Leader(chord("Control+Alt+KeyK")), &catalog)
        .expect("a choice in force");
    let unwritten = |why: &str| {
        let refused = keys::written(&found, &proposed).expect_err(why);
        assert_eq!(refused.code(), ErrorCode::ChannelShortcutsUnsaved, "{why}");
        let said = refused.to_string();
        assert!(
            said.contains(", so the keybinding configuration was not written")
                && said.ends_with(", and the keys in force stand"),
            "{said}"
        );
        refused
    };
    assert!(
        matches!(unwritten("no directory"), Unwritten::Failed(what) if what.contains("is not there")),
        "a directory that is not there was written into"
    );
    assert!(!found.layout.config.exists(), "the directory was made");
    scratch.made(&found.layout.config);
    let file = scratch.configure(Channel::Production, HERE, &leader_of("Control+Alt+KeyL"));
    let beside = PathBuf::from(format!("{}.new", file.display()));
    write_private(&beside, b"half a wri");
    nc_platform::testing::open_to_another_user(&beside).expect("opened to another user");
    assert!(matches!(
        unwritten("a file left beside it that another account can reach"),
        Unwritten::NotPrivate(_)
    ));
    assert_eq!(
        std::fs::read(&beside).expect("left as it was"),
        b"half a wri"
    );
    nc_platform::testing::make_private(&beside).expect("private again");
    nc_platform::testing::open_to_another_user(&found.layout.config).expect("opened");
    assert!(matches!(
        unwritten("a directory another account can reach"),
        Unwritten::NotPrivate(_)
    ));
    nc_platform::testing::make_private(&found.layout.config).expect("private again");
    assert_eq!(
        std::fs::read_to_string(&file).expect("the file"),
        leader_of("Control+Alt+KeyL"),
        "a refused write changed the configuration"
    );
    keys::written(&found, &proposed).expect("written over what a cut write left");
    assert!(
        !beside.exists(),
        "the file written through was left beside it"
    );
    assert_eq!(
        std::fs::read_to_string(&file).expect("the file"),
        leader_of("Control+Alt+KeyK")
    );
    assert!(nc_platform::testing::is_private(&file));
}
