//! A plugin's manifest: who it is, what it asks for, and what it lowers.
//!
//! **`plugin.toml`, read against the catalog.** A manifest names its plugin's
//! `id` and the `namespace` every command and topic of it starts with, the
//! `capabilities` it asks for — each a name `catalog/capabilities.toml`
//! declares, which is what makes `NOCTUA.md §11.3`'s "capabilities are granted
//! handles" a list the kernel reads rather than a convention — and, for the two
//! capabilities that need one, what it may reach: the domains `net` may fetch
//! from, and the topics `events.subscribe` may hear. A manifest may lower four
//! of the catalog's bounds for its plugin and raise none (`limits.rs`).
//!
//! **Every field is the kernel's, and a field it does not know is refused.**
//! The shape is `deny_unknown_fields` throughout, so a misspelt `capabilites`
//! is a refused manifest rather than a plugin silently granted nothing — and
//! one that writes a `version` is refused too, since the version is the
//! workspace's (`cargo xtask release`). Every problem of a manifest that has
//! the shape is reported, not the first, each with `[NC-PLUG-0010]`; one that
//! does not — TOML that does not parse, a field the kernel does not know, a
//! value of the wrong kind — is the one problem the parser stopped at, since
//! nothing past it was read. This said *every problem is reported* of both.
//!
//! **A domain is a name and nothing else.** Lower-case DNS labels of letters,
//! digits and inner hyphens — punycode's `xn--` included — at least two of
//! them, with one `*.` allowed in front for every name below a domain. No
//! scheme, port, path, user, trailing dot or IP literal — in any spelling a
//! URL parser reads as an address, `0x7f.0x1` as much as `127.0.0.1` — and
//! not `localhost`: each of those is how a request reaches somewhere other
//! than the name it shows, which `net.fetch` refuses again at the request.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin is granted exactly what its
//! manifest names from the catalog, a network it declares by name, and limits
//! no higher than the catalog's. What it does not answer: whether a manifest
//! asks for more than its plugin needs — the author's, and the person's who
//! installs it — and what a domain serves, which a name cannot say.

use core::fmt;

use nc_catalog::{Capability, ErrorCode};
use serde::Deserialize;

use crate::capability::GrantSet;
use crate::limits::{LimitRefused, Limits, Lowering};

/// The most domains one manifest may declare.
pub const DOMAINS_MAX: usize = 32;

/// The longest a plugin's `id` may be, in bytes.
pub const ID_MAX: usize = 64;

/// The longest a plugin's `namespace` may be, in bytes.
pub const NAMESPACE_MAX: usize = 32;

/// A plugin's manifest, read and judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    id: String,
    namespace: String,
    grants: GrantSet,
    domains: Vec<String>,
    subscriptions: Vec<String>,
    limits: Limits,
}

/// One way a manifest is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ManifestProblem {
    /// It is not TOML of the manifest's shape: malformed, a field missing, or a
    /// field the kernel does not know. Carries what the parser said.
    NotAManifest(String),
    /// Its `id` is not `<words>:<words>` of lower-case letters, digits and
    /// hyphens, at most [`ID_MAX`] bytes.
    IdMalformed(String),
    /// Its `namespace` is not a lower-case word of at most [`NAMESPACE_MAX`]
    /// bytes, starting with a letter.
    NamespaceMalformed(String),
    /// It asks for a capability the catalog does not declare.
    CapabilityUnknown(String),
    /// It asks for one capability twice.
    CapabilityTwice(String),
    /// It asks for `net` and declares no `[net] domains`.
    NetWithoutDomains,
    /// It declares `[net] domains` and does not ask for `net`.
    DomainsWithoutNet,
    /// A declared domain is not a name `net.fetch` can be held to.
    DomainMalformed(String),
    /// One domain is declared twice.
    DomainTwice(String),
    /// It declares more domains than [`DOMAINS_MAX`]; carries how many.
    TooManyDomains(usize),
    /// It asks for `events.subscribe` and declares no `[events] subscribe`.
    SubscribingWithoutTopics,
    /// It declares `[events] subscribe` and does not ask for `events.subscribe`.
    TopicsWithoutSubscribing,
    /// A topic it would subscribe to is not one a plugin can publish.
    TopicMalformed(String),
    /// It raises a limit, or lowers one to zero: one of these for each limit
    /// it does.
    Limit(LimitRefused),
}

impl fmt::Display for ManifestProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAManifest(said) => write!(formatter, "it is not a manifest: {said}"),
            Self::IdMalformed(id) => write!(
                formatter,
                "the id {id:?} is not `<name>:<name>` of lower-case letters, digits and hyphens, \
                 at most {ID_MAX} bytes"
            ),
            Self::NamespaceMalformed(namespace) => write!(
                formatter,
                "the namespace {namespace:?} is not a lower-case word of at most \
                 {NAMESPACE_MAX} bytes starting with a letter"
            ),
            Self::CapabilityUnknown(name) => write!(
                formatter,
                "it asks for the capability {name:?}, which catalog/capabilities.toml does not \
                 declare"
            ),
            Self::CapabilityTwice(name) => {
                write!(formatter, "it asks for the capability {name:?} twice")
            }
            Self::NetWithoutDomains => formatter.write_str(
                "it asks for `net` and declares no `[net] domains`, so there is nowhere it may \
                 fetch from",
            ),
            Self::DomainsWithoutNet => formatter
                .write_str("it declares `[net] domains` and does not ask for the capability `net`"),
            Self::DomainMalformed(domain) => write!(
                formatter,
                "the domain {domain:?} is not lower-case DNS labels, at least two, with `*.` only \
                 in front — no scheme, port, path, address or `localhost`"
            ),
            Self::DomainTwice(domain) => {
                write!(formatter, "the domain {domain:?} is declared twice")
            }
            Self::TooManyDomains(count) => write!(
                formatter,
                "it declares {count} domains, and a manifest may declare at most {DOMAINS_MAX}"
            ),
            Self::SubscribingWithoutTopics => formatter.write_str(
                "it asks for `events.subscribe` and declares no `[events] subscribe`, so there \
                 is nothing it may hear",
            ),
            Self::TopicsWithoutSubscribing => formatter.write_str(
                "it declares `[events] subscribe` and does not ask for the capability \
                 `events.subscribe`",
            ),
            Self::TopicMalformed(topic) => write!(
                formatter,
                "the topic {topic:?} is not lower-case words joined by dots, at least two, with \
                 `.*` only at the end"
            ),
            Self::Limit(refused) => write!(formatter, "{refused}"),
        }
    }
}

/// A manifest the kernel refuses, with every problem it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    problems: Vec<ManifestProblem>,
}

impl ManifestError {
    /// Every problem found, in the order the manifest's fields are judged.
    #[must_use]
    pub fn problems(&self) -> &[ManifestProblem] {
        &self.problems
    }

    /// The code every refused manifest carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        ErrorCode::PluginManifestInvalid
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "[{}] the plugin's manifest was refused:",
            ErrorCode::PluginManifestInvalid
        )?;
        for problem in &self.problems {
            write!(formatter, " {problem};")?;
        }
        Ok(())
    }
}

impl std::error::Error for ManifestError {}

/// The manifest as written, before it is judged.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    id: String,
    namespace: String,
    capabilities: Vec<String>,
    net: Option<WrittenNet>,
    events: Option<WrittenEvents>,
    #[serde(default)]
    limits: Lowering,
}

/// `[net]` as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenNet {
    domains: Vec<String>,
}

/// `[events]` as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenEvents {
    subscribe: Vec<String>,
}

impl PluginManifest {
    /// Reads and judges a manifest.
    ///
    /// # Errors
    ///
    /// Returns every [`ManifestProblem`] a manifest of the right shape has, or
    /// the one the parser stopped at in a manifest of another, as one
    /// [`ManifestError`] carrying `[NC-PLUG-0010]`.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let written: Written = toml::from_str(text).map_err(|error| ManifestError {
            problems: vec![ManifestProblem::NotAManifest(error.message().to_owned())],
        })?;
        let mut problems = Vec::new();
        if !id_is_well_formed(&written.id) {
            problems.push(ManifestProblem::IdMalformed(written.id.clone()));
        }
        if !namespace_is_well_formed(&written.namespace) {
            problems.push(ManifestProblem::NamespaceMalformed(
                written.namespace.clone(),
            ));
        }
        let grants = granted(&written.capabilities, &mut problems);
        let domains = declared_domains(&grants, written.net, &mut problems);
        let subscriptions = declared_topics(&grants, written.events, &mut problems);
        let limits = Limits::catalog()
            .lowered(&written.limits)
            .map_err(|refused| problems.extend(refused.into_iter().map(ManifestProblem::Limit)));
        match (problems.is_empty(), limits) {
            (true, Ok(limits)) => Ok(Self {
                id: written.id,
                namespace: written.namespace,
                grants,
                domains,
                subscriptions,
                limits,
            }),
            _ => Err(ManifestError { problems }),
        }
    }

    /// The plugin's stable identity: `noctua:reference`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The namespace every command and topic of the plugin starts with.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The capabilities the plugin was granted.
    #[must_use]
    pub const fn grants(&self) -> &GrantSet {
        &self.grants
    }

    /// The domains `net.fetch` may reach for it, as declared.
    #[must_use]
    pub fn domains(&self) -> &[String] {
        &self.domains
    }

    /// The topics its `activate` may subscribe to, as declared.
    #[must_use]
    pub fn subscriptions(&self) -> &[String] {
        &self.subscriptions
    }

    /// The bounds it is held to: the catalog's, lowered where it asked.
    #[must_use]
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }
}

/// Every capability `names` asks for, each found in the catalog and asked
/// for once.
fn granted(names: &[String], problems: &mut Vec<ManifestProblem>) -> GrantSet {
    let mut held: Vec<Capability> = Vec::new();
    for name in names {
        match nc_catalog::capability(name) {
            None => problems.push(ManifestProblem::CapabilityUnknown(name.clone())),
            Some(spec) if held.contains(&spec.capability) => {
                problems.push(ManifestProblem::CapabilityTwice(name.clone()));
            }
            Some(spec) => held.push(spec.capability),
        }
    }
    GrantSet::of(held)
}

/// The domains `[net]` declares, held to `net` being granted and to the
/// grammar.
fn declared_domains(
    grants: &GrantSet,
    net: Option<WrittenNet>,
    problems: &mut Vec<ManifestProblem>,
) -> Vec<String> {
    let domains = match (grants.holds(Capability::Net), net) {
        (true, Some(net)) if !net.domains.is_empty() => net.domains,
        (true, _) => {
            problems.push(ManifestProblem::NetWithoutDomains);
            return Vec::new();
        }
        (false, Some(_)) => {
            problems.push(ManifestProblem::DomainsWithoutNet);
            return Vec::new();
        }
        (false, None) => return Vec::new(),
    };
    if domains.len() > DOMAINS_MAX {
        problems.push(ManifestProblem::TooManyDomains(domains.len()));
    }
    for (at, domain) in domains.iter().enumerate() {
        if !domain_is_well_formed(domain) {
            problems.push(ManifestProblem::DomainMalformed(domain.clone()));
        } else if domains[..at].contains(domain) {
            problems.push(ManifestProblem::DomainTwice(domain.clone()));
        }
    }
    domains
}

/// The topics `[events]` declares, held to `events.subscribe` being granted
/// and to the grammar.
fn declared_topics(
    grants: &GrantSet,
    events: Option<WrittenEvents>,
    problems: &mut Vec<ManifestProblem>,
) -> Vec<String> {
    let topics = match (grants.holds(Capability::EventsSubscribe), events) {
        (true, Some(events)) if !events.subscribe.is_empty() => events.subscribe,
        (true, _) => {
            problems.push(ManifestProblem::SubscribingWithoutTopics);
            return Vec::new();
        }
        (false, Some(_)) => {
            problems.push(ManifestProblem::TopicsWithoutSubscribing);
            return Vec::new();
        }
        (false, None) => return Vec::new(),
    };
    for topic in &topics {
        if !topic_pattern_is_well_formed(topic) {
            problems.push(ManifestProblem::TopicMalformed(topic.clone()));
        }
    }
    topics
}

/// A lower-case word: letters, digits and inner hyphens, starting with a
/// letter.
pub(crate) fn a_word(text: &str) -> bool {
    text.starts_with(|character: char| character.is_ascii_lowercase())
        && !text.ends_with('-')
        && text.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// Whether `id` is `<word>:<word>`, at most [`ID_MAX`] bytes.
fn id_is_well_formed(id: &str) -> bool {
    id.len() <= ID_MAX
        && id
            .split_once(':')
            .is_some_and(|(owner, name)| a_word(owner) && a_word(name))
}

/// Whether `namespace` is one word, at most [`NAMESPACE_MAX`] bytes.
fn namespace_is_well_formed(namespace: &str) -> bool {
    namespace.len() <= NAMESPACE_MAX && a_word(namespace)
}

/// Whether `domain` is a name `net.fetch` can be held to.
///
/// Lower-case DNS labels of one to sixty-three letters, digits and inner
/// hyphens, at least two of them, at most 253 bytes, and the last no number:
/// neither all digits nor `0x` and hexadecimal digits, a bare `0x` included.
/// That is the WHATWG URL parser's "ends in a number" rule, under which a host
/// whose last label reads as a number is an IPv4 address however it is spelt —
/// `0x7f.0x1` is `127.0.0.1` to it and to the system resolver alike — so the
/// rule is what leaves every IPv4 literal out, and no top-level domain is
/// numeric or a hexadecimal number. `*.` may stand in front, once, over at
/// least two labels, so a wildcard never covers a whole top-level domain.
fn domain_is_well_formed(domain: &str) -> bool {
    let name = domain.strip_prefix("*.").unwrap_or(domain);
    let labels: Vec<&str> = name.split('.').collect();
    let label = |label: &&str| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
    };
    name.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(label)
        && labels
            .last()
            .is_some_and(|last| !last.chars().all(|character| character.is_ascii_digit()))
        && labels.last().is_some_and(|last| {
            !last
                .strip_prefix("0x")
                .is_some_and(|hex| hex.chars().all(|digit| digit.is_ascii_hexdigit()))
        })
        && labels.last() != Some(&"localhost")
}

/// Whether `topic` is a topic, or a topic with `.*` after it.
///
/// Lower-case words joined by dots, at least two of them — a namespace and a
/// name, which is the least a plugin can publish — and `*` only as the last.
fn topic_pattern_is_well_formed(topic: &str) -> bool {
    let name = topic.strip_suffix(".*").unwrap_or(topic);
    let words: Vec<&str> = name.split('.').collect();
    (words.len() >= 2 || topic.ends_with(".*")) && words.iter().all(|word| a_word(word))
}

#[cfg(test)]
mod tests {
    use super::{ManifestProblem, PluginManifest};
    use nc_catalog::{Capability, ErrorCode};

    /// A manifest every field of which is used, and every one well formed.
    const WHOLE: &str = "id = \"noctua:media-player\"\n\
                         namespace = \"media\"\n\
                         capabilities = [\"store\", \"net\", \"events.subscribe\"]\n\
                         [net]\n\
                         domains = [\"example.com\", \"*.cdn.example.net\", \"xn--bcher-kva.example\"]\n\
                         [events]\n\
                         subscribe = [\"library.album.added\", \"library.*\"]\n\
                         [limits]\n\
                         memory_bytes = 4_194_304\n";

    fn refused(text: &str) -> Vec<ManifestProblem> {
        PluginManifest::parse(text)
            .expect_err("the manifest is refused")
            .problems()
            .to_vec()
    }

    #[test]
    fn a_manifest_using_every_field_is_read_as_written() {
        let manifest = PluginManifest::parse(WHOLE).expect("the manifest is well formed");
        assert_eq!(manifest.id(), "noctua:media-player");
        assert_eq!(manifest.namespace(), "media");
        assert_eq!(
            manifest.grants().iter().collect::<Vec<_>>(),
            [
                Capability::EventsSubscribe,
                Capability::Net,
                Capability::Store
            ]
        );
        assert_eq!(
            manifest.domains(),
            ["example.com", "*.cdn.example.net", "xn--bcher-kva.example"]
        );
        assert_eq!(
            manifest.subscriptions(),
            ["library.album.added", "library.*"]
        );
        assert_eq!(manifest.limits().memory_bytes(), 4 << 20);
        assert_eq!(
            manifest.limits().fuel_per_dispatch(),
            nc_catalog::KERNEL.fuel_per_dispatch,
            "a bound the manifest does not lower is the catalog's"
        );
    }

    #[test]
    fn the_reference_plugins_manifest_is_one_the_kernel_reads() {
        let manifest =
            PluginManifest::parse(include_str!("../../../plugins/reference/plugin.toml"))
                .expect("plugins/reference/plugin.toml is a manifest");
        assert_eq!(
            (manifest.id(), manifest.namespace()),
            ("noctua:reference", "reference")
        );
    }

    #[test]
    fn a_refusal_carries_its_code_and_every_problem() {
        let error = PluginManifest::parse(
            "id = \"Noctua\"\nnamespace = \"9lives\"\ncapabilities = [\"teleport\", \"ui\", \"ui\"]\n",
        )
        .expect_err("four problems");
        assert_eq!(error.code(), ErrorCode::PluginManifestInvalid);
        assert_eq!(
            error.problems(),
            [
                ManifestProblem::IdMalformed("Noctua".to_owned()),
                ManifestProblem::NamespaceMalformed("9lives".to_owned()),
                ManifestProblem::CapabilityUnknown("teleport".to_owned()),
                ManifestProblem::CapabilityTwice("ui".to_owned()),
            ]
        );
        let said = error.to_string();
        assert!(
            said.starts_with(&format!("[{}] ", ErrorCode::PluginManifestInvalid))
                && said.contains("\"teleport\"")
                && said.contains("\"9lives\""),
            "{said}"
        );
    }

    #[test]
    fn a_field_the_kernel_does_not_know_or_a_field_missing_is_refused() {
        for (text, naming) in [
            (
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilites = []\n",
                "capabilites",
            ),
            (
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = []\nversion = \"1.0.0\"\n",
                "version",
            ),
            ("id = \"noctua:a\"\nnamespace = \"a\"\n", "capabilities"),
            (
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = []\n[limits]\nfuel = 1\n",
                "fuel",
            ),
            (
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"net\"]\n[net]\n\
                 domains = [\"example.com\"]\nports = [443]\n",
                "ports",
            ),
            (
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"events.subscribe\"]\n\
                 [events]\nsubscribe = [\"a.b\"]\npublish = [\"a.c\"]\n",
                "publish",
            ),
        ] {
            let problems = refused(text);
            assert!(
                matches!(problems.as_slice(), [ManifestProblem::NotAManifest(said)] if said.contains(naming)),
                "{text:?}: {problems:?}"
            );
        }
    }

    #[test]
    fn an_id_or_a_namespace_outside_the_grammar_is_refused() {
        for id in [
            "reference",
            "noctua:",
            ":reference",
            "Noctua:reference",
            "noctua:reference:again",
            "noctua:ref_erence",
            "noctua:reference-",
            &format!("noctua:{}", "a".repeat(58)),
        ] {
            let text = format!("id = {id:?}\nnamespace = \"a\"\ncapabilities = []\n");
            assert_eq!(
                refused(&text),
                [ManifestProblem::IdMalformed(id.to_owned())],
                "{id}"
            );
        }
        assert!(
            PluginManifest::parse(&format!(
                "id = \"noctua:{}\"\nnamespace = \"a\"\ncapabilities = []\n",
                "a".repeat(57)
            ))
            .is_ok(),
            "an id of exactly the bound is accepted"
        );
        for namespace in [
            "",
            "Media",
            "9media",
            "media.player",
            "media-",
            &"a".repeat(33),
        ] {
            let text = format!("id = \"noctua:a\"\nnamespace = {namespace:?}\ncapabilities = []\n");
            assert_eq!(
                refused(&text),
                [ManifestProblem::NamespaceMalformed(namespace.to_owned())],
                "{namespace}"
            );
        }
        // **And the namespace's bound itself**, which nothing accepted: a review
        // planted `<` for `<=` in the length clause and every test passed,
        // since the refused list holds one byte past the bound and no row held
        // the bound.
        assert!(
            PluginManifest::parse(&format!(
                "id = \"noctua:a\"\nnamespace = \"{}\"\ncapabilities = []\n",
                "a".repeat(super::NAMESPACE_MAX)
            ))
            .is_ok(),
            "a namespace of exactly the bound is accepted"
        );
    }

    #[test]
    fn domains_go_with_net_and_net_with_domains() {
        assert_eq!(
            refused("id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"net\"]\n"),
            [ManifestProblem::NetWithoutDomains]
        );
        assert_eq!(
            refused(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"net\"]\n[net]\ndomains = []\n"
            ),
            [ManifestProblem::NetWithoutDomains]
        );
        assert_eq!(
            refused(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = []\n[net]\ndomains = [\"example.com\"]\n"
            ),
            [ManifestProblem::DomainsWithoutNet]
        );
    }

    #[test]
    fn a_domain_that_is_not_a_name_is_refused() {
        // **`localhost` under a name too**, which only the `localhost` clause
        // refuses: the bare word and its wildcard are one label, and the
        // two-label rule refuses those first. Found by its mutation operator,
        // which survived the list without this row.
        //
        // **And an address spelt in hexadecimal.** A review found the grammar
        // accepting `0x7f.0x1`, which the system resolver and the WHATWG URL
        // parser both read as `127.0.0.1` without asking DNS — as they read
        // `10.0x1` as `10.0.0.1` and `169.254.169.0xfe` as the cloud metadata
        // address — because the last label was refused only when it was all
        // digits. A bare `0x` is the number zero to the URL parser, which then
        // refuses the host whole, so `a.0x` names nothing a fetch could reach.
        for domain in [
            "localhost",
            "*.localhost",
            "printer.localhost",
            "com",
            "*.com",
            "*",
            "10.0.0.1",
            "0x7f.0x1",
            "0x7f.0x0.0x0.0x1",
            "10.0x1",
            "169.254.169.0xfe",
            "*.example.0xff",
            "a.0x",
            "[::1]",
            "example.com:8443",
            "https://example.com",
            "example.com/path",
            "user@example.com",
            "Example.com",
            "example.com.",
            "-example.com",
            "example-.com",
            "a..com",
            "*.*.example.com",
            "ex*ample.com",
            &format!("{}.com", "a".repeat(64)),
            &format!("{}.com", vec!["a".repeat(63); 4].join(".")),
            &longest_name(254),
        ] {
            let text = format!(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"net\"]\n[net]\ndomains = [{domain:?}]\n"
            );
            assert_eq!(
                refused(&text),
                [ManifestProblem::DomainMalformed(domain.to_owned())],
                "{domain}"
            );
        }
        for domain in [
            "example.com",
            "*.example.com",
            "a-b.example",
            "xn--bcher-kva.example",
            "1password.com",
            "0x7f.example",
            "example.0xg",
            &format!("{}.com", "a".repeat(63)),
            &longest_name(253),
        ] {
            let text = format!(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"net\"]\n[net]\ndomains = [{domain:?}]\n"
            );
            assert!(PluginManifest::parse(&text).is_ok(), "{domain} is a name");
        }
    }

    /// A name of `bytes` bytes, every label but the last as long as a label
    /// may be: three labels of 63 and one of what is left.
    ///
    /// **The 253-byte bound, held on both sides.** A review planted `<= 252`
    /// for `<= 253` and every test passed: the longest name refused was 259
    /// bytes, and no name accepted was longer than 67.
    fn longest_name(bytes: usize) -> String {
        let name = format!(
            "{}.{}",
            vec!["a".repeat(63); 3].join("."),
            "b".repeat(bytes - 3 * 64)
        );
        assert_eq!(name.len(), bytes);
        name
    }

    #[test]
    fn a_domain_twice_or_one_domain_too_many_is_refused() {
        assert_eq!(
            refused(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"net\"]\n[net]\n\
                 domains = [\"example.com\", \"example.org\", \"example.com\"]\n"
            ),
            [ManifestProblem::DomainTwice("example.com".to_owned())]
        );
        let names = |count: usize| {
            (0..count)
                .map(|at| format!("\"d{at}.example\""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let text = |count: usize| {
            format!(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"net\"]\n[net]\ndomains = [{}]\n",
                names(count)
            )
        };
        assert!(
            PluginManifest::parse(&text(super::DOMAINS_MAX)).is_ok(),
            "the bound itself is accepted"
        );
        assert_eq!(
            refused(&text(super::DOMAINS_MAX + 1)),
            [ManifestProblem::TooManyDomains(super::DOMAINS_MAX + 1)]
        );
    }

    #[test]
    fn topics_go_with_subscribing_and_each_is_a_topic() {
        assert_eq!(
            refused(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"events.subscribe\"]\n"
            ),
            [ManifestProblem::SubscribingWithoutTopics]
        );
        // **An empty list is no topic either**, the twin of `net`'s
        // `domains = []`: a review planted the guard away and every test of
        // this crate passed, accepting a grant that subscribes to nothing.
        assert_eq!(
            refused(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"events.subscribe\"]\n[events]\nsubscribe = []\n"
            ),
            [ManifestProblem::SubscribingWithoutTopics]
        );
        assert_eq!(
            refused(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = []\n[events]\nsubscribe = [\"a.b\"]\n"
            ),
            [ManifestProblem::TopicsWithoutSubscribing]
        );
        for topic in ["a", "a.", "A.b", "a.*.b", "*", "a.b*", "a..b", "a.b-"] {
            let text = format!(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"events.subscribe\"]\n\
                 [events]\nsubscribe = [{topic:?}]\n"
            );
            assert_eq!(
                refused(&text),
                [ManifestProblem::TopicMalformed(topic.to_owned())],
                "{topic}"
            );
        }
        for topic in ["a.b", "a.*", "library.album.added", "library.album.*"] {
            let text = format!(
                "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = [\"events.subscribe\"]\n\
                 [events]\nsubscribe = [{topic:?}]\n"
            );
            assert!(PluginManifest::parse(&text).is_ok(), "{topic} is a topic");
        }
    }

    #[test]
    fn a_limit_a_manifest_raises_is_refused_by_name() {
        let text = format!(
            "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = []\n[limits]\nfuel_per_dispatch = {}\n",
            nc_catalog::KERNEL.fuel_per_dispatch + 1
        );
        assert!(
            matches!(refused(&text).as_slice(), [ManifestProblem::Limit(raised)] if raised.limit() == "fuel_per_dispatch"),
            "{text}"
        );
    }

    /// A manifest without the shape is the one problem the parser stopped at,
    /// however many others it holds past it: what the module's header says,
    /// which said every problem was reported of this one too.
    #[test]
    fn a_manifest_without_the_shape_is_the_one_problem_the_parser_stopped_at() {
        let text = "id = \"Not An Id\"\nnamespace = \"A\"\ncapabilites = []\n";
        assert!(
            matches!(refused(text).as_slice(), [ManifestProblem::NotAManifest(_)]),
            "{:?}",
            refused(text)
        );
    }

    /// Every limit a manifest raises is a problem of its own.
    ///
    /// **A review raised two in one manifest and was told of the first:**
    /// `Limits::lowered` returned on it, while this module says every problem
    /// is reported.
    #[test]
    fn every_limit_a_manifest_raises_is_reported_not_the_first() {
        let text = format!(
            "id = \"noctua:a\"\nnamespace = \"a\"\ncapabilities = []\n[limits]\n\
             memory_bytes = {}\nfuel_per_dispatch = {}\n",
            nc_catalog::KERNEL.memory_bytes + 1,
            nc_catalog::KERNEL.fuel_per_dispatch + 1
        );
        assert_eq!(
            refused(&text)
                .iter()
                .map(|problem| match problem {
                    ManifestProblem::Limit(raised) => raised.limit(),
                    other => panic!("{other:?} is not a limit"),
                })
                .collect::<Vec<_>>(),
            ["memory_bytes", "fuel_per_dispatch"]
        );
    }
}
