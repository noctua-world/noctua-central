//! No two installs on one computer share a name (`NOCTUA.md §17.1`).
//!
//! Every channel's names are derived here for two users, at a default root and
//! under two namespaces, and compared case-folded — a pipe's name and a
//! case-insensitive file system compare them that way: one user's installs
//! share no name, two users' installs share none the whole machine shares, and
//! a relocated install shares none with a real one, whoever each runs as.
//! **Nothing here touches the disk**: every root is a path under this
//! crate's own directory that no test creates, so the names are computed and
//! never made.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;
use nc_channel::{
    Bases, Channel, ChannelError, Checkout, FORMAT, Install, Keyboard, Layout, Modifier, Namespace,
    PORT_PAIRS, PORTS_FROM, Stack, Update, layout,
};

/// A root no test writes under: absolute on every system, which is all a base
/// needs to be.
fn nowhere() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("no-test-writes-here")
}

/// The four bases under `root`.
fn bases_under(root: &Path) -> Bases {
    Bases::new(
        root.join("data"),
        root.join("config"),
        root.join("cache"),
        root.join("state"),
    )
    .expect("a root under an absolute path is a plain absolute path")
}

/// Two checkouts, written out so their stacks are the same on every run.
fn checkouts() -> [Checkout; 2] {
    [
        Checkout::parse("0123abcd").expect("eight digits"),
        Checkout::parse("89efcd45").expect("eight digits"),
    ]
}

/// Production, staging and the two checkouts.
fn channels() -> [Channel; 4] {
    let [one, other] = checkouts();
    [
        Channel::Production,
        Channel::Staging,
        Channel::Development(one),
        Channel::Development(other),
    ]
}

/// Each field's name beside what it holds.
///
/// The name is a `String`, because outside `nc-log` only a `const fn` may hand
/// back a `&'static str`, and these are built in a loop.
type Named<T> = Vec<(String, T)>;

/// The paths and the names of one layout, every field of it read.
///
/// **No `..` in the pattern**: a field `Layout` gains fails to compile here
/// until it is read into one of the two lists — a name the comparisons below
/// never see is a name two installs may share. The Leader is the one field held
/// apart differently (see [`the_leader_is_held_apart_across_the_three_channels_and_only_there`]),
/// and is read here only so the pattern is whole.
fn parts(layout: &Layout) -> (Named<PathBuf>, Named<String>) {
    let Layout {
        install,
        data,
        config,
        cache,
        state,
        log,
        runtime,
        manifest,
        plugins,
        device_key,
        device,
        world,
        mark,
        stopped,
        instance_lock,
        pid_file,
        token,
        socket,
        daemon_err,
        cli,
        daemon,
        pipe,
        mutex,
        systemd_unit,
        launchd_label,
        task,
        run_key,
        app_id,
        desktop_entry,
        tray_id,
        url_scheme,
        credential_service,
        leader: _,
        update,
        stack,
    } = layout.clone();
    let paths = vec![
        ("install".to_owned(), install),
        ("data".to_owned(), data),
        ("config".to_owned(), config),
        ("cache".to_owned(), cache),
        ("state".to_owned(), state),
        ("log".to_owned(), log),
        ("runtime".to_owned(), runtime),
        ("manifest".to_owned(), manifest),
        ("plugins".to_owned(), plugins),
        ("device_key".to_owned(), device_key),
        ("device".to_owned(), device),
        ("world".to_owned(), world),
        ("mark".to_owned(), mark),
        ("stopped".to_owned(), stopped),
        ("instance_lock".to_owned(), instance_lock),
        ("pid_file".to_owned(), pid_file),
        ("token".to_owned(), token),
        ("socket".to_owned(), socket),
        ("daemon_err".to_owned(), daemon_err),
    ];
    let mut names = vec![
        ("cli".to_owned(), cli),
        ("daemon".to_owned(), daemon),
        ("pipe".to_owned(), pipe),
        ("mutex".to_owned(), mutex),
        ("systemd_unit".to_owned(), systemd_unit),
        ("launchd_label".to_owned(), launchd_label),
        ("task".to_owned(), task),
        ("run_key".to_owned(), run_key),
        ("app_id".to_owned(), app_id),
        ("desktop_entry".to_owned(), desktop_entry),
        ("tray_id".to_owned(), tray_id),
        ("url_scheme".to_owned(), url_scheme),
        ("credential_service".to_owned(), credential_service),
    ];
    if let Some(Update { feed, signing_key }) = update {
        names.push(("feed".to_owned(), feed));
        names.push(("signing_key".to_owned(), signing_key));
    }
    if let Some(Stack {
        project,
        postgres,
        logs,
    }) = stack
    {
        names.push(("project".to_owned(), project));
        names.push(("postgres".to_owned(), format!("port {postgres}")));
        names.push(("logs".to_owned(), format!("port {logs}")));
    }
    (paths, names)
}

/// The names the whole machine shares, whoever is logged in: these alone must
/// differ between two users of one install.
const MACHINE_WIDE: [&str; 5] = ["pipe", "mutex", "systemd_unit", "launchd_label", "task"];

/// One install's names, as one user, ready to compare.
struct Derived {
    /// What it is, for a failure to name.
    label: String,
    /// Which user.
    user: &'static str,
    /// Whether a test relocated it.
    relocated: bool,
    /// Every name, paths included, case-folded.
    every: BTreeSet<String>,
    /// The names the machine shares, case-folded.
    machine_wide: BTreeSet<String>,
}

/// Every install this file derives: the four channels at a default root and
/// under each of two namespaces, each for two users.
fn derived() -> Vec<Derived> {
    let users = ["1000", "S-1-5-21-1-2-3-1001"];
    let roots = [
        (None, nowhere().join("default")),
        (
            Some(Namespace::parse("t1").expect("a namespace")),
            nowhere().join("t1"),
        ),
        (
            Some(Namespace::parse("t2").expect("a namespace")),
            nowhere().join("t2"),
        ),
    ];
    let mut all = Vec::new();
    for user in users {
        for (namespace, root) in &roots {
            for channel in channels() {
                let install = Install {
                    channel,
                    bases: bases_under(root),
                    namespace: namespace.clone(),
                    log_endpoint: None,
                };
                let (paths, names) = parts(&layout(&install, user));
                let mut every = BTreeSet::new();
                let mut machine_wide = BTreeSet::new();
                for (_, path) in paths {
                    every.insert(path.to_string_lossy().to_lowercase());
                }
                for (field, name) in names {
                    if MACHINE_WIDE.contains(&field.as_str()) {
                        machine_wide.insert(name.to_lowercase());
                    }
                    every.insert(name.to_lowercase());
                }
                all.push(Derived {
                    label: format!(
                        "{} {} for {user}",
                        match channel {
                            Channel::Development(checkout) => format!("development {checkout}"),
                            other => other.word().to_owned(),
                        },
                        namespace
                            .as_ref()
                            .map_or("at the default root", Namespace::as_str)
                    ),
                    user,
                    relocated: namespace.is_some(),
                    every,
                    machine_wide,
                });
            }
        }
    }
    all
}

/// No two installs share a name, and no test's install ever names what a real
/// one does.
///
/// **Two installs of one user share nothing at all**: they are four channels
/// and two namespaces on one person's account. **Nor does a test's install and
/// a real one, whoever each runs as**, which is what keeps a test from reaching
/// a developer's daemon. Two users' real installs may share what each keeps in
/// a home and a `PATH` of their own — the CLI's name, the application identity —
/// and nothing the whole machine shares.
#[test]
fn no_two_installs_share_a_name_and_no_test_install_meets_a_real_one() {
    let all = derived();
    assert_eq!(all.len(), 24, "two users, three roots, four channels");
    let mut compared = 0_usize;
    for (at, one) in all.iter().enumerate() {
        assert!(
            one.every.len() > 20 && one.machine_wide.len() == MACHINE_WIDE.len(),
            "{} derived {} name(s), {} of them machine-wide",
            one.label,
            one.every.len(),
            one.machine_wide.len()
        );
        for other in &all[at + 1..] {
            let every = one.user == other.user || one.relocated != other.relocated;
            let (left, right, what) = if every {
                (&one.every, &other.every, "a name")
            } else {
                (
                    &one.machine_wide,
                    &other.machine_wide,
                    "a machine-wide name",
                )
            };
            let shared: Vec<&String> = left.intersection(right).collect();
            assert!(
                shared.is_empty(),
                "{} and {} share {what}: {shared:?}",
                one.label,
                other.label
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 24 * 23 / 2);
}

/// Each name is spelled as the crate's header writes it.
///
/// **These are names a system keeps**: a unit registered, a pipe a client
/// opens, a label launchd holds, a volume compose made. A spelling that moved
/// leaves the old one registered beside the new, so the spellings are written
/// out here rather than derived from the code under test.
#[test]
fn each_name_is_spelled_as_the_crate_header_writes_it() {
    let [one, _] = checkouts();
    let root = nowhere().join("default");
    let development = layout(
        &Install {
            channel: Channel::Development(one),
            bases: bases_under(&root),
            namespace: None,
            log_endpoint: None,
        },
        "1000",
    );
    let leaf = "noctua-dev-0123abcd";
    let state = root.join("state").join(leaf);
    let (paths, names) = parts(&development);
    let expected_paths = [
        ("install", state.join("bin")),
        ("data", root.join("data").join(leaf)),
        ("config", root.join("config").join(leaf)),
        ("cache", root.join("cache").join(leaf)),
        ("state", state.clone()),
        ("log", state.join("log")),
        ("runtime", state.join("run")),
        ("manifest", state.join("bin").join("install.json")),
        ("plugins", root.join("data").join(leaf).join("plugins")),
        (
            "device_key",
            root.join("data").join(leaf).join("device.key"),
        ),
        ("device", root.join("data").join(leaf).join("device.json")),
        ("world", root.join("config").join(leaf).join("world.json")),
        ("mark", state.join("ids.mark")),
        ("stopped", state.join("stopped")),
        ("instance_lock", state.join("instance.lock")),
        ("pid_file", state.join("run").join("daemon.pid")),
        ("token", state.join("run").join("token")),
        ("socket", state.join("run").join("s")),
        ("daemon_err", state.join("log").join("daemon.err")),
    ];
    assert_eq!(
        paths,
        expected_paths.map(|(field, path)| (field.to_owned(), path))
    );
    let application = "io.github.noctua_world.noctua_dev_0123abcd";
    let expected_names: Vec<(&str, String)> = vec![
        ("cli", leaf.to_owned()),
        ("daemon", format!("{leaf}-daemon")),
        ("pipe", format!(r"\\.\pipe\{leaf}.1000")),
        ("mutex", format!(r"Local\{leaf}.1000")),
        ("systemd_unit", format!("{leaf}.1000.service")),
        ("launchd_label", format!("{application}.daemon.1000")),
        ("task", format!(r"\{leaf}.1000")),
        ("run_key", leaf.to_owned()),
        ("app_id", application.to_owned()),
        ("desktop_entry", format!("{application}.desktop")),
        ("tray_id", format!("{application}.tray")),
        ("url_scheme", leaf.to_owned()),
        ("credential_service", format!("{application}.credentials")),
        ("project", "noctua-central-0123abcd".to_owned()),
        (
            "postgres",
            format!("port {}", 20_000 + 2 * (0x0123_abcd % 6_383)),
        ),
        (
            "logs",
            format!("port {}", 20_000 + 2 * (0x0123_abcd % 6_383) + 1),
        ),
    ];
    assert_eq!(
        names,
        expected_names
            .into_iter()
            .map(|(field, name)| (field.to_owned(), name))
            .collect::<Vec<_>>()
    );
}

/// A relocated install carries its namespace after its channel's leaf, and a
/// published channel names its feed and its key, as the crate's header writes
/// them.
#[test]
fn a_relocated_leaf_and_a_published_feed_are_spelled_as_the_header_writes_them() {
    let root = nowhere().join("default");
    let relocated = layout(
        &Install {
            channel: Channel::Production,
            bases: bases_under(&root),
            namespace: Some(Namespace::parse("t1").expect("a namespace")),
            log_endpoint: None,
        },
        "S-1-5-21-1-2-3-1001",
    );
    assert_eq!(relocated.cli, "noctua-t1");
    assert_eq!(relocated.pipe, r"\\.\pipe\noctua-t1.S-1-5-21-1-2-3-1001");
    assert_eq!(relocated.app_id, "io.github.noctua_world.noctua_t1");
    let staging = layout(
        &Install {
            channel: Channel::Staging,
            bases: bases_under(&root),
            namespace: None,
            log_endpoint: None,
        },
        "1000",
    );
    assert_eq!(
        staging.update,
        Some(Update {
            feed: "noctua-staging-updates".to_owned(),
            signing_key: "noctua-staging-release".to_owned(),
        })
    );
}

/// Every path a relocated install derives lies under the root it was given.
#[test]
fn every_path_a_relocated_install_derives_lies_under_its_root() {
    let root = nowhere().join("t7");
    for channel in channels() {
        let install = Install {
            channel,
            bases: bases_under(&root),
            namespace: Some(Namespace::parse("t7").expect("a namespace")),
            log_endpoint: None,
        };
        let (paths, _) = parts(&layout(&install, "1000"));
        assert_eq!(paths.len(), 19);
        for (field, path) in paths {
            assert!(
                path.starts_with(&root),
                "{} put its {field} at {}, outside {}",
                channel.word(),
                path.display(),
                root.display()
            );
        }
    }
}

/// Each channel's default Leader is its own, and two checkouts share theirs.
///
/// **Apart across the three channels, and only there** (`NOCTUA.md §12.8`):
/// the default is one chord per channel, so a second checkout keeps the
/// development chord until one is configured, and a relocated install keeps
/// its channel's.
#[test]
fn the_leader_is_held_apart_across_the_three_channels_and_only_there() {
    let leader_of = |channel: Channel, namespace: Option<&str>| {
        let install = Install {
            channel,
            bases: bases_under(&nowhere()),
            namespace: namespace.map(|text| Namespace::parse(text).expect("a namespace")),
            log_endpoint: None,
        };
        layout(&install, "1000").leader
    };
    let [one, other] = checkouts();
    let production = leader_of(Channel::Production, None);
    let staging = leader_of(Channel::Staging, None);
    let development = leader_of(Channel::Development(one), None);
    assert_ne!(production, staging);
    assert_ne!(production, development);
    assert_ne!(staging, development);
    assert_eq!(development, leader_of(Channel::Development(other), None));
    assert_eq!(production, leader_of(Channel::Production, Some("t1")));
    assert_eq!(
        development,
        leader_of(Channel::Development(one), Some("t1"))
    );
    // **The chords `§12.8`'s table writes**, each keyboard's labels for the
    // same keys.
    let written = |leader: nc_channel::Leader| {
        [Keyboard::Linux, Keyboard::Windows, Keyboard::Mac]
            .map(|keyboard| leader.written_for(keyboard))
    };
    assert_eq!(
        written(production),
        ["Alt+Space", "Alt+Space", "Option+Space"]
    );
    assert_eq!(
        written(staging),
        ["Ctrl+Alt+Space", "Ctrl+Alt+Space", "Control+Option+Space"]
    );
    assert_eq!(
        written(development),
        ["Alt+Super+Space", "Alt+Win+Space", "Option+Command+Space"]
    );
    assert_eq!(development.modifiers(), [Modifier::Alt, Modifier::Super]);
}

/// The checkout is the digest it always was, of its canonicalized path.
///
/// **Frozen.** The same digest names the checkout's compose project, so a
/// number that moved would strand every developer's database volumes. The
/// first two paths do not exist, so they are hashed as written — on every
/// system, since neither is a path any of them has.
#[test]
fn the_checkout_is_the_digest_it_always_was() {
    assert_eq!(
        Checkout::of(Path::new("/nonexistent/9/noctua-central")).to_string(),
        "05d42f30"
    );
    // **And the rendering, not only the digest**: `{:x}` would drop that
    // leading zero, and rename the project of one checkout path in sixteen.
    assert_eq!(
        Checkout::of(Path::new("/nonexistent/9/noctua-central"))
            .stack()
            .project,
        "noctua-central-05d42f30"
    );
    assert_eq!(Checkout::of(Path::new("")).to_string(), "811c9dc5");
    // **A path that exists, so the canonicalizing branch is the one under
    // test**: the crate's own directory, and the same reached by going into
    // its sources and back up. Without the canonicalization the two texts
    // differ, and one checkout would be two stacks.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        Checkout::of(here),
        Checkout::of(&here.join("src").join(".."))
    );
    assert_ne!(Checkout::of(here), Checkout::of(&here.join("src")));
    // A checkout reads back as itself, and as nothing else.
    for checkout in checkouts() {
        assert_eq!(Checkout::parse(&checkout.to_string()), Ok(checkout));
    }
    for refused in [
        "0123ABCD",
        "0123abc",
        "0123abcde",
        "+123abcd",
        "0123abcg",
        "",
    ] {
        let refusal = Checkout::parse(refused).expect_err(refused);
        assert_eq!(
            refusal.code(),
            ErrorCode::ChannelCheckoutMalformed,
            "{refused}"
        );
    }
}

/// A checkout's stack takes a pair of ports inside its block, and no two
/// checkouts' pairs overlap unless their digests meet in it.
#[test]
fn the_stack_ports_stay_in_their_block_and_apart() {
    let mut taken = BTreeSet::new();
    for pair in 0..u32::from(PORT_PAIRS) {
        let checkout = Checkout::parse(&format!("{pair:08x}")).expect("eight digits");
        let Stack {
            project,
            postgres,
            logs,
        } = checkout.stack();
        assert_eq!(project, format!("noctua-central-{pair:08x}"));
        // **The formula itself, because it is a stored value**: a developer's
        // stack is reached on these ports, and a formula that moved would
        // point every checkout's tools at a port nothing listens on.
        assert_eq!(u32::from(postgres), u32::from(PORTS_FROM) + 2 * pair);
        assert_eq!(logs, postgres + 1);
        assert!(
            (20_000..=32_767).contains(&postgres) && (20_000..=32_767).contains(&logs),
            "{checkout} took {postgres} and {logs}, outside the block"
        );
        assert!(taken.insert(postgres) && taken.insert(logs), "{checkout}");
    }
    assert_eq!(taken.len(), 2 * usize::from(PORT_PAIRS));
    // Never PostgreSQL's or the log store's default, which a stack started
    // before this block existed still holds.
    assert!(!taken.contains(&5432) && !taken.contains(&9428));
    // The block wraps: the digest past its last pair takes its first.
    let wrapped = Checkout::parse(&format!("{:08x}", u32::from(PORT_PAIRS)))
        .expect("eight digits")
        .stack();
    assert_eq!(wrapped.postgres, PORTS_FROM);
    let highest = Checkout::parse("ffffffff").expect("eight digits").stack();
    assert!(taken.contains(&highest.postgres) && taken.contains(&highest.logs));
}

/// Only a published channel follows an update feed, only a checkout has a
/// stack, and a relocated install has neither.
#[test]
fn only_a_real_published_channel_updates_and_only_a_real_checkout_has_a_stack() {
    let [one, _] = checkouts();
    let at = |channel: Channel, namespace: Option<&str>| {
        layout(
            &Install {
                channel,
                bases: bases_under(&nowhere()),
                namespace: namespace.map(|text| Namespace::parse(text).expect("a namespace")),
                log_endpoint: None,
            },
            "1000",
        )
    };
    let production = at(Channel::Production, None);
    let staging = at(Channel::Staging, None);
    assert!(production.update.is_some() && staging.update.is_some());
    assert_ne!(production.update, staging.update);
    assert!(production.stack.is_none() && staging.stack.is_none());
    let development = at(Channel::Development(one), None);
    assert!(development.update.is_none());
    assert_eq!(development.stack, Some(one.stack()));
    for channel in channels() {
        let relocated = at(channel, Some("t1"));
        assert!(
            relocated.update.is_none() && relocated.stack.is_none(),
            "a relocated {} install follows a feed or names a stack",
            channel.word()
        );
    }
}

/// What an installer writes reads back as the install it described.
#[test]
fn a_manifest_reads_back_as_the_install_that_wrote_it() {
    for channel in channels() {
        for namespace in [None, Some("t1")] {
            for log_endpoint in [None, Some("http://127.0.0.1:9428")] {
                let install = Install {
                    channel,
                    bases: bases_under(&nowhere()),
                    namespace: namespace.map(|text| Namespace::parse(text).expect("a namespace")),
                    log_endpoint: log_endpoint.map(str::to_owned),
                };
                let written = install.to_canonical();
                assert_eq!(Install::parse(&written), Ok(install), "{written}");
                assert!(!written.contains("null"), "{written}");
            }
        }
    }
}

/// A base's path under the root no test writes under, as a manifest writes it.
fn base_text(name: &str) -> String {
    nc_canon::Value::text(nowhere().join(name).to_string_lossy()).to_canonical()
}

/// A manifest of a real production install, with `fields` after its format.
fn manifest(fields: &str) -> String {
    format!(
        "{{\"bases\":{{\"cache\":{},\"config\":{},\"data\":{},\"state\":{}}},\"format\":{FORMAT}{fields}}}",
        base_text("cache"),
        base_text("config"),
        base_text("data"),
        base_text("state")
    )
}

/// Each case refused with its code, in the sentence every refusal opens with —
/// and the manifest every case was made from read, so a refusal is the
/// defect's and not the base text's.
fn each_is_refused_with_its_code(cases: Vec<(&str, String, ErrorCode)>) {
    assert!(Install::parse(&manifest(r#","channel":"production","namespace":"t1""#)).is_ok());
    for (what, text, code) in cases {
        let refusal = Install::parse(&text).expect_err(what);
        assert_eq!(refusal.code(), code, "{what}: {refusal}");
        assert!(
            refusal.to_string().starts_with(&format!(
                "[{}] the install manifest was refused: ",
                code.code()
            )),
            "{what}: {refusal}"
        );
    }
}

/// A manifest this build cannot read as one is refused as unreadable.
#[test]
fn a_manifest_this_build_cannot_read_is_refused_as_unreadable() {
    let production = manifest(r#","channel":"production""#);
    let unreadable = ErrorCode::ChannelManifestUnreadable;
    each_is_refused_with_its_code(vec![
        (
            "an unknown key",
            manifest(r#","channel":"production","port":1"#),
            unreadable,
        ),
        (
            "an unknown key under the bases",
            production.replacen("\"bases\":{", "\"bases\":{\"runtime\":\"x\",", 1),
            unreadable,
        ),
        ("not an object", "[]".to_owned(), unreadable),
        ("not JSON", "{".to_owned(), unreadable),
        (
            "a format this build does not read",
            production.replace(&format!("\"format\":{FORMAT}"), "\"format\":2"),
            unreadable,
        ),
        (
            "a channel written as a number",
            manifest(r#","channel":1"#),
            unreadable,
        ),
        (
            "a namespace written as null",
            manifest(r#","channel":"production","namespace":null"#),
            unreadable,
        ),
        ("no channel", manifest(""), unreadable),
        (
            "a missing base",
            production.replacen(&format!("\"cache\":{},", base_text("cache")), "", 1),
            unreadable,
        ),
    ]);
}

/// A manifest holding a wrong value is refused with that value's own code.
#[test]
fn a_manifest_holding_a_wrong_value_is_refused_with_that_values_code() {
    let production = manifest(r#","channel":"production""#);
    let relative = nc_canon::Value::text("relative/data").to_canonical();
    let climbing =
        nc_canon::Value::text(nowhere().join("..").join("data").to_string_lossy()).to_canonical();
    each_is_refused_with_its_code(vec![
        (
            "a wrong channel",
            manifest(r#","channel":"nightly""#),
            ErrorCode::ChannelUnknown,
        ),
        (
            "a channel in another case",
            manifest(r#","channel":"Production""#),
            ErrorCode::ChannelUnknown,
        ),
        (
            "a checkout on a non-development channel",
            manifest(r#","channel":"staging","checkout":"0123abcd""#),
            ErrorCode::ChannelCheckoutMalformed,
        ),
        (
            "a development install naming no checkout",
            manifest(r#","channel":"development""#),
            ErrorCode::ChannelCheckoutMalformed,
        ),
        (
            "a malformed checkout",
            manifest(r#","channel":"development","checkout":"0123ABCD""#),
            ErrorCode::ChannelCheckoutMalformed,
        ),
        (
            "a relative base",
            production.replacen(&base_text("data"), &relative, 1),
            ErrorCode::ChannelBaseMalformed,
        ),
        (
            "a base that climbs",
            production.replacen(&base_text("data"), &climbing, 1),
            ErrorCode::ChannelBaseMalformed,
        ),
        (
            "a namespace with a separator",
            manifest(r#","channel":"production","namespace":"t/1""#),
            ErrorCode::ChannelNamespaceMalformed,
        ),
        (
            "a namespace with a hyphen",
            manifest(r#","channel":"production","namespace":"t-1""#),
            ErrorCode::ChannelNamespaceMalformed,
        ),
        (
            "a namespace that is a channel's word",
            manifest(r#","channel":"production","namespace":"staging""#),
            ErrorCode::ChannelNamespaceMalformed,
        ),
    ]);
}

/// A refusal quotes the manifest's own text escaped and cut, so no manifest
/// forges a line of the diagnostic that refuses it.
///
/// **Every place a refusal quotes the manifest is asked**, because each is a
/// `format!` of its own, and one that wrote the text as it came would reopen
/// this for its own field alone: a key, a channel, a checkout, a base and a
/// namespace, each holding what ends a line and what ends a field. So is the
/// key written twice, which `nc-canon` refuses before this crate reads a key,
/// and whose refusal this crate's sentence carries.
#[test]
fn a_refusal_quotes_the_manifest_escaped_and_cut() {
    // A line feed and a line separator, which end a line, and a quote and a
    // backslash, which end a field — each written out as its code point.
    let hostile = "\n\u{2028}\"\\";
    let written = "U+000AU+2028U+0022U+005C";
    let text = |value: &str| nc_canon::Value::text(value).to_canonical();
    let cases = [
        (
            "a key",
            manifest(&format!(
                r#","channel":"production",{}:1"#,
                text(&format!("k{hostile}"))
            )),
        ),
        (
            "a channel",
            manifest(&format!(r#","channel":{}"#, text(&format!("c{hostile}")))),
        ),
        (
            "a checkout",
            manifest(&format!(
                r#","channel":"development","checkout":{}"#,
                text(&format!("0{hostile}"))
            )),
        ),
        (
            "a base",
            manifest(r#","channel":"production""#).replacen(
                &base_text("data"),
                &text(&format!("d{hostile}")),
                1,
            ),
        ),
        (
            "a namespace",
            manifest(&format!(
                r#","channel":"production","namespace":{}"#,
                text(&format!("n{hostile}"))
            )),
        ),
        (
            "a key written twice",
            manifest(&format!(
                r#","channel":"production",{key}:1,{key}:2"#,
                key = text(&format!("k{hostile}"))
            )),
        ),
    ];
    for (what, written_down) in cases {
        let refusal = Install::parse(&written_down).expect_err(what).to_string();
        assert!(
            !refusal.chars().any(nc_canon::is_unsafe_in_a_message),
            "{what}: {refusal:?}"
        );
        assert!(refusal.contains(written), "{what}: {refusal:?}");
    }
    // **Cut at sixty-four characters, counted as characters and not bytes**:
    // a channel of a hundred, opening with the four above, is quoted as its
    // first sixty-four with each of the four written out and `…` after them,
    // and one of exactly sixty-four is quoted whole.
    let refused = |channel: &str| {
        Install::parse(&manifest(&format!(r#","channel":{}"#, text(channel))))
            .expect_err(channel)
            .to_string()
    };
    let long = refused(&format!("{hostile}{}", "é".repeat(96)));
    assert!(
        long.contains(&format!("`{written}{}…`", "é".repeat(60))),
        "{long:?}"
    );
    let whole = "é".repeat(64);
    assert!(refused(&whole).contains(&format!("`{whole}`")), "{whole}");
}

/// A namespace is a short word of lower-case letters and digits holding a
/// digit, and nothing else.
#[test]
fn a_namespace_is_short_lower_case_and_holds_a_digit() {
    for accepted in ["t1", "1", "0123456789abcdef", "a9z"] {
        assert_eq!(
            Namespace::parse(accepted).map(|namespace| namespace.as_str().to_owned()),
            Ok(accepted.to_owned())
        );
    }
    for refused in [
        "",
        "t",
        "staging",
        "daemon",
        "T1",
        "t.1",
        "t_1",
        "t 1",
        r"t\1",
        "0123456789abcdef0",
        "t\n1",
        "é1",
    ] {
        assert!(
            matches!(Namespace::parse(refused), Err(ChannelError::Namespace(_))),
            "{refused:?}"
        );
    }
}

/// A base is a plain absolute path, as its own components spell it.
#[test]
fn a_base_is_a_plain_absolute_path() {
    let root = nowhere();
    let good = root.join("data");
    let refused = [
        PathBuf::from("relative/data"),
        PathBuf::new(),
        root.join("..").join("data"),
        PathBuf::from(format!(
            "{}{}",
            root.join(".").display(),
            std::path::MAIN_SEPARATOR
        )),
        PathBuf::from(format!("{}{}", good.display(), std::path::MAIN_SEPARATOR)),
        root.join("da\nta"),
    ];
    for base in refused {
        let refusal = Bases::new(base.clone(), good.clone(), good.clone(), good.clone())
            .expect_err(&base.display().to_string());
        assert_eq!(
            refusal.code(),
            ErrorCode::ChannelBaseMalformed,
            "{}",
            base.display()
        );
        // Whichever of the four it is, the refusal names it.
        let as_state = Bases::new(good.clone(), good.clone(), good.clone(), base.clone())
            .expect_err(&base.display().to_string());
        assert!(
            as_state.to_string().contains("the state base"),
            "{as_state}"
        );
    }
    assert!(Bases::new(good.clone(), good.clone(), good.clone(), good).is_ok());
}
