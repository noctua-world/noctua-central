//! `net.fetch` reaches what the plugin's manifest declares and nothing else,
//! and what it reaches it reaches as the kernel judged it (`NOCTUA.md §11.3`,
//! acceptance criterion 8).
//!
//! **Through the real component**: every request below is the reference
//! plugin's, sent from inside the component `cargo xtask build --plugins`
//! wrote, loaded under a manifest granting `store` and `net` and declaring
//! `example.com` and `*.cdn.example.net`, and dispatched as `ana` through the
//! decision point — or, for what a callback sends, under one granting the
//! timers, the events and a unit as well, called back by the kernel's pump.
//! The plugin answers a refusal as a value, so each test reads
//! the kernel's own sentence back, and the transport is scripted and keeps
//! every request it was handed — which is how "never reaches the transport" is
//! read rather than inferred.
//!
//! **No test here reaches the network**, and none may: the gate does not
//! depend on one. What the transport a daemon sends with does on its own —
//! its configuration, the bound on a body, the resolver against loopback, the
//! environment's proxy — is held by the tests in
//! `crates/nc-kernel/src/net/ureq.rs`, which reach sockets on this machine
//! and nothing else.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nc_authz::{EntityRef, MemoryDirectory, Pdp, PrincipalKind};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::{MachineFlake, Minter};
use nc_kernel::blobs::MemoryBlobs;
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_kernel::net::{Method, Response, Scripted};
use nc_kernel::{Caller, Environment, Kernel, KernelConfig, PluginSlot, Refusal};
use nc_local::{Expect, LocalStore, Quotas, StagedWrite, UnitKind, UnitMeta, UnitRecord};
use nc_plugin_reference::behaviour::{ACTIVATION, Activation, Variant};

/// The world every policy case is about.
const WORLD: &str = include_str!("../../../policy/world.json");

/// The instant the policy cases call now, `2026-08-26T00:00:00Z`: inside the
/// grant `g.current`.
const NOW_MS: u64 = 1_787_702_400_000;

/// A clock standing at [`NOW_MS`].
#[derive(Debug, Clone)]
struct Manual {
    now: Arc<AtomicU64>,
}

impl nc_ids::Clock for Manual {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }

    fn wait_past(&self, millis: u64) {
        self.now
            .fetch_max(millis.saturating_add(1), Ordering::SeqCst);
    }
}

/// A kernel over the local database held in memory, the journal in memory,
/// the world's directory and a [`Scripted`] transport a test keeps a hand on.
#[derive(Debug)]
struct Env;

impl Environment for Env {
    type Store = LocalStore;
    type Blobs = MemoryBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = Manual;
    type Directory = MemoryDirectory;
    type Log = Arc<Captured>;
    type Transport = Arc<Scripted>;
}

/// A kernel with the reference plugin loaded, and everything a test reads
/// beside it.
struct Fixture {
    kernel: Kernel<Env>,
    store: LocalStore,
    transport: Arc<Scripted>,
    log: Arc<Captured>,
    world: MemoryDirectory,
    /// The clock the kernel reads, which a test moves for its timers.
    clock: Manual,
    /// Where the reference plugin is loaded, whose own log a test reads.
    reference: PluginSlot,
}

/// The manifest the reference plugin is loaded under: the store, the
/// network, and the two names it may reach.
const MANIFEST: &str = "id = \"noctua:reference\"\nnamespace = \"reference\"\n\
                        capabilities = [\"store\", \"net\"]\n[net]\n\
                        domains = [\"example.com\", \"*.cdn.example.net\"]\n";

/// The manifest a callback's request is sent under: the store, the network
/// with the two names a callback of the reference plugin sends to, timers, a
/// unit, and the events under its own namespace, which it subscribes to.
const CALLED_BACK: &str = "id = \"noctua:reference\"\nnamespace = \"reference\"\n\
                           capabilities = [\"store\", \"net\", \"timers\", \"commands.begin\", \
                           \"events.publish\", \"events.subscribe\"]\n[net]\n\
                           domains = [\"example.com\", \"example.org\"]\n\
                           [events]\nsubscribe = [\"reference.*\"]\n";

/// The kernel, with the reference plugin loaded under [`MANIFEST`].
async fn opened() -> Fixture {
    opened_under(MANIFEST, &[]).await
}

/// The kernel, with the reference plugin loaded under `manifest`, its store's
/// activation subscribing to `heard` when it names any.
async fn opened_under(manifest: &str, heard: &[&str]) -> Fixture {
    let pack = Pdp::embedded("family").expect("the family pack is carried");
    let world = MemoryDirectory::from_world(WORLD, &pack).expect("the world is valid");
    let directory = MemoryDirectory::from_world(WORLD, &pack)
        .expect("the world is valid")
        .with_epoch("home", 7)
        .with_grant(
            "home",
            EntityRef {
                type_name: "User".to_owned(),
                id: "ana".to_owned(),
            },
            "g.current",
        )
        .expect("the realm holds the grant");
    let store = LocalStore::in_memory_for_tests()
        .await
        .expect("the local database opens in memory");
    if !heard.is_empty() {
        let activation = Activation {
            namespace: "reference".to_owned(),
            variant: Variant::Normal,
            subscribe: heard.iter().map(|&topic| topic.to_owned()).collect(),
        };
        seeded(&store, &world, &activation.written()).await;
    }
    let clock = Manual {
        now: Arc::new(AtomicU64::new(NOW_MS)),
    };
    let log = Arc::new(Captured::default());
    let transport = Arc::new(Scripted::default());
    let keeper = world
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user")
        .id()
        .clone();
    let kernel = Kernel::open(KernelConfig {
        store: store.clone(),
        blobs: MemoryBlobs::default(),
        pdp: Ok(pack),
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
            world.realm_id("home").expect("home is a realm"),
            keeper,
            Minter::new(MachineFlake::from_value(3).expect("a flake"), clock.clone()),
        ),
        directory,
        clock: clock.clone(),
        log: Arc::clone(&log),
        transport: Arc::clone(&transport),
    })
    .await
    .expect("the kernel opens");
    let reference = kernel
        .load(
            PluginManifest::parse(manifest).expect("a manifest the kernel reads"),
            &nc_kernel::component_for_tests("reference"),
        )
        .await
        .unwrap_or_else(|refused| panic!("the reference plugin loads: {refused}"));
    Fixture {
        kernel,
        store,
        transport,
        log,
        world,
        clock,
        reference,
    }
}

/// Commits `activation` as the reference plugin's activation key in `home`,
/// as a unit of `bruno`'s nobody dispatched: not a step of `ana`, whose
/// history a test reads.
async fn seeded(store: &LocalStore, world: &MemoryDirectory, activation: &str) {
    let mut minter = Minter::new(
        MachineFlake::from_value(40).expect("a flake"),
        Manual {
            now: Arc::new(AtomicU64::new(NOW_MS)),
        },
    );
    let unit = UnitRecord {
        unit: minter.mint().expect("a unit"),
        meta: UnitMeta {
            realm: world.realm_id("home").expect("home is a realm"),
            principal: world
                .principal(PrincipalKind::User, "bruno")
                .expect("bruno is a user")
                .id()
                .clone(),
            kind: UnitKind::Step,
            label: "seeding the test".to_owned(),
            committed_at_ms: NOW_MS,
        },
        writes: vec![StagedWrite {
            plugin: "noctua:reference".to_owned(),
            key: ACTIVATION.to_owned(),
            expect: Expect::Anything,
            after: Some(activation.to_owned()),
        }],
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: Vec::new(),
    };
    let quotas = Quotas {
        entries_per_plugin: 100,
        lowered: BTreeMap::new(),
        blobs: 10,
        writes_per_unit: 10,
        notes_per_unit: 10,
    };
    store
        .commit(&unit, &quotas)
        .await
        .expect("the seed commits");
}

impl Fixture {
    /// Dispatches `command` with `args` as `ana`, who owns the note `photo`.
    async fn as_ana(&self, command: &str, args: &str) -> Result<String, Refusal> {
        let ana = self
            .world
            .principal(PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        self.kernel.dispatch(&Caller::new(ana), command, args).await
    }

    /// What `command` answered fetching `url`, with `more` among its
    /// arguments: the plugin answers a refusal as a value.
    async fn fetched(&self, command: &str, url: &str, more: Vec<(&str, Value)>) -> String {
        let mut fields = vec![("note", Value::text("photo")), ("url", Value::text(url))];
        fields.extend(more);
        self.as_ana(command, &Value::object(fields).to_canonical())
            .await
            .unwrap_or_else(|refused| panic!("`{command}` of {url} answers: {refused}"))
    }

    /// Each line of the reference plugin's own log that opens with `word`.
    fn lines_opening(&self, word: &str) -> Vec<String> {
        self.kernel
            .plugin_log(self.reference)
            .expect("the plugin is loaded")
            .lines()
            .iter()
            .map(|line| line.text().to_owned())
            .filter(|line| line.starts_with(&format!("{word} ")))
            .collect()
    }

    /// Every reason and code the kernel logged a refused fetch with.
    fn refusals_logged(&self) -> Vec<(String, String)> {
        self.log
            .named("net.refused")
            .iter()
            .map(|line| {
                (
                    line.text("reason").expect("a reason"),
                    line.text("code").expect("a code"),
                )
            })
            .collect()
    }

    /// The notes of what left the machine that the newest unit `ana`
    /// committed keeps.
    ///
    /// Its other notes, of what each command was allowed, are what an undo
    /// authorizes again.
    async fn newest_notes(&self) -> Vec<String> {
        let realm = self.world.realm_id("home").expect("home is a realm");
        let ana = self
            .world
            .principal(PrincipalKind::User, "ana")
            .expect("ana is a user");
        let newest = self
            .store
            .units(&realm, ana.id(), 1)
            .await
            .expect("the store answers")
            .into_iter()
            .next()
            .expect("a unit was committed");
        self.store
            .unit(newest.unit)
            .await
            .expect("the store answers")
            .expect("the unit is held")
            .notes
            .into_iter()
            .filter(|note| note.contains(r#""note":"left_the_machine""#))
            .collect()
    }
}

/// The field `name` of the canonical JSON object `answer`.
fn field(answer: &str, name: &str) -> Value {
    nc_canon::parse(answer)
        .unwrap_or_else(|refused| panic!("{answer} is not canonical JSON: {refused}"))
        .into_object()
        .and_then(|mut fields| fields.remove(name))
        .unwrap_or_else(|| panic!("{answer} holds no `{name}`"))
}

/// Whether `answer` is the plugin telling of a refusal in `case` with `code`.
fn refused_with(answer: &str, case: &str, code: ErrorCode) -> bool {
    field(answer, "refused")
        .into_text()
        .is_some_and(|said| said.starts_with(&format!("{case}: [{code}] ")))
}

/// The status a fetch answered with.
fn status(answer: &str) -> Value {
    field(answer, "status")
}

#[tokio::test]
async fn a_fetch_to_an_undeclared_domain_is_denied_and_never_reaches_the_transport() {
    let fixture = opened().await;
    for url in [
        "https://example.org/",
        "https://www.example.com/",
        "https://cdn.example.net/",
        "https://evilcdn.example.net/",
        "https://example.com./",
    ] {
        let answer = fixture
            .fetched("reference.abuse.fetch", url, Vec::new())
            .await;
        assert!(
            refused_with(&answer, "denied", ErrorCode::PluginDomainNotDeclared),
            "{url}: {answer}"
        );
    }
    let answer = fixture
        .fetched("reference.effect.post", "https://example.org/", Vec::new())
        .await;
    assert!(
        refused_with(&answer, "denied", ErrorCode::PluginDomainNotDeclared),
        "{answer}"
    );
    let seen = fixture.newest_notes().await;
    assert!(
        seen.is_empty(),
        "a request refused before it was sent was noted as having left: {seen:?}"
    );
    let seen = fixture.transport.asked();
    assert!(
        seen.is_empty(),
        "a request to a name the manifest does not declare reached the transport: {seen:?}"
    );
    for (url, host) in [
        ("https://example.com/", "example.com"),
        ("https://img.cdn.example.net/x", "img.cdn.example.net"),
    ] {
        let answer = fixture
            .fetched("reference.abuse.fetch", url, Vec::new())
            .await;
        assert_eq!(status(&answer), Value::Integer(200), "{url}: {answer}");
        assert_eq!(
            fixture
                .transport
                .asked()
                .last()
                .map(|asked| asked.host().to_owned()),
            Some(host.to_owned())
        );
    }
    assert_eq!(
        fixture.refusals_logged(),
        [(
            "domain".to_owned(),
            ErrorCode::PluginDomainNotDeclared.to_string()
        )],
        "six refusals, one reason, one line"
    );
}

/// Every way a URL reaches somewhere other than the name it shows is refused.
///
/// An address in every spelling the parser reads as one, a port, a scheme
/// other than `https`, a user and this machine's own name are each refused,
/// and none reaches the transport.
#[tokio::test]
async fn an_ip_literal_a_port_http_and_userinfo_are_refused() {
    let fixture = opened().await;
    for (url, reason) in [
        ("https://127.0.0.1/", "ip_literal"),
        ("https://127.1/", "ip_literal"),
        ("https://0x7f000001/", "ip_literal"),
        ("https://2130706433/", "ip_literal"),
        ("https://0177.0.0.1/", "ip_literal"),
        ("https://0x7f.1/", "ip_literal"),
        ("https://127.0.0.1./", "ip_literal"),
        ("https://%31%32%37.0.0.1/", "ip_literal"),
        ("https://\u{ff11}\u{ff12}\u{ff17}.0.0.1/", "ip_literal"),
        ("https://0/", "ip_literal"),
        ("https://[::1]/", "ip_literal"),
        ("https://[0:0:0:0:0:0:0:1]/", "ip_literal"),
        ("https://[::ffff:127.0.0.1]/", "ip_literal"),
        ("https://169.254.169.254/latest/meta-data/", "ip_literal"),
        ("https://localhost/", "localhost"),
        ("https://LOCALHOST./", "localhost"),
        ("https://a.b.localhost/", "localhost"),
        ("https://example.com:8443/", "port"),
        ("https://example.com:0/", "port"),
        ("http://example.com/", "scheme"),
        ("https://user:pass@example.com/", "userinfo"),
        ("https://user@example.com/", "userinfo"),
        ("https://:pass@example.com/", "userinfo"),
        ("https://example.com%2F@evil.test/", "userinfo"),
    ] {
        let answer = fixture
            .fetched("reference.abuse.fetch", url, Vec::new())
            .await;
        assert!(
            refused_with(&answer, "denied", ErrorCode::PluginRequestRefused),
            "{url} ({reason}): {answer}"
        );
    }
    let seen = fixture.transport.asked();
    assert!(
        seen.is_empty(),
        "a refused request reached the transport: {seen:?}"
    );
    let mut logged: Vec<String> = fixture
        .refusals_logged()
        .into_iter()
        .map(|(reason, code)| {
            assert_eq!(code, ErrorCode::PluginRequestRefused.to_string());
            reason
        })
        .collect();
    logged.sort();
    assert_eq!(
        logged,
        ["ip_literal", "localhost", "port", "scheme", "userinfo"],
        "each reason once"
    );
    // `:443` on `https` is its own port, which the parser writes away.
    let answer = fixture
        .fetched(
            "reference.abuse.fetch",
            "https://example.com:443/a",
            Vec::new(),
        )
        .await;
    assert_eq!(status(&answer), Value::Integer(200), "{answer}");
    assert_eq!(
        fixture
            .transport
            .asked()
            .iter()
            .map(|asked| asked.url().as_str().to_owned())
            .collect::<Vec<_>>(),
        ["https://example.com/a"]
    );
}

/// A redirect reaches the plugin as an answer, its `location` with it.
///
/// The kernel sends nothing more: following it is a request of the plugin's
/// own, judged again.
#[tokio::test]
async fn a_redirect_is_returned_not_followed() {
    let fixture = opened().await;
    fixture.transport.answering(Ok(Response {
        status: 302,
        headers: vec![(
            "location".to_owned(),
            "https://elsewhere.example/".to_owned(),
        )],
        body: b"moved".to_vec(),
    }));
    let answer = fixture
        .fetched("reference.abuse.fetch", "https://example.com/", Vec::new())
        .await;
    assert_eq!(status(&answer), Value::Integer(302), "{answer}");
    assert_eq!(
        field(&answer, "location"),
        Value::text("https://elsewhere.example/")
    );
    assert_eq!(field(&answer, "text"), Value::text("moved"));
    assert_eq!(
        fixture.transport.asked().len(),
        1,
        "the redirect was followed"
    );
    let followed = fixture
        .fetched(
            "reference.abuse.fetch",
            "https://elsewhere.example/",
            Vec::new(),
        )
        .await;
    assert!(
        refused_with(&followed, "denied", ErrorCode::PluginDomainNotDeclared),
        "{followed}"
    );
    assert_eq!(fixture.transport.asked().len(), 1);
}

/// An answer of exactly the catalog's bounds reaches the plugin whole, and
/// one past either bound — its body, or its headers — is refused.
#[tokio::test]
async fn a_response_past_the_bound_is_refused_and_one_exactly_at_it_is_accepted() {
    let fixture = opened().await;
    let bytes = usize::try_from(KERNEL.net_response_body_bytes).expect("fits");
    let headers = usize::try_from(KERNEL.net_header_count).expect("fits");
    let answered = |body: usize, headers: usize| {
        Ok(Response {
            status: 200,
            headers: (0..headers)
                .map(|n| (format!("x-{n}"), "v".to_owned()))
                .collect(),
            body: vec![b'x'; body],
        })
    };
    fixture.transport.answering(answered(bytes, headers));
    let whole = fixture
        .fetched("reference.abuse.fetch", "https://example.com/", Vec::new())
        .await;
    assert_eq!(
        field(&whole, "bytes"),
        Value::Integer(i64::try_from(bytes).expect("fits")),
        "{whole}"
    );
    assert_eq!(
        field(&whole, "headers"),
        Value::Integer(i64::try_from(headers).expect("fits"))
    );
    for (too_large, what) in [
        (answered(bytes + 1, 0), "a body one byte past the bound"),
        (answered(0, headers + 1), "one header past the bound"),
    ] {
        fixture.transport.answering(too_large);
        let answer = fixture
            .fetched("reference.abuse.fetch", "https://example.com/", Vec::new())
            .await;
        assert!(
            refused_with(&answer, "unavailable", ErrorCode::PluginResponseTooLarge),
            "{what}: {answer}"
        );
    }
}

/// A request's body of exactly the catalog's bound is sent, and one byte
/// more is refused before the transport sees it.
#[tokio::test]
async fn a_request_body_past_the_bound_is_refused_and_one_exactly_at_it_is_sent() {
    let fixture = opened().await;
    let bound = KERNEL.net_request_body_bytes;
    let answer = fixture
        .fetched(
            "reference.effect.post",
            "https://example.com/",
            vec![(
                "fill",
                Value::Integer(i64::try_from(bound).expect("fits") + 1),
            )],
        )
        .await;
    assert!(
        refused_with(&answer, "invalid", ErrorCode::PluginBoundReached),
        "{answer}"
    );
    assert_eq!(
        fixture.transport.asked(),
        [] as [nc_kernel::net::Request; 0]
    );
    let answer = fixture
        .fetched(
            "reference.effect.post",
            "https://example.com/",
            vec![("fill", Value::Integer(i64::try_from(bound).expect("fits")))],
        )
        .await;
    assert_eq!(status(&answer), Value::Integer(200), "{answer}");
    assert_eq!(
        fixture
            .transport
            .asked()
            .first()
            .and_then(|asked| asked.body().map(<[u8]>::len)),
        Some(usize::try_from(bound).expect("fits"))
    );
}

/// A request's head — its URL and its headers' names and values, counted
/// together — of exactly the catalog's bound is sent, and one byte more is
/// refused before the transport sees it, and logged by its headers.
///
/// **Unbounded, the head reaches what `http` cannot write**: a request the
/// kernel accepted and `http` could not write — a URL past its length, a header
/// past ureq's buffer — would be told to the plugin as a network that did not
/// answer, which may pass, and logged as nothing.
#[tokio::test]
async fn a_request_head_past_the_bound_is_refused_and_one_exactly_at_it_is_sent() {
    let fixture = opened().await;
    let bound = usize::try_from(KERNEL.net_request_head_bytes).expect("fits");
    let url = "https://example.com/";
    // The reference plugin takes a header's value of at most 8,192 bytes, so
    // the head is filled by two of them.
    let (first, second) = ("x-a", "x-b");
    let room = bound - url.len() - first.len() - second.len() - 8_192;
    assert!(
        room < 8_192,
        "two headers no longer fill the head, and the plugin carries no third here"
    );
    let headers = |extra: usize| {
        let header = |name: &str, length: usize| {
            Value::object([
                ("name", Value::text(name)),
                ("value", Value::text("v".repeat(length))),
            ])
        };
        (
            "headers",
            Value::Array(vec![header(first, 8_192), header(second, room + extra)]),
        )
    };
    let answer = fixture
        .fetched("reference.abuse.fetch", url, vec![headers(1)])
        .await;
    assert!(
        refused_with(&answer, "invalid", ErrorCode::PluginBoundReached),
        "{answer}"
    );
    let seen = fixture.transport.asked();
    assert!(
        seen.is_empty(),
        "a head past the bound reached the transport: {seen:?}"
    );
    assert_eq!(
        fixture.refusals_logged(),
        [(
            "header".to_owned(),
            ErrorCode::PluginBoundReached.to_string()
        )]
    );
    let answer = fixture
        .fetched("reference.abuse.fetch", url, vec![headers(0)])
        .await;
    assert_eq!(status(&answer), Value::Integer(200), "{answer}");
    let asked = fixture.transport.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        asked[0]
            .headers()
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>()
            + asked[0].url().as_str().len(),
        bound,
        "the head that reached the transport is not the bound"
    );
}

/// A command that only reads may `GET` and `HEAD` and nothing else, and a
/// command that changes state sends the rest, each noted in its unit as
/// having left the machine.
#[tokio::test]
async fn a_read_only_frame_may_only_get() {
    let fixture = opened().await;
    for method in Method::ALL {
        let answer = fixture
            .fetched(
                "reference.abuse.fetch",
                "https://example.com/",
                vec![("method", Value::text(method.as_str()))],
            )
            .await;
        if method.leaves_a_mark() {
            assert!(
                refused_with(&answer, "denied", ErrorCode::PluginRequestRefused),
                "{method:?}: {answer}"
            );
            assert!(
                field(&answer, "refused")
                    .into_text()
                    .is_some_and(|said| said.contains("only from a command that changes state")),
                "{answer}"
            );
        } else {
            assert_eq!(status(&answer), Value::Integer(200), "{method:?}: {answer}");
        }
    }
    assert_eq!(
        fixture
            .transport
            .asked()
            .iter()
            .map(nc_kernel::net::Request::method)
            .collect::<Vec<_>>(),
        [Method::Get, Method::Head],
        "a read sent what changes something elsewhere"
    );
    for method in Method::ALL
        .into_iter()
        .filter(|method| method.leaves_a_mark())
    {
        let answer = fixture
            .fetched(
                "reference.effect.post",
                "https://img.cdn.example.net/",
                vec![("method", Value::text(method.as_str()))],
            )
            .await;
        assert_eq!(status(&answer), Value::Integer(200), "{method:?}: {answer}");
        assert_eq!(
            fixture.newest_notes().await,
            [format!(
                r#"{{"host":"img.cdn.example.net","method":"{}","note":"left_the_machine"}}"#,
                method.as_str()
            )],
            "{method:?}: the unit does not say what left the machine"
        );
    }
    assert_eq!(fixture.transport.asked().len(), 6);
    for word in ["TRACE", "CONNECT", "post"] {
        let answer = fixture
            .fetched(
                "reference.effect.post",
                "https://example.com/",
                vec![("method", Value::text(word))],
            )
            .await;
        assert!(
            refused_with(&answer, "denied", ErrorCode::PluginRequestRefused),
            "{word}: {answer}"
        );
    }
    assert_eq!(fixture.transport.asked().len(), 6);
}

/// A header that would change where a request goes or how it is framed is
/// refused, and any other is sent as the plugin wrote it.
#[tokio::test]
async fn a_header_that_changes_where_or_how_a_request_goes_is_refused() {
    let fixture = opened().await;
    let header = |name: &str, value: &str| {
        (
            "headers",
            Value::Array(vec![Value::object([
                ("name", Value::text(name)),
                ("value", Value::text(value)),
            ])]),
        )
    };
    for name in [
        "Host",
        "Content-Length",
        "Transfer-Encoding",
        "Proxy-Authorization",
    ] {
        let answer = fixture
            .fetched(
                "reference.abuse.fetch",
                "https://example.com/",
                vec![header(name, "evil.test")],
            )
            .await;
        assert!(
            refused_with(&answer, "denied", ErrorCode::PluginRequestRefused),
            "{name}: {answer}"
        );
    }
    let answer = fixture
        .fetched(
            "reference.abuse.fetch",
            "https://example.com/",
            vec![header("X-Api-Key", "a key\r\nHost: evil.test")],
        )
        .await;
    assert!(
        refused_with(&answer, "invalid", ErrorCode::PluginRequestRefused),
        "a line break smuggled a header: {answer}"
    );
    assert_eq!(
        fixture.transport.asked(),
        [] as [nc_kernel::net::Request; 0]
    );
    let answer = fixture
        .fetched(
            "reference.abuse.fetch",
            "https://example.com/",
            vec![header("X-Api-Key", "a key")],
        )
        .await;
    assert_eq!(status(&answer), Value::Integer(200), "{answer}");
    assert_eq!(
        fixture.transport.asked()[0].headers(),
        [("X-Api-Key".to_owned(), "a key".to_owned())]
    );
}

/// A refused fetch is logged once for its reason, by the plugin's slot, the
/// reason and the code, and by nothing the plugin wrote.
#[tokio::test]
async fn a_refused_fetch_is_logged_once_for_its_reason_and_names_nothing_the_plugin_wrote() {
    let fixture = opened().await;
    for _ in 0..3 {
        fixture
            .fetched(
                "reference.abuse.fetch",
                "https://secret-name.example.org/token",
                Vec::new(),
            )
            .await;
    }
    let lines = fixture.log.named("net.refused");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        lines[0]
            .fields
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["code", "reason", "slot"]
    );
    assert_eq!(lines[0].text("slot").as_deref(), Some("0"));
    assert_eq!(lines[0].text("reason").as_deref(), Some("domain"));
    assert!(
        !format!("{lines:?}").contains("secret-name"),
        "the log holds what the plugin wrote"
    );
}

/// A timer's callback, set where a change was allowed and holding the unit it
/// began, sends a `GET` and is refused a `POST`, which never reaches the
/// transport: what leaves a mark is sent only from a command that changes
/// state, and a callback is none, whatever unit it holds.
///
/// **A unit does not make a callback a change**: were a timer's callback
/// holding one let send what leaves a mark, noted in its unit, this would fail.
#[tokio::test]
async fn a_timers_callback_holding_a_unit_may_get_and_is_refused_what_leaves_a_mark() {
    let fixture = opened_under(CALLED_BACK, &[]).await;
    for method in ["POST", "GET"] {
        let timer = Value::object([
            ("ms", Value::Integer(1_000)),
            ("note", Value::text("photo")),
            ("tag", Value::text(format!("fetch:{method}"))),
        ])
        .to_canonical();
        fixture
            .as_ana("reference.timer.set", &timer)
            .await
            .unwrap_or_else(|refused| panic!("the timer for a {method} is set: {refused}"));
    }
    fixture.clock.now.fetch_add(1_000, Ordering::SeqCst);
    let pumped = fixture.kernel.pump().await;
    assert_eq!((pumped.fired, pumped.failed), (2, 0), "{pumped:?}");
    let fetched = fixture.lines_opening("fetch");
    let refused = format!("denied: [{}] ", ErrorCode::PluginRequestRefused);
    assert_eq!(fetched.len(), 2, "{fetched:#?}");
    assert_eq!(
        fetched
            .iter()
            .filter(|line| line.contains(&refused)
                && line.contains("only from a command that changes state"))
            .count(),
        1,
        "the POST was not refused as sent from where nothing changes: {fetched:#?}"
    );
    assert_eq!(
        fixture
            .transport
            .asked()
            .iter()
            .map(nc_kernel::net::Request::method)
            .collect::<Vec<_>>(),
        [Method::Get],
        "a callback sent what changes something elsewhere"
    );
}

/// An event's callback, in a plugin granted `net` and the name it fetches,
/// sends its `GET`: a callback only reads, and a read may fetch.
#[tokio::test]
async fn an_events_callback_granted_the_network_may_get() {
    let fixture = opened_under(CALLED_BACK, &["reference.*"]).await;
    let published = Value::object([
        ("data", Value::text(r#"{"then":"fetch"}"#)),
        ("note", Value::text("photo")),
        ("topic", Value::text("reference.noted")),
    ])
    .to_canonical();
    fixture
        .as_ana("reference.event.publish", &published)
        .await
        .expect("the event is published");
    let pumped = fixture.kernel.pump().await;
    assert_eq!((pumped.delivered, pumped.failed), (1, 0), "{pumped:?}");
    let asked = fixture.transport.asked();
    assert_eq!(
        asked
            .iter()
            .map(|request| (request.method(), request.url().as_str()))
            .collect::<Vec<_>>(),
        [(Method::Get, "https://example.org/heard")],
        "{:#?}",
        fixture.lines_opening("fetch")
    );
}

/// A request the transport refused before it wrote any of it leaves no note
/// that it left the machine, so its step is no barrier to undo; one that may
/// have reached its host keeps it.
///
/// **What it holds against**: a `POST` whose name resolved to no public address
/// — dialled nowhere — kept as having left, and `kernel.undo` answering that
/// the step did one thing that left the machine, never reaching past it.
#[tokio::test]
async fn a_request_the_transport_refused_unwritten_leaves_no_note_and_bars_no_undo() {
    use nc_kernel::net::TransportError;

    for (refusal, case, code, left) in [
        (
            TransportError::PrivateAddress {
                host: "example.com".to_owned(),
            },
            "denied",
            ErrorCode::PluginRequestRefused,
            false,
        ),
        (
            TransportError::UnwritableUrl("the URL is not HTTP's".to_owned()),
            "invalid",
            ErrorCode::PluginRequestRefused,
            false,
        ),
        (
            TransportError::UnwritableHeader("the header is not HTTP's".to_owned()),
            "invalid",
            ErrorCode::PluginRequestRefused,
            false,
        ),
        (
            TransportError::Unavailable("no answer".to_owned()),
            "unavailable",
            ErrorCode::PluginNetworkUnavailable,
            true,
        ),
    ] {
        let fixture = opened().await;
        let put = Value::object([
            ("note", Value::text("photo")),
            ("text", Value::text("first")),
        ])
        .to_canonical();
        fixture
            .as_ana("reference.note.put", &put)
            .await
            .expect("a note is put");
        fixture.transport.answering(Err(refusal.clone()));
        let answer = fixture
            .fetched("reference.effect.post", "https://example.com/", Vec::new())
            .await;
        assert!(refused_with(&answer, case, code), "{refusal:?}: {answer}");
        let notes = fixture.newest_notes().await;
        let undone = fixture.as_ana("kernel.undo", "{}").await;
        if left {
            assert_eq!(
                notes,
                [r#"{"host":"example.com","method":"POST","note":"left_the_machine"}"#],
                "{refusal:?}: a request that may have reached its host was not kept"
            );
            assert!(
                undone
                    .as_ref()
                    .is_err_and(|refusal| refusal.code() == ErrorCode::KernelUndoBarrier),
                "{refusal:?}: {undone:?}"
            );
        } else {
            assert!(
                notes.is_empty(),
                "{refusal:?}: a request never written is kept as having left: {notes:?}"
            );
            assert!(undone.is_ok(), "{refusal:?}: {undone:?}");
        }
    }
}

/// A request that leaves a mark and is answered too large keeps its note, so
/// its step is a barrier to undo: the request was sent, and its host may have
/// acted on it, whatever the size of what came back.
///
/// **What `net.rs` says, held**: an answer too large was sent for. With
/// `TooLarge` taken as sent nothing, in `sent_nothing` or in the condition that
/// takes the note back, `kernel.undo` would reach past a `POST` that reached
/// its host, which this test refuses.
#[tokio::test]
async fn a_request_answered_too_large_keeps_its_note_and_bars_undo() {
    use nc_kernel::net::TransportError;

    let bytes = usize::try_from(KERNEL.net_response_body_bytes).expect("fits");
    for (answered, what) in [
        (
            Err(TransportError::TooLarge(
                "the answer was too large".to_owned(),
            )),
            "the transport refused the answer as too large",
        ),
        (
            Ok(Response {
                status: 200,
                headers: Vec::new(),
                body: vec![b'x'; bytes + 1],
            }),
            "the kernel refused the answer as too large",
        ),
    ] {
        let fixture = opened().await;
        let put = Value::object([
            ("note", Value::text("photo")),
            ("text", Value::text("first")),
        ])
        .to_canonical();
        fixture
            .as_ana("reference.note.put", &put)
            .await
            .expect("a note is put");
        fixture.transport.answering(answered);
        let answer = fixture
            .fetched("reference.effect.post", "https://example.com/", Vec::new())
            .await;
        assert!(
            refused_with(&answer, "unavailable", ErrorCode::PluginResponseTooLarge),
            "{what}: {answer}"
        );
        assert_eq!(fixture.transport.asked().len(), 1, "{what}");
        assert_eq!(
            fixture.newest_notes().await,
            [r#"{"host":"example.com","method":"POST","note":"left_the_machine"}"#],
            "{what}: a request that reached its host was not kept as having left"
        );
        let undone = fixture.as_ana("kernel.undo", "{}").await;
        assert!(
            undone
                .as_ref()
                .is_err_and(|refusal| refusal.code() == ErrorCode::KernelUndoBarrier),
            "{what}: undo reached past a request that left the machine: {undone:?}"
        );
    }
}
