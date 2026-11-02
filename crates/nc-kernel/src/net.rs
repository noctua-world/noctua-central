//! `net.fetch`: what a plugin may ask of the network, judged before anything
//! is sent, and the [`Transport`] that sends it (`NOCTUA.md §11.3`).
//!
//! **A request is judged whole before a transport sees it**, in this order,
//! and the first rule it breaks is the refusal it gets. The URL must parse as
//! WHATWG parses one, and be `https`. Its host must be a name: every spelling
//! of an address — `127.1`, `0x7f000001`, `2130706433`, full-width digits, a
//! bracketed IPv6 — parses to an address and is refused, and so is
//! `localhost` and every name under it, with or without a trailing dot; and a
//! DNS name — labels of one to sixty-three letters, digits and inner hyphens,
//! none empty, at most 253 bytes in all — because a declaration with `*.`
//! holds only its suffix, and WHATWG keeps characters in a host — `"`, `{`,
//! `}`, a backtick — that no DNS name has and HTTP cannot write: a reviewer
//! found such a name judged and then refused by the transport. It may carry
//! no user and no password, and no port: `:443` on `https` is the default,
//! which the parser removes, and every other is refused. The name must be one
//! the plugin's manifest declares — the name itself, or one below a `*.`
//! declaration, never the declaration's own apex — compared as the parser
//! writes it, lower-case and in its IDNA form, which is the form a manifest is
//! held to (`crate::manifest`). Then the method: `GET` and `HEAD` wherever the
//! plugin is running, and `POST`, `PUT`, `PATCH` and `DELETE` only
//! in a command that changes state, whose unit records the request as having
//! left the machine before it is sent; any other word, any other spelling, is
//! refused. Then the headers — at most `net_header_count`, each a name of
//! HTTP's token characters and a value of visible ASCII, never one the
//! transport writes itself or one that reaches past the request
//! (`Host`, `Content-Length`, `Transfer-Encoding`, `Connection`, `Keep-Alive`,
//! `Upgrade`, `TE`, `Trailer`, `Expect`, every `Proxy-`). Then the request's
//! head — the URL as it is sent, without its fragment, which is the plugin's
//! own and is never sent, and the headers' names and values, counted together
//! — which is at most `net_request_head_bytes`; and then the body, which a
//! `GET` and a `HEAD` carry none of and no request carries more than
//! `net_request_body_bytes` of.
//!
//! **What each refusal is.** A domain the manifest does not declare is
//! `denied` with `[NC-PLUG-0012]`; every other rule above is
//! `[NC-PLUG-0013]` — `invalid` for text that is not a URL, a header or a body
//! HTTP can carry, `denied` for one it can and the kernel will not send — but
//! the three bounds, on the headers, the head and the body, which are
//! `invalid` with `[NC-PLUG-0017]`: the head by its URL when the URL alone is
//! past it, and by its headers when they take it past. A name that resolves
//! to no public address is `denied` with `[NC-PLUG-0013]` too, by the
//! transport; a request the transport's HTTP cannot write is `invalid` with
//! it, by its URL or its header, and nothing of it is sent — which the bound
//! on the head and the rule on the host keep from happening to a request
//! judged here; an answer past
//! `net_response_body_bytes` or with more than
//! `net_header_count` headers is `unavailable` with `[NC-PLUG-0014]`, held here
//! again whatever the transport bounded; and a request that did not complete —
//! no answer, a timeout, a connection that failed — is `unavailable` with
//! `[NC-PLUG-0015]`, which may pass. A refusal of the first kind is logged as
//! `net.refused`, once for each reason in each instance of a plugin, so the
//! log grows with the reasons and never with the requests.
//!
//! **A transport is blocking, and the kernel bounds it.** [`Transport::fetch`]
//! is a plain function, which the host function runs on the runtime's blocking
//! pool under the host call's deadline (`supervisor::in_time`): a transport
//! that never answers is abandoned at `host_call_deadline_ms` and the plugin
//! waiting on it killed, and the thread goes on until the transport's own
//! bounds end it. `crate::net::ureq` is the one a daemon sends with — its own
//! timeout is `net_timeout_ms`, which the catalog holds below the host call's
//! deadline. A redirect is an answer like any other: the transport follows
//! none, so a `3xx` reaches the plugin with its `location`, and a plugin that
//! follows it sends a request judged here again.
//!
//! **Which addresses are public is [`is_public`]**, which the transport's
//! resolver holds every address it would dial to: the name was judged here,
//! and what it resolves to is judged where it is dialled.
//!
//! What this answers (`NOCTUA.md §2.4`): server-side request forgery — a
//! plugin reaching this machine's loopback, a private or link-local network, a
//! tailnet or a cloud's metadata through a request, by an address in any
//! spelling, by a name that resolves to one, by a proxy the environment names
//! or by a redirect to somewhere its manifest never named; a request smuggled past the policy through a header
//! that changes how it is framed or where it goes; a request or an answer
//! larger than the catalog allows; and a request the kernel accepted and HTTP
//! cannot carry, told as a network that did not answer and asked again for
//! ever. What it does not answer: what a host a
//! plugin may reach does with what the plugin sends it — a plugin granted
//! `net` and a domain can send that domain anything it can read, which is why
//! the grant and the domain are its manifest's and a person's to accept; a
//! header that host trusts — a header is refused by the names above and
//! admitted by none, so `Forwarded`, `X-Forwarded-For`, `Via`, a cookie and
//! any other a plugin sets are sent as it set them, for the host to believe
//! or not; the owner of a declared name pointing it at a public address of
//! their choosing — **this machine's own among them**: a global IPv6 address
//! of its interfaces, every neighbour's on a network that hands them out, the
//! router's included, and a NAT's own public IPv4 address reached by hairpin
//! are public addresses to [`is_public`], which judges an address and not
//! whose it is, so a name that resolves to one is dialled, and only the
//! certificate's verification against the declared name keeps what answers
//! there from the plugin. A reviewer measured this machine's own address
//! dialled that way; what would refuse it is the machine's interfaces and
//! routes read at the moment a name resolves, which no crate of this tree
//! reads (`NOCTUA.md §2.3` makes that a question for the developer), and what
//! is done meanwhile is that the transport tells every request that did not
//! complete in one sentence, so a plugin learns nothing of which ports there
//! answer; **an address under a network-specific NAT64 prefix** (RFC 6052
//! §2.2), which a network's DNS64 may synthesise with and which, unlike the
//! well-known prefix, may carry a private IPv4 address (§3.1): [`is_public`]
//! judges it as the IPv6 address it is, so on such a network a declared name
//! whose AAAA record is that prefix and a private IPv4 address is dialled, and
//! translated into that network — what would judge it is the network's own
//! prefixes, discovered as RFC 7050 says from the AAAA records of
//! `ipv4only.arpa`, which nothing here asks; and a request that left the
//! machine inside a unit that is then let go, which is not undone — it is
//! recorded in the unit only when the unit is kept.

mod ureq;

pub use self::ureq::UreqTransport;

use core::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use nc_catalog::{ErrorCode, KERNEL};
use url::{Host, Url};

use crate::RefusalKind;
use crate::dispatch::Refusal;

/// A method a plugin may send, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// Reads what is there.
    Get,
    /// Reads what is there, without its body.
    Head,
    /// Hands the host something to act on.
    Post,
    /// Puts something where the URL says.
    Put,
    /// Changes part of what is there.
    Patch,
    /// Removes what is there.
    Delete,
}

impl Method {
    /// Every method a plugin may send, in the order HTTP lists them.
    pub const ALL: [Self; 6] = [
        Self::Get,
        Self::Head,
        Self::Post,
        Self::Put,
        Self::Patch,
        Self::Delete,
    ];

    /// The word the request line carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    /// Whether a request of this method asks the host to change something,
    /// which no unit of this machine can undo.
    #[must_use]
    pub const fn leaves_a_mark(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch | Self::Delete)
    }

    /// The method `word` names, written exactly as HTTP writes it.
    fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|method| method.as_str() == word)
    }
}

/// A request the kernel judged, as a transport is handed it.
///
/// **Only the kernel builds one**, from what a plugin asked once every rule
/// of this module held, so a transport is never handed a request nobody
/// judged; its URL is the parser's own text, which is what a transport sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    method: Method,
    url: Url,
    host: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

impl Request {
    /// What is asked.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// Where, as the parser writes it: `https`, a declared name, no user, no
    /// port and no fragment.
    #[must_use]
    pub const fn url(&self) -> &Url {
        &self.url
    }

    /// The name the request is sent to: the URL's host as the parser writes
    /// it, which the manifest declares itself or below a `*.` declaration.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The headers the plugin set, in the order it set them — which the
    /// transport a daemon sends with writes grouped by name ([`UreqTransport`]):
    /// each name's values in this order, the names in the order each first
    /// appears.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// The body, when there is one.
    #[must_use]
    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }
}

/// What a host answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status, a redirect's included.
    pub status: u16,
    /// Every header, grouped by name as [`UreqTransport`] reads them: each
    /// name's values in the order the host sent them, the names in the order
    /// each first came. HTTP gives a meaning to the order of one name's lines
    /// alone (RFC 9110 §5.3), and `http`'s map, which ureq reads an answer
    /// into, keeps no more.
    pub headers: Vec<(String, String)>,
    /// The whole body.
    pub body: Vec<u8>,
}

/// Why a transport did not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The name resolved to no address the kernel dials: every one it
    /// resolved to is outside the public internet, as [`is_public`] says.
    PrivateAddress {
        /// The name that was resolved.
        host: String,
    },
    /// The answer was larger than the kernel accepts: its head, or its body.
    TooLarge(String),
    /// The request's URL is not one the transport's HTTP can write, so nothing
    /// of the request was sent.
    UnwritableUrl(String),
    /// A header of the request is not one the transport's HTTP can write, so
    /// nothing of the request was sent.
    UnwritableHeader(String),
    /// No answer: a name that does not resolve, a connection that failed, a
    /// timeout, or a host that broke off.
    Unavailable(String),
}

impl TransportError {
    /// Whether the transport refused the request before it wrote any of it:
    /// a name that resolved to no public address, for which nothing was
    /// dialled, and a URL or a header its HTTP cannot write. **Every other
    /// refusal may follow a request that reached its host** — an answer too
    /// large was sent for, and one that did not come may have been — so the
    /// unit keeps its note of each of those as having left the machine.
    #[must_use]
    pub const fn sent_nothing(&self) -> bool {
        matches!(
            self,
            Self::PrivateAddress { .. } | Self::UnwritableUrl(_) | Self::UnwritableHeader(_)
        )
    }

    /// The registry code it is refused with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::PrivateAddress { .. } | Self::UnwritableUrl(_) | Self::UnwritableHeader(_) => {
                ErrorCode::PluginRequestRefused
            }
            Self::TooLarge(_) => ErrorCode::PluginResponseTooLarge,
            Self::Unavailable(_) => ErrorCode::PluginNetworkUnavailable,
        }
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrivateAddress { host } => write!(
                formatter,
                "[{}] {} resolves to no public address, and a plugin reaches the public \
                 internet alone",
                self.code(),
                crate::registry::quoted_up_to(host, QUOTED)
            ),
            Self::TooLarge(why)
            | Self::UnwritableUrl(why)
            | Self::UnwritableHeader(why)
            | Self::Unavailable(why) => write!(formatter, "[{}] {why}", self.code()),
        }
    }
}

impl std::error::Error for TransportError {}

/// What sends a request the kernel judged.
///
/// **Blocking.** The kernel calls it on the runtime's blocking pool, under
/// the host call's deadline, and abandons a call that passes it; what a
/// transport must do itself is dial only what [`is_public`] accepts and follow
/// no redirect, which no rule of the kernel can do for it.
pub trait Transport: fmt::Debug + Send + Sync + 'static {
    /// Sends `request` and waits for the whole answer.
    ///
    /// # Errors
    ///
    /// A [`TransportError`]: the name resolved to no public address, the
    /// request was not one its HTTP can write, the answer was larger than the
    /// transport accepts, or there was no answer.
    fn fetch(&self, request: &Request) -> Result<Response, TransportError>;
}

/// A transport shared, answering as the transport it shares: what lets a test
/// keep a hand on the transport a kernel was opened with.
impl<T: Transport> Transport for Arc<T> {
    fn fetch(&self, request: &Request) -> Result<Response, TransportError> {
        (**self).fetch(request)
    }
}

/// What a plugin asked to send, before anything about it is judged.
#[derive(Debug)]
pub(crate) struct Asked {
    /// The method's word, as the plugin wrote it.
    pub(crate) method: String,
    /// The URL, as the plugin wrote it.
    pub(crate) url: String,
    /// The headers, as the plugin wrote them.
    pub(crate) headers: Vec<(String, String)>,
    /// The body, if the plugin gave one.
    pub(crate) body: Option<Vec<u8>>,
}

/// Why a fetch was refused, as `net.refused` logs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Reason {
    /// Text that is not a URL, one that names no host, one past the bound on
    /// a request's head, or one the transport's HTTP cannot write.
    Url,
    /// A scheme other than `https`.
    Scheme,
    /// An address, in any spelling, where a name belongs.
    IpLiteral,
    /// `localhost`, or a name under it.
    Localhost,
    /// A user or a password in the URL.
    Userinfo,
    /// A port in the URL.
    Port,
    /// A name the manifest does not declare.
    Domain,
    /// A method the plugin may not send, or not from where it is running.
    Method,
    /// A header the kernel does not send, more headers than it sends, headers
    /// carrying the request's head past its bound, or one the transport's HTTP
    /// refuses to write.
    Header,
    /// A body on a method that carries none, or one past its bound.
    Body,
    /// A name that resolved to no public address.
    PrivateAddress,
    /// An answer past its bounds.
    TooLarge,
}

impl Reason {
    /// Every reason, in the order declared — which a test reads the
    /// declaration for, since the compiler holds no list to an enum.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 12] = [
        Self::Url,
        Self::Scheme,
        Self::IpLiteral,
        Self::Localhost,
        Self::Userinfo,
        Self::Port,
        Self::Domain,
        Self::Method,
        Self::Header,
        Self::Body,
        Self::PrivateAddress,
        Self::TooLarge,
    ];

    /// The variant `net.refused` logs it as.
    ///
    /// **Written out, one literal each**, because a log field is a variant
    /// the source names. Each is the reason's own name in snake case and a
    /// word the comment on the event in `catalog/log-events.toml`, the
    /// registry of what the log may carry, names: a test holds every arm to
    /// each.
    pub(crate) fn variant(self) -> nc_log::Variant {
        match self {
            Self::Url => nc_log::variant!("url"),
            Self::Scheme => nc_log::variant!("scheme"),
            Self::IpLiteral => nc_log::variant!("ip_literal"),
            Self::Localhost => nc_log::variant!("localhost"),
            Self::Userinfo => nc_log::variant!("userinfo"),
            Self::Port => nc_log::variant!("port"),
            Self::Domain => nc_log::variant!("domain"),
            Self::Method => nc_log::variant!("method"),
            Self::Header => nc_log::variant!("header"),
            Self::Body => nc_log::variant!("body"),
            Self::PrivateAddress => nc_log::variant!("private_address"),
            Self::TooLarge => nc_log::variant!("too_large"),
        }
    }
}

/// A fetch refused: what the plugin is told, and the reason it is logged
/// under when it is one `net.refused` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    /// Why, as the log names it; none for a request that did not complete,
    /// which is the network's and not a refusal.
    pub(crate) reason: Option<Reason>,
    /// What the plugin is told.
    pub(crate) refusal: Refusal,
}

/// A refusal for `reason`, in the world's `kind`, with `code`, said in
/// the words of `sentence`.
fn refused(reason: Reason, kind: RefusalKind, code: ErrorCode, sentence: String) -> Refused {
    Refused {
        reason: Some(reason),
        refusal: Refusal::new(kind, code, sentence),
    }
}

/// How many characters of a URL, a name, a method or a header's name a
/// refusal repeats: the plugin wrote them, and the sentence is the kernel's.
const QUOTED: usize = 128;

/// `text`, as a refusal of a fetch repeats it.
fn quoted(text: &str) -> String {
    crate::registry::quoted_up_to(text, QUOTED)
}

/// The request `asked` is, once every rule of this module holds — `domains`
/// being what the plugin's manifest declares, and `in_a_change` whether it is
/// running a command that changes state, inside its unit.
///
/// # Errors
///
/// A [`Refused`] for the first rule `asked` breaks, in the order the module
/// header lists them, with its reason and its code.
pub(crate) fn judged(
    asked: Asked,
    domains: &[String],
    in_a_change: bool,
) -> Result<Request, Refused> {
    let Asked {
        method,
        url,
        headers,
        body,
    } = asked;
    let request_refused = ErrorCode::PluginRequestRefused;
    let mut parsed = Url::parse(&url).map_err(|error| {
        refused(
            Reason::Url,
            RefusalKind::Invalid,
            request_refused,
            format!("{} is not a URL: {error}", quoted(&url)),
        )
    })?;
    let shown = quoted(parsed.as_str());
    if parsed.scheme() != "https" {
        return Err(refused(
            Reason::Scheme,
            RefusalKind::Denied,
            request_refused,
            format!("{shown} is not `https`, and a plugin fetches over HTTPS alone"),
        ));
    }
    let host = host_of(&parsed, &shown)?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(refused(
            Reason::Userinfo,
            RefusalKind::Denied,
            request_refused,
            format!(
                "{shown} carries a user or a password, which a plugin sends in a header if \
                 at all"
            ),
        ));
    }
    if let Some(port) = parsed.port() {
        return Err(refused(
            Reason::Port,
            RefusalKind::Denied,
            request_refused,
            format!("{shown} names the port {port}, and a plugin fetches from HTTPS's own"),
        ));
    }
    if !declares(domains, &host) {
        return Err(refused(
            Reason::Domain,
            RefusalKind::Denied,
            ErrorCode::PluginDomainNotDeclared,
            format!(
                "{} is not a name the plugin's manifest declares under `[net] domains`",
                quoted(&host)
            ),
        ));
    }
    let method = sendable(&method, in_a_change)?;
    held_to_the_rules(&headers)?;
    parsed.set_fragment(None);
    within_the_head(&parsed, &headers)?;
    carried(method, body.as_deref())?;
    Ok(Request {
        method,
        url: parsed,
        host,
        headers,
        body,
    })
}

/// The host of `parsed`, shown as `shown`, once it is a name: a DNS name, and
/// not this machine's.
///
/// # Errors
///
/// A [`Refused`] for a host that is an address, none, this machine, or no DNS
/// name, in that order.
fn host_of(parsed: &Url, shown: &str) -> Result<String, Refused> {
    let request_refused = ErrorCode::PluginRequestRefused;
    let host = match parsed.host() {
        Some(Host::Domain(name)) => name.to_owned(),
        Some(Host::Ipv4(_) | Host::Ipv6(_)) => {
            return Err(refused(
                Reason::IpLiteral,
                RefusalKind::Denied,
                request_refused,
                format!(
                    "{shown} names an address, and a plugin fetches from a name its manifest \
                     declares"
                ),
            ));
        }
        None => {
            return Err(refused(
                Reason::Url,
                RefusalKind::Invalid,
                request_refused,
                format!("{shown} names no host"),
            ));
        }
    };
    if is_localhost(&host) {
        return Err(refused(
            Reason::Localhost,
            RefusalKind::Denied,
            request_refused,
            format!("{shown} names this machine, which no plugin reaches through the network"),
        ));
    }
    if !a_dns_name(&host) {
        return Err(refused(
            Reason::Url,
            RefusalKind::Invalid,
            request_refused,
            format!(
                "{shown} names a host that is not a DNS name, whose labels are letters, \
                 digits and inner hyphens"
            ),
        ));
    }
    Ok(host)
}

/// Whether `host` is a DNS name: labels of one to sixty-three ASCII letters,
/// digits and inner hyphens, none empty, at most 253 bytes in all — and the
/// trailing dot of a name written whole, which is no label.
///
/// What a name a manifest declares is held to (`crate::manifest`), asked of
/// the request's host as well, since a `*.` declaration holds only its
/// suffix and a host the parser keeps may carry anything before it.
fn a_dns_name(host: &str) -> bool {
    let labels = host.strip_suffix('.').unwrap_or(host);
    labels.len() <= 253
        && labels.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Whether `host` is this machine's own name, or one under it, with or
/// without the trailing dot of a name written whole.
fn is_localhost(host: &str) -> bool {
    let name = host.strip_suffix('.').unwrap_or(host);
    name == "localhost" || name.ends_with(".localhost")
}

/// Whether `domains`, as a manifest declares them, hold `host`: the name
/// itself, or a name below one declared with `*.`.
///
/// **Below, never the apex.** `*.example.com` holds `cdn.example.com` and
/// `a.b.example.com`, and not `example.com`, which a manifest declares
/// beside it when it means it; nor `badexample.com`, which ends with the
/// same letters and is someone else's name.
pub(crate) fn declares(domains: &[String], host: &str) -> bool {
    domains
        .iter()
        .any(|declared| match declared.strip_prefix("*.") {
            Some(parent) => host
                .strip_suffix(parent)
                .is_some_and(|below| below.len() > 1 && below.ends_with('.')),
            None => declared == host,
        })
}

/// The method `word` names, when a plugin may send it from where it runs —
/// a method that leaves a mark only `in_a_change`.
fn sendable(word: &str, in_a_change: bool) -> Result<Method, Refused> {
    let method = Method::named(word).ok_or_else(|| {
        refused(
            Reason::Method,
            RefusalKind::Denied,
            ErrorCode::PluginRequestRefused,
            format!(
                "{} is not a method a plugin sends: it sends GET, HEAD, POST, PUT, PATCH and \
                 DELETE, written so",
                quoted(word)
            ),
        )
    })?;
    if method.leaves_a_mark() && !in_a_change {
        return Err(refused(
            Reason::Method,
            RefusalKind::Denied,
            ErrorCode::PluginRequestRefused,
            format!(
                "a {} asks the host to change something, and is sent only from a command that \
                 changes state, whose unit records it: `activate`, a callback and a command \
                 that only reads may GET and HEAD",
                method.as_str()
            ),
        ));
    }
    Ok(method)
}

/// The headers a plugin may not set, by their lower-case names.
///
/// Each is one the transport writes itself, or one that reaches past the
/// request to the connection or a proxy; `proxy-` stands for every name it
/// begins. **A list of what is refused, not of what is allowed**: every other
/// header that is HTTP is sent as the plugin set it.
const NOT_SENT: [&str; 10] = [
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "keep-alive",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "proxy-",
];

/// Whether `name` is a header a plugin may not set.
fn not_sent(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    NOT_SENT.iter().any(|refused| {
        refused
            .strip_suffix('-')
            .map_or(lower == *refused, |_| lower.starts_with(refused))
    })
}

/// Whether `name` is a header's name: one or more of HTTP's token characters.
fn is_a_token(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

/// Whether `value` is a header's value the kernel sends: visible ASCII,
/// spaces and tabs, and nothing a line break could be made of.
fn is_a_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte))
}

/// Refuses `headers` past their bound, or holding one a plugin may not set,
/// or one that is not a header at all.
fn held_to_the_rules(headers: &[(String, String)]) -> Result<(), Refused> {
    let bound = usize::try_from(KERNEL.net_header_count).unwrap_or(usize::MAX);
    if headers.len() > bound {
        return Err(refused(
            Reason::Header,
            RefusalKind::Invalid,
            ErrorCode::PluginBoundReached,
            format!(
                "a request carries at most {bound} header(s), and this one carries {}",
                headers.len()
            ),
        ));
    }
    for (name, value) in headers {
        if !is_a_token(name) || !is_a_value(value) {
            return Err(refused(
                Reason::Header,
                RefusalKind::Invalid,
                ErrorCode::PluginRequestRefused,
                format!(
                    "the header {} is not one HTTP carries: a name of token characters, and a \
                     value of visible ASCII with no line break",
                    quoted(name)
                ),
            ));
        }
        if not_sent(name) {
            return Err(refused(
                Reason::Header,
                RefusalKind::Denied,
                ErrorCode::PluginRequestRefused,
                format!(
                    "the header {} is the transport's to write, or reaches past the request, \
                     and no plugin sets it",
                    quoted(name)
                ),
            ));
        }
    }
    Ok(())
}

/// Refuses a request whose head — `url` as it is sent and the names and
/// values of `headers`, counted together — is past `net_request_head_bytes`:
/// by its URL when the URL alone is past the bound, and by its headers when
/// they take it past.
///
/// **Held below what HTTP can write.** Past it, the transport's `http`
/// refuses a URL or a header's name too long for it, and ureq a head longer
/// than its buffer, after every rule here has passed — and a request that
/// could never be sent was told to the plugin as a network that did not
/// answer, which it may ask again. A test of the transport writes a request
/// of exactly this bound whole.
fn within_the_head(url: &Url, headers: &[(String, String)]) -> Result<(), Refused> {
    let bound = usize::try_from(KERNEL.net_request_head_bytes).unwrap_or(usize::MAX);
    let sent = url.as_str().len();
    let head = headers.iter().fold(sent, |head, (name, value)| {
        head.saturating_add(name.len()).saturating_add(value.len())
    });
    if head <= bound {
        return Ok(());
    }
    let reason = if sent > bound {
        Reason::Url
    } else {
        Reason::Header
    };
    Err(refused(
        reason,
        RefusalKind::Invalid,
        ErrorCode::PluginBoundReached,
        format!(
            "a request's URL and headers are at most {bound} bytes together, and this \
             one's are {head}"
        ),
    ))
}

/// Refuses a body on a method that carries none, and a body past the
/// bound of `net_request_body_bytes`.
fn carried(method: Method, body: Option<&[u8]>) -> Result<(), Refused> {
    let Some(body) = body else {
        return Ok(());
    };
    if matches!(method, Method::Get | Method::Head) {
        return Err(refused(
            Reason::Body,
            RefusalKind::Invalid,
            ErrorCode::PluginRequestRefused,
            format!(
                "a {} carries no body, and this one carries one",
                method.as_str()
            ),
        ));
    }
    if u64::try_from(body.len()).unwrap_or(u64::MAX) > KERNEL.net_request_body_bytes {
        return Err(refused(
            Reason::Body,
            RefusalKind::Invalid,
            ErrorCode::PluginBoundReached,
            format!(
                "a request's body is at most {} bytes, and this one is {}",
                KERNEL.net_request_body_bytes,
                body.len()
            ),
        ));
    }
    Ok(())
}

/// `response`, once it is within the catalog's bounds on an answer.
///
/// **Held here again, whatever the transport bounded**, so a transport that
/// reads more than it should hands the plugin nothing of it.
///
/// # Errors
///
/// A [`Refused`] with `[NC-PLUG-0014]` for more headers than
/// `net_header_count` or a body past `net_response_body_bytes`, both of
/// them the catalog's.
pub(crate) fn answered(response: Response) -> Result<Response, Refused> {
    let headers = usize::try_from(KERNEL.net_header_count).unwrap_or(usize::MAX);
    let too_large = |why: String| {
        Err(refused(
            Reason::TooLarge,
            RefusalKind::Unavailable,
            ErrorCode::PluginResponseTooLarge,
            why,
        ))
    };
    if u64::try_from(response.body.len()).unwrap_or(u64::MAX) > KERNEL.net_response_body_bytes {
        return too_large(format!(
            "an answer's body is at most {} bytes, and this one is {}",
            KERNEL.net_response_body_bytes,
            response.body.len()
        ));
    }
    if response.headers.len() > headers {
        return too_large(format!(
            "an answer carries at most {headers} header(s), and this one carries {}",
            response.headers.len()
        ));
    }
    Ok(response)
}

/// What a transport that did not answer tells the plugin.
pub(crate) fn failed(error: &TransportError) -> Refused {
    let said = error.to_string();
    let sentence = said
        .strip_prefix(&format!("[{}] ", error.code()))
        .unwrap_or(&said)
        .to_owned();
    match error {
        TransportError::PrivateAddress { .. } => refused(
            Reason::PrivateAddress,
            RefusalKind::Denied,
            error.code(),
            sentence,
        ),
        TransportError::TooLarge(_) => refused(
            Reason::TooLarge,
            RefusalKind::Unavailable,
            error.code(),
            sentence,
        ),
        TransportError::UnwritableUrl(_) => {
            refused(Reason::Url, RefusalKind::Invalid, error.code(), sentence)
        }
        TransportError::UnwritableHeader(_) => {
            refused(Reason::Header, RefusalKind::Invalid, error.code(), sentence)
        }
        TransportError::Unavailable(_) => Refused {
            reason: None,
            refusal: Refusal::new(RefusalKind::Unavailable, error.code(), sentence),
        },
    }
}

/// What the unit a request that leaves a mark is sent from records of it.
///
/// Its method and the name it went to, never its path, its headers or its
/// body, which may hold what the plugin was sending: the note says that
/// something left the machine and where, which is what undoing the unit
/// cannot reach.
pub(crate) fn note(request: &Request) -> String {
    nc_canon::Value::object([
        ("host", nc_canon::Value::text(request.host.clone())),
        (
            "method",
            nc_canon::Value::text(request.method.as_str().to_owned()),
        ),
        ("note", nc_canon::Value::text(LEFT_THE_MACHINE.to_owned())),
    ])
    .to_canonical()
}

/// The kind of note a unit keeps of a request that left the machine.
pub(crate) const LEFT_THE_MACHINE: &str = "left_the_machine";

/// IPv4 blocks that are never dialled.
const V4_NON_PUBLIC: [(Ipv4Addr, u8); 15] = [
    // "This network" (RFC 1122), which includes the unspecified 0.0.0.0.
    (Ipv4Addr::UNSPECIFIED, 8),
    // Private (RFC 1918).
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    // Shared address space (RFC 6598): carrier-grade NAT, and the addresses
    // Tailscale and similar overlays number their hosts in.
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    // Loopback.
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    // Link-local, the cloud metadata address 169.254.169.254 among them.
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    // Private (RFC 1918).
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    // IETF protocol assignments (RFC 6890).
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    // Documentation, TEST-NET-1 (RFC 5737).
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    // The deprecated 6to4 relay anycast (RFC 7526).
    (Ipv4Addr::new(192, 88, 99, 0), 24),
    // Private (RFC 1918).
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    // Benchmarking (RFC 2544).
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    // Documentation, TEST-NET-2.
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    // Documentation, TEST-NET-3.
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    // Multicast.
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    // Reserved, the limited broadcast 255.255.255.255 among them.
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

/// The only IPv6 space that can be public: global unicast.
const V6_GLOBAL_UNICAST: (Ipv6Addr, u8) = (Ipv6Addr::new(0x2000, 0, 0, 0, 0, 0, 0, 0), 3);

/// Blocks inside global unicast that are never dialled.
const V6_NON_PUBLIC_IN_GLOBAL: [(Ipv6Addr, u8); 4] = [
    // IETF protocol assignments, Teredo's 2001::/32 among them, which embeds an
    // IPv4 address, and benchmarking and ORCHID.
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
    // Documentation (RFC 3849).
    (Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 0), 32),
    // 6to4 (RFC 3056), which embeds an IPv4 address.
    (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
    // Documentation (RFC 9637).
    (Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
];

/// The well-known NAT64 prefix (RFC 6052), whose last 32 bits are an IPv4
/// address a DNS64 resolver synthesised the IPv6 one from.
const V6_NAT64: (Ipv6Addr, u8) = (Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96);

/// Whether `ip` sits in the IPv4 block `net`/`len`.
fn v4_in(ip: Ipv4Addr, (net, len): (Ipv4Addr, u8)) -> bool {
    let mask = u32::MAX.checked_shl(32 - u32::from(len)).unwrap_or(0);
    u32::from(ip) & mask == u32::from(net) & mask
}

/// Whether `ip` sits in the IPv6 block `net`/`len`.
fn v6_in(ip: Ipv6Addr, (net, len): (Ipv6Addr, u8)) -> bool {
    let mask = u128::MAX.checked_shl(128 - u32::from(len)).unwrap_or(0);
    u128::from(ip) & mask == u128::from(net) & mask
}

/// Whether `ip` is an address `net.fetch` may dial: one on the public
/// internet.
///
/// **IPv4 is a list of what is refused**, every special-purpose block that is
/// not reachable globally. **IPv6 is a list of what is allowed** — global
/// unicast, the first eighth of the space, less the blocks inside it that
/// are not — so loopback, the unspecified address, IPv4-mapped and
/// IPv4-compatible addresses, the
/// local-use NAT64 prefix, discard, unique-local, link-local, site-local and
/// multicast are refused without being named one by one. **One exception, by
/// what it holds:** an address under the well-known NAT64 prefix is the IPv4
/// address in its last 32 bits, on a network whose resolver synthesises one
/// for a host that has only IPv4, and it is public exactly when that address
/// is — `64:ff9b::808:808` is 8.8.8.8, and `64:ff9b::a00:1` is 10.0.0.1, which
/// is refused. An address under a network-specific NAT64 prefix is not read
/// for what it carries, which the module's header says is not answered.
#[must_use]
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_is_public(v4),
        IpAddr::V6(v6) if v6_in(v6, V6_NAT64) => {
            let [.., a, b, c, d] = v6.octets();
            v4_is_public(Ipv4Addr::new(a, b, c, d))
        }
        IpAddr::V6(v6) => {
            v6_in(v6, V6_GLOBAL_UNICAST)
                && !V6_NON_PUBLIC_IN_GLOBAL
                    .iter()
                    .any(|block| v6_in(v6, *block))
        }
    }
}

/// Whether the IPv4 address `ip` is in no block that is never dialled.
fn v4_is_public(ip: Ipv4Addr) -> bool {
    !V4_NON_PUBLIC.iter().any(|block| v4_in(ip, *block))
}

/// A transport a test scripts, holding every request it was handed. **For
/// tests**, behind `test-support`.
///
/// It answers each fetch as [`Scripted::answering`] last said — a `200` with
/// no headers and no body until then — and, while a [`Stall`] it handed out
/// is held, waits before answering: as a transport that never returns, for at
/// most [`STALLED_AT_MOST`] of the real clock, so a test of a deadline that is
/// missing fails rather than hangs.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct Scripted {
    answer: std::sync::Mutex<Option<Result<Response, TransportError>>>,
    asked: std::sync::Mutex<Vec<Request>>,
    stalled: std::sync::Mutex<Option<Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>>>,
    reached: tokio::sync::Notify,
}

/// How long a stalled [`Scripted`] transport waits on the real clock before
/// it answers anyway: far past any deadline a test is about, and short enough
/// that a run whose deadline is missing ends.
#[cfg(any(test, feature = "test-support"))]
pub const STALLED_AT_MOST: std::time::Duration = std::time::Duration::from_secs(30);

/// What keeps a [`Scripted`] transport waiting, until it is dropped.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct Stall {
    _held: std::sync::mpsc::Sender<()>,
}

#[cfg(any(test, feature = "test-support"))]
impl Scripted {
    /// Makes every later fetch answer `answer`.
    pub fn answering(&self, answer: Result<Response, TransportError>) {
        *lock(&self.answer) = Some(answer);
    }

    /// Every request the transport was handed, in order.
    #[must_use]
    pub fn asked(&self) -> Vec<Request> {
        lock(&self.asked).clone()
    }

    /// Makes every later fetch wait, until the [`Stall`] answered is dropped.
    #[must_use]
    pub fn stall(&self) -> Stall {
        let (held, waiting) = std::sync::mpsc::channel();
        *lock(&self.stalled) = Some(Arc::new(std::sync::Mutex::new(waiting)));
        Stall { _held: held }
    }

    /// Waits until a fetch has reached the transport.
    pub async fn reached(&self) {
        self.reached.notified().await;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Transport for Scripted {
    fn fetch(&self, request: &Request) -> Result<Response, TransportError> {
        lock(&self.asked).push(request.clone());
        self.reached.notify_one();
        let stalled = lock(&self.stalled).clone();
        if let Some(waiting) = stalled {
            let _released = lock(&waiting).recv_timeout(STALLED_AT_MOST);
        }
        lock(&self.answer).clone().unwrap_or_else(|| {
            Ok(Response {
                status: 200,
                headers: Vec::new(),
                body: Vec::new(),
            })
        })
    }
}

/// `mutex`, whoever last held it: nothing panics holding it.
#[cfg(any(test, feature = "test-support"))]
fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use nc_catalog::{ErrorCode, KERNEL};

    use super::{
        Asked, Method, Reason, Refused, Response, Scripted, Transport, TransportError, answered,
        declares, failed, is_public, judged, note,
    };
    use crate::RefusalKind;

    /// The domains the requests below are held to.
    fn declared() -> Vec<String> {
        vec!["example.com".to_owned(), "*.cdn.example.net".to_owned()]
    }

    /// A `GET` of `url`, with nothing else.
    fn get(url: &str) -> Asked {
        asked("GET", url, Vec::new(), None)
    }

    /// What a plugin asked, written out.
    fn asked(
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Asked {
        Asked {
            method: method.to_owned(),
            url: url.to_owned(),
            headers,
            body,
        }
    }

    /// The reason, the case and the code `asked` was refused with, from a
    /// command that changes state when `in_a_change`.
    fn refused_as(asked: Asked, in_a_change: bool) -> (Option<Reason>, RefusalKind, ErrorCode) {
        let refused: Refused =
            judged(asked, &declared(), in_a_change).expect_err("the request is refused");
        (
            refused.reason,
            refused.refusal.kind(),
            refused.refusal.code(),
        )
    }

    /// The address `text` is.
    fn ip(text: &str) -> IpAddr {
        text.parse().expect("an address")
    }

    /// An address under a network-specific NAT64 prefix is judged as the IPv6
    /// address it is, and not by the IPv4 address it may carry, as one under
    /// the well-known prefix is: what the module's header says is not
    /// answered, held here so a judge that learns a network's prefixes
    /// changes that sentence in the same edit.
    ///
    /// A reviewer found it written nowhere: RFC 6052 §3.1 lets such a prefix
    /// carry a private IPv4 address, which the well-known one may not.
    #[test]
    fn an_address_under_a_network_specific_nat64_prefix_is_judged_as_the_ipv6_address_it_is() {
        for (carried, well_known, network_specific) in [
            ("10.0.0.1", "64:ff9b::a00:1", "2a00:1450:64::a00:1"),
            (
                "169.254.169.254",
                "64:ff9b::a9fe:a9fe",
                "2a00:1450:64::a9fe:a9fe",
            ),
            ("127.0.0.1", "64:ff9b::7f00:1", "2a00:1450:64::7f00:1"),
        ] {
            assert!(
                !is_public(ip(well_known)),
                "{carried} under the well-known prefix"
            );
            assert!(
                is_public(ip(network_specific)),
                "{carried} under 2a00:1450:64::/96 was read for what it carries, which the \
                 module's header says nothing reads"
            );
        }
    }

    #[test]
    fn a_name_the_manifest_declares_is_fetched_as_the_parser_writes_it() {
        let request = judged(
            get("https://EXAMPLE.com/a/../b?q=1#frag"),
            &declared(),
            false,
        )
        .expect("example.com is declared");
        assert_eq!(request.method(), Method::Get);
        assert_eq!(request.url().as_str(), "https://example.com/b?q=1");
        assert_eq!(request.host(), "example.com");
        assert!(request.headers().is_empty() && request.body().is_none());
        for (url, host) in [
            ("https://img.cdn.example.net/x", "img.cdn.example.net"),
            ("https://a.b.cdn.example.net/x", "a.b.cdn.example.net"),
            ("https://example.com:443/", "example.com"),
            (
                "https://ex\u{e4}mple.cdn.example.net/",
                "xn--exmple-cua.cdn.example.net",
            ),
        ] {
            let request = judged(get(url), &declared(), false).expect(url);
            assert_eq!(request.host(), host, "{url}");
            assert_eq!(request.url().port(), None, "{url}: `:443` is https's own");
        }
        // **The parser's reading, not the text's.** A backslash is a path
        // separator to WHATWG, so what follows it is a path on example.com.
        let backslash = judged(get("https://example.com\\@evil.test/"), &declared(), false)
            .expect("the host is example.com");
        assert_eq!(backslash.host(), "example.com");
    }

    #[test]
    fn a_name_is_declared_by_itself_or_below_a_wildcard_and_never_by_its_letters() {
        let domains = declared();
        assert!(declares(&domains, "example.com"));
        assert!(declares(&domains, "x.cdn.example.net"));
        assert!(declares(&domains, "y.x.cdn.example.net"));
        for host in [
            "cdn.example.net",
            "evilcdn.example.net",
            ".cdn.example.net",
            "www.example.com",
            "example.com.",
            "example.co",
            "xexample.com",
        ] {
            assert!(!declares(&domains, host), "{host} was declared");
        }
        assert!(
            !declares(&[], "example.com"),
            "nothing declared holds nothing"
        );
    }

    /// A name below a `*.` declaration is held to what a DNS name is, before
    /// it is held to the declaration: refused as a URL no request is sent to.
    ///
    /// **A reviewer found a real bug with it**: the kernel judged a host the
    /// parser keeps and HTTP cannot write — `a"b.cdn.example.net` — and hosts
    /// that are no DNS name, `a!b.` or an empty label among them, because a
    /// wildcard was asked only whether the host ended with its suffix.
    #[test]
    fn a_name_below_a_wildcard_is_held_to_what_a_dns_name_is() {
        for url in [
            "https://a\"b.cdn.example.net/",
            "https://a%22b.cdn.example.net/",
            "https://a{b.cdn.example.net/",
            "https://a}b.cdn.example.net/",
            "https://a`b.cdn.example.net/",
            "https://a!b.cdn.example.net/",
            "https://a$b.cdn.example.net/",
            "https://a;b.cdn.example.net/",
            "https://*.cdn.example.net/",
            "https://x..cdn.example.net/",
            "https://-x.cdn.example.net/",
            "https://a_b.cdn.example.net/",
        ] {
            assert_eq!(
                refused_as(get(url), false),
                (
                    Some(Reason::Url),
                    RefusalKind::Invalid,
                    ErrorCode::PluginRequestRefused
                ),
                "{url}"
            );
        }
        let long = format!("https://{}.cdn.example.net/", "a".repeat(64));
        assert_eq!(refused_as(get(&long), false).0, Some(Reason::Url), "{long}");
        // **At both edges of the name's bound, and a label's last character.**
        // A reviewer planted a bound of 253,000 bytes and dropped the rule on a
        // label's last hyphen together, and every suite of the crate passed:
        // the 64-byte label and the leading hyphen above held neither.
        let suffix = ".cdn.example.net";
        let host_of = |bytes: usize| {
            let last = bytes - suffix.len() - 3 * 64;
            format!(
                "{}.{}{suffix}",
                vec!["a".repeat(63); 3].join("."),
                "b".repeat(last)
            )
        };
        assert_eq!(host_of(253).len(), 253);
        let at_the_bound = format!("https://{}/", host_of(253));
        assert!(
            judged(get(&at_the_bound), &declared(), false).is_ok(),
            "a host of 253 bytes is a DNS name"
        );
        for url in [
            format!("https://{}/", host_of(254)),
            "https://x-.cdn.example.net/".to_owned(),
        ] {
            assert_eq!(
                refused_as(get(&url), false),
                (
                    Some(Reason::Url),
                    RefusalKind::Invalid,
                    ErrorCode::PluginRequestRefused
                ),
                "{url}"
            );
        }
        for url in [
            "https://a.cdn.example.net/",
            "https://a-b.x.cdn.example.net/",
            "https://xn--bcher-kva.cdn.example.net/",
        ] {
            assert!(
                judged(get(url), &declared(), false).is_ok(),
                "{url} is a DNS name below the wildcard"
            );
        }
    }

    #[test]
    fn a_name_the_manifest_does_not_declare_is_denied_as_undeclared() {
        for url in [
            "https://www.example.com/",
            "https://example.org/",
            "https://cdn.example.net/",
            "https://example.com./",
        ] {
            assert_eq!(
                refused_as(get(url), true),
                (
                    Some(Reason::Domain),
                    RefusalKind::Denied,
                    ErrorCode::PluginDomainNotDeclared
                ),
                "{url}"
            );
        }
    }

    /// Every spelling of an address the parser reads as one is refused as an
    /// address, never fetched as a name — those `step0-net` measured, each.
    #[test]
    fn an_address_in_any_spelling_is_refused_where_a_name_belongs() {
        for url in [
            "https://127.0.0.1/",
            "https://127.1/",
            "https://0x7f000001/",
            "https://2130706433/",
            "https://0177.0.0.1/",
            "https://0x7f.1/",
            "https://127.0.0.1./",
            "https://%31%32%37.0.0.1/",
            "https://\u{ff11}\u{ff12}\u{ff17}.0.0.1/",
            "https://0/",
            "https://[::1]/",
            "https://[0:0:0:0:0:0:0:1]/",
            "https://[::ffff:127.0.0.1]/",
            "https://169.254.169.254/latest/meta-data/",
            "https://8.8.8.8/",
        ] {
            assert_eq!(
                refused_as(get(url), true),
                (
                    Some(Reason::IpLiteral),
                    RefusalKind::Denied,
                    ErrorCode::PluginRequestRefused
                ),
                "{url}"
            );
        }
    }

    #[test]
    fn this_machines_own_name_is_refused_in_every_spelling() {
        for url in [
            "https://localhost/",
            "https://LOCALHOST/",
            "https://localhost./",
            "https://a.b.localhost/",
            "https://x.LocalHost./",
        ] {
            assert_eq!(
                refused_as(get(url), true),
                (
                    Some(Reason::Localhost),
                    RefusalKind::Denied,
                    ErrorCode::PluginRequestRefused
                ),
                "{url}"
            );
        }
    }

    #[test]
    fn a_scheme_a_user_and_a_port_are_each_refused() {
        let refused = |reason| {
            (
                Some(reason),
                RefusalKind::Denied,
                ErrorCode::PluginRequestRefused,
            )
        };
        for url in [
            "http://example.com/",
            "ftp://example.com/",
            "wss://example.com/",
            "file:///etc/passwd",
            "data:text/plain,hi",
        ] {
            assert_eq!(refused_as(get(url), true), refused(Reason::Scheme), "{url}");
        }
        // The second hides its user inside what reads as a host: the parser
        // reads `example.com%2F` as the user and `evil.test` as the host.
        for url in [
            "https://user:pass@example.com/",
            "https://user@example.com/",
            "https://:pass@example.com/",
            "https://example.com%2F@evil.test/",
        ] {
            assert_eq!(
                refused_as(get(url), true),
                refused(Reason::Userinfo),
                "{url}"
            );
        }
        for url in ["https://example.com:8443/", "https://example.com:0/"] {
            assert_eq!(refused_as(get(url), true), refused(Reason::Port), "{url}");
        }
        // An empty user is nothing the parser keeps, so nothing is sent.
        assert!(judged(get("https://@example.com/"), &declared(), false).is_ok());
        for url in ["not a url", "https://", "https://exa mple.com/"] {
            let (reason, kind, code) = refused_as(get(url), true);
            assert_eq!(
                (reason, kind, code),
                (
                    Some(Reason::Url),
                    RefusalKind::Invalid,
                    ErrorCode::PluginRequestRefused
                ),
                "{url}"
            );
        }
    }

    #[test]
    fn a_method_that_leaves_a_mark_is_sent_only_from_a_command_that_changes_state() {
        for method in Method::ALL {
            let body = method.leaves_a_mark().then(|| b"{}".to_vec());
            let from_a_change = judged(
                asked(
                    method.as_str(),
                    "https://example.com/",
                    Vec::new(),
                    body.clone(),
                ),
                &declared(),
                true,
            );
            assert!(from_a_change.is_ok(), "{method:?}: {from_a_change:?}");
            let from_a_read = judged(
                asked(method.as_str(), "https://example.com/", Vec::new(), body),
                &declared(),
                false,
            );
            assert_eq!(
                from_a_read.is_ok(),
                !method.leaves_a_mark(),
                "{method:?}: {from_a_read:?}"
            );
            if let Err(refused) = from_a_read {
                assert_eq!(refused.reason, Some(Reason::Method));
                assert_eq!(refused.refusal.code(), ErrorCode::PluginRequestRefused);
                assert!(refused.refusal.sentence().contains(method.as_str()));
            }
        }
        assert_eq!(
            Method::ALL.map(Method::leaves_a_mark),
            [false, false, true, true, true, true]
        );
        for word in ["get", "Get", "TRACE", "CONNECT", "OPTIONS", "", "GET "] {
            assert_eq!(
                refused_as(asked(word, "https://example.com/", Vec::new(), None), true),
                (
                    Some(Reason::Method),
                    RefusalKind::Denied,
                    ErrorCode::PluginRequestRefused
                ),
                "{word:?}"
            );
        }
    }

    #[test]
    fn a_header_the_transport_writes_or_that_reaches_past_the_request_is_refused() {
        let with = |name: &str, value: &str| {
            asked(
                "GET",
                "https://example.com/",
                vec![(name.to_owned(), value.to_owned())],
                None,
            )
        };
        for name in [
            "Host",
            "content-length",
            "Transfer-Encoding",
            "CONNECTION",
            "keep-alive",
            "Upgrade",
            "TE",
            "Trailer",
            "Expect",
            "Proxy-Authorization",
            "proxy-connection",
            "Proxy-Anything",
        ] {
            assert_eq!(
                refused_as(with(name, "x"), true),
                (
                    Some(Reason::Header),
                    RefusalKind::Denied,
                    ErrorCode::PluginRequestRefused
                ),
                "{name}"
            );
        }
        for (name, value) in [
            ("", "x"),
            ("a b", "x"),
            ("a:b", "x"),
            ("a\nb", "x"),
            ("x-ok", "a\r\nInjected: yes"),
            ("x-ok", "a\0b"),
            ("x-ok", "caf\u{e9}"),
            ("x-ok", "\u{7f}"),
        ] {
            assert_eq!(
                refused_as(with(name, value), true),
                (
                    Some(Reason::Header),
                    RefusalKind::Invalid,
                    ErrorCode::PluginRequestRefused
                ),
                "{name:?}: {value:?}"
            );
        }
        for (name, value) in [
            ("Accept", "application/json"),
            ("Authorization", "Bearer abc.def"),
            ("X-Api-Key", "k\t1 2 ~"),
            ("Proxied", "a name that only begins like one"),
            ("Hosting", "x"),
        ] {
            assert!(
                judged(with(name, value), &declared(), true).is_ok(),
                "{name}: {value}"
            );
        }
    }

    #[test]
    fn a_request_carries_at_most_the_catalogs_headers_and_body() {
        let bound = usize::try_from(KERNEL.net_header_count).expect("fits");
        let headers = |n: usize| -> Vec<(String, String)> {
            (0..n).map(|i| (format!("x-{i}"), "v".to_owned())).collect()
        };
        assert!(
            judged(
                asked("GET", "https://example.com/", headers(bound), None),
                &declared(),
                false
            )
            .is_ok(),
            "exactly the bound is carried"
        );
        assert_eq!(
            refused_as(
                asked("GET", "https://example.com/", headers(bound + 1), None),
                false
            ),
            (
                Some(Reason::Header),
                RefusalKind::Invalid,
                ErrorCode::PluginBoundReached
            )
        );
        let body = usize::try_from(KERNEL.net_request_body_bytes).expect("fits");
        assert!(
            judged(
                asked(
                    "POST",
                    "https://example.com/",
                    Vec::new(),
                    Some(vec![0; body])
                ),
                &declared(),
                true
            )
            .is_ok(),
            "exactly the bound is carried"
        );
        assert_eq!(
            refused_as(
                asked(
                    "POST",
                    "https://example.com/",
                    Vec::new(),
                    Some(vec![0; body + 1])
                ),
                true
            ),
            (
                Some(Reason::Body),
                RefusalKind::Invalid,
                ErrorCode::PluginBoundReached
            )
        );
        for method in ["GET", "HEAD"] {
            assert_eq!(
                refused_as(
                    asked(method, "https://example.com/", Vec::new(), Some(Vec::new())),
                    true
                ),
                (
                    Some(Reason::Body),
                    RefusalKind::Invalid,
                    ErrorCode::PluginRequestRefused
                ),
                "{method}"
            );
        }
        let posted = judged(
            asked(
                "PATCH",
                "https://example.com/",
                vec![("Content-Type".to_owned(), "text/plain".to_owned())],
                Some(b"hi".to_vec()),
            ),
            &declared(),
            true,
        )
        .expect("a body on a PATCH is carried");
        assert_eq!(posted.body(), Some(&b"hi"[..]));
        assert_eq!(
            posted.headers(),
            [("Content-Type".to_owned(), "text/plain".to_owned())]
        );
    }

    /// A refusal names the first rule a request breaks.
    ///
    /// In the order the module header lists them: a URL that breaks several
    /// is told of its scheme before its host, its host before its port, and
    /// its port before its domain.
    #[test]
    fn the_first_rule_a_request_breaks_is_the_refusal_it_gets() {
        for (url, reason) in [
            ("http://127.0.0.1:8080/", Reason::Scheme),
            ("https://user@127.0.0.1:8080/", Reason::IpLiteral),
            ("https://user@localhost:8080/", Reason::Localhost),
            ("https://user@example.org:8080/", Reason::Userinfo),
            ("https://example.org:8080/", Reason::Port),
        ] {
            assert_eq!(refused_as(get(url), true).0, Some(reason), "{url}");
        }
        assert_eq!(
            refused_as(
                asked("TRACE", "https://example.org/", Vec::new(), None),
                true
            )
            .0,
            Some(Reason::Domain),
            "the domain before the method"
        );
        assert_eq!(
            refused_as(
                asked(
                    "TRACE",
                    "https://example.com/",
                    vec![("Host".to_owned(), "x".to_owned())],
                    None
                ),
                true
            )
            .0,
            Some(Reason::Method),
            "the method before the headers"
        );
        assert_eq!(
            refused_as(
                asked(
                    "GET",
                    "https://example.com/",
                    vec![("Host".to_owned(), "x".to_owned())],
                    Some(Vec::new())
                ),
                true
            )
            .0,
            Some(Reason::Header),
            "the headers before the body"
        );
        let past = format!(
            "https://example.com/{}",
            "a".repeat(usize::try_from(KERNEL.net_request_head_bytes).expect("fits"))
        );
        let (reason, kind, _) = refused_as(
            asked(
                "GET",
                &past,
                vec![("Host".to_owned(), "x".to_owned())],
                None,
            ),
            true,
        );
        assert_eq!(
            (reason, kind),
            (Some(Reason::Header), RefusalKind::Denied),
            "the headers before the head"
        );
        assert_eq!(
            refused_as(asked("GET", &past, Vec::new(), Some(Vec::new())), true).0,
            Some(Reason::Url),
            "the head before the body"
        );
    }

    /// A request's head of exactly `net_request_head_bytes` is sent, and one
    /// byte more is refused.
    ///
    /// The head is the URL as it is sent and the headers' names and values,
    /// counted together: one past the bound is refused by its URL when the URL
    /// alone is past it, and by its headers when they take it past — a name
    /// counted as much as a value — and a fragment, never sent, counts for
    /// nothing.
    #[test]
    fn a_head_of_exactly_the_bound_is_sent_and_one_byte_more_is_refused() {
        let bound = usize::try_from(KERNEL.net_request_head_bytes).expect("fits");
        let base = "https://example.com/";
        let url_of = |length: usize| format!("{base}{}", "a".repeat(length - base.len()));
        let past_the_bound = |reason| {
            (
                Some(reason),
                RefusalKind::Invalid,
                ErrorCode::PluginBoundReached,
            )
        };
        let at = judged(get(&url_of(bound)), &declared(), false).expect("exactly the bound");
        assert_eq!(at.url().as_str().len(), bound);
        assert_eq!(
            refused_as(get(&url_of(bound + 1)), false),
            past_the_bound(Reason::Url)
        );
        let fragment = format!("{}#{}", url_of(bound), "f".repeat(64));
        assert!(
            judged(get(&fragment), &declared(), false).is_ok(),
            "a fragment is never sent, and was counted"
        );
        let valued = |length: usize| {
            asked(
                "GET",
                base,
                vec![("x-a".to_owned(), "v".repeat(length))],
                None,
            )
        };
        let room = bound - base.len() - "x-a".len();
        assert!(
            judged(valued(room), &declared(), false).is_ok(),
            "a URL and a header of exactly the bound"
        );
        assert_eq!(
            refused_as(valued(room + 1), false),
            past_the_bound(Reason::Header)
        );
        let named =
            |length: usize| asked("GET", base, vec![("x".repeat(length), String::new())], None);
        assert!(judged(named(bound - base.len()), &declared(), false).is_ok());
        assert_eq!(
            refused_as(named(bound - base.len() + 1), false),
            past_the_bound(Reason::Header)
        );
        assert_eq!(
            refused_as(
                asked(
                    "GET",
                    &url_of(bound + 1),
                    vec![("x-a".to_owned(), "v".to_owned())],
                    None
                ),
                false
            ),
            past_the_bound(Reason::Url),
            "a URL past the bound alone is refused by its URL, whatever is beside it"
        );
    }

    #[test]
    fn an_answer_is_held_to_the_catalogs_bounds_whatever_the_transport_read() {
        let body = usize::try_from(KERNEL.net_response_body_bytes).expect("fits");
        let headers = usize::try_from(KERNEL.net_header_count).expect("fits");
        let answer = |body: usize, headers: usize| Response {
            status: 200,
            headers: (0..headers)
                .map(|i| (format!("x-{i}"), "v".to_owned()))
                .collect(),
            body: vec![0; body],
        };
        assert!(
            answered(answer(body, headers)).is_ok(),
            "exactly the bounds"
        );
        for (too_large, what) in [
            (answer(body + 1, 0), "the body"),
            (answer(0, headers + 1), "the headers"),
        ] {
            let refused = answered(too_large).expect_err(what);
            assert_eq!(refused.reason, Some(Reason::TooLarge), "{what}");
            assert_eq!(
                (refused.refusal.kind(), refused.refusal.code()),
                (RefusalKind::Unavailable, ErrorCode::PluginResponseTooLarge),
                "{what}"
            );
        }
    }

    #[test]
    fn a_transport_that_did_not_answer_is_told_by_its_own_code() {
        for (error, reason, kind) in [
            (
                TransportError::PrivateAddress {
                    host: "internal.example.com".to_owned(),
                },
                Some(Reason::PrivateAddress),
                RefusalKind::Denied,
            ),
            (
                TransportError::TooLarge("the head was 70000 bytes".to_owned()),
                Some(Reason::TooLarge),
                RefusalKind::Unavailable,
            ),
            (
                TransportError::UnwritableUrl("uri too long".to_owned()),
                Some(Reason::Url),
                RefusalKind::Invalid,
            ),
            (
                TransportError::UnwritableHeader("invalid HTTP header name".to_owned()),
                Some(Reason::Header),
                RefusalKind::Invalid,
            ),
            (
                TransportError::Unavailable("connection refused".to_owned()),
                None,
                RefusalKind::Unavailable,
            ),
        ] {
            let refused = failed(&error);
            assert_eq!(refused.reason, reason, "{error}");
            assert_eq!(refused.refusal.kind(), kind, "{error}");
            assert_eq!(refused.refusal.code(), error.code(), "{error}");
            assert!(
                !refused.refusal.sentence().starts_with('['),
                "the code is said once, by the refusal: {}",
                refused.refusal
            );
        }
        assert_eq!(
            [
                TransportError::PrivateAddress {
                    host: String::new()
                }
                .code(),
                TransportError::TooLarge(String::new()).code(),
                TransportError::UnwritableUrl(String::new()).code(),
                TransportError::UnwritableHeader(String::new()).code(),
                TransportError::Unavailable(String::new()).code(),
            ],
            [
                ErrorCode::PluginRequestRefused,
                ErrorCode::PluginResponseTooLarge,
                ErrorCode::PluginRequestRefused,
                ErrorCode::PluginRequestRefused,
                ErrorCode::PluginNetworkUnavailable
            ]
        );
        assert!(
            TransportError::PrivateAddress {
                host: "internal.example.com".to_owned()
            }
            .to_string()
            .contains("`internal.example.com` resolves to no public address")
        );
    }

    #[test]
    fn a_request_that_left_the_machine_is_noted_by_its_method_and_its_name_alone() {
        let request = judged(
            asked(
                "POST",
                "https://example.com/secret/path?token=abc",
                vec![("Authorization".to_owned(), "Bearer abc".to_owned())],
                Some(b"the body".to_vec()),
            ),
            &declared(),
            true,
        )
        .expect("a POST from a change");
        assert_eq!(
            note(&request),
            r#"{"host":"example.com","method":"POST","note":"left_the_machine"}"#
        );
    }

    /// The words `net.refused` logs a reason by are the words the comment on
    /// its `reason` field in `catalog/log-events.toml` names in backticks, both
    /// ways.
    ///
    /// A reason the comment does not name is logged unannounced, and a word it
    /// names that no reason is logged by is a promise of nothing; the comment
    /// is the one written list of these words, which no generated file
    /// carries.
    #[test]
    fn every_reason_is_logged_by_a_word_the_event_names_and_no_other() {
        let events = include_str!("../../../catalog/log-events.toml");
        let block = events
            .split("[[event]]")
            .find(|block| block.contains("name = \"net.refused\""))
            .expect("the event is declared");
        let lines: Vec<&str> = block.lines().collect();
        let field = lines
            .iter()
            .position(|line| line.contains("name = \"reason\""))
            .expect("the event logs a reason");
        let above = lines[..field]
            .iter()
            .rev()
            .take_while(|line| line.trim_start().starts_with('#'))
            .count();
        let comment = lines[field - above..field].join("\n");
        let named: std::collections::BTreeSet<&str> =
            comment.split('`').skip(1).step_by(2).collect();
        let logged: std::collections::BTreeSet<&str> = Reason::ALL
            .iter()
            .map(|reason| reason.variant().get())
            .collect();
        assert_eq!(
            logged.len(),
            Reason::ALL.len(),
            "two reasons are logged as one word"
        );
        assert_eq!(
            named, logged,
            "the words the `reason` field's comment names are not the words a reason is \
             logged by"
        );
    }

    /// `Reason::ALL` holds every reason the enum declares, in the order it
    /// declares them, and nothing else.
    ///
    /// **Read from the declaration**, because a list of a fieldless enum's
    /// variants is one the compiler cannot hold to the enum: a reason added
    /// and given its word in `Reason::variant` compiles without joining
    /// `ALL`, and the test above would never compare its word with the
    /// event's comment.
    #[test]
    fn every_reason_declared_is_one_all_holds() {
        let source = include_str!("net.rs");
        let (_, after) = source
            .split_once(concat!("pub(crate) enum ", "Reason {"))
            .expect("the enum is declared");
        let (body, _) = after.split_once("\n}").expect("the enum ends");
        let declared: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with("#["))
            .map(|line| line.trim_end_matches(','))
            .collect();
        let held: Vec<String> = Reason::ALL
            .iter()
            .map(|reason| format!("{reason:?}"))
            .collect();
        assert_eq!(
            declared, held,
            "the reasons `Reason` declares are not the ones `Reason::ALL` holds"
        );
    }

    /// Each reason is logged by its own name in snake case, the word its
    /// declaration spells.
    ///
    /// **What the comparison of the event's words with the reasons' cannot
    /// see**: it compares two sets, and two reasons logged by each other's
    /// words leave both as they were — a URL refused logged as a body, and a
    /// body as a URL. Found by a reviewer, who swapped those two and left
    /// every suite green.
    #[test]
    fn each_reason_is_logged_by_its_own_name_in_snake_case() {
        for reason in Reason::ALL {
            let mut word = String::new();
            for (at, letter) in format!("{reason:?}").chars().enumerate() {
                if letter.is_ascii_uppercase() && at > 0 {
                    word.push('_');
                }
                word.push(letter.to_ascii_lowercase());
            }
            assert_eq!(reason.variant().get(), word, "{reason:?}");
        }
    }

    /// Every block that is never dialled is refused at both of its edges.
    ///
    /// The first and the last address of each, IPv4's fifteen and the IPv6
    /// space outside global unicast and inside it, so a block one bit too
    /// narrow is refused at one edge and not at the other.
    #[test]
    fn every_block_that_is_never_dialled_is_refused_at_both_edges() {
        let blocks = [
            ("0.0.0.0", "0.255.255.255"),
            ("10.0.0.0", "10.255.255.255"),
            ("100.64.0.0", "100.127.255.255"),
            ("127.0.0.0", "127.255.255.255"),
            ("169.254.0.0", "169.254.255.255"),
            ("172.16.0.0", "172.31.255.255"),
            ("192.0.0.0", "192.0.0.255"),
            ("192.0.2.0", "192.0.2.255"),
            ("192.88.99.0", "192.88.99.255"),
            ("192.168.0.0", "192.168.255.255"),
            ("198.18.0.0", "198.19.255.255"),
            ("198.51.100.0", "198.51.100.255"),
            ("203.0.113.0", "203.0.113.255"),
            ("224.0.0.0", "239.255.255.255"),
            ("240.0.0.0", "255.255.255.255"),
            ("::", "::"),
            ("::1", "::1"),
            ("::0.0.0.2", "::255.255.255.255"),
            ("::ffff:0.0.0.0", "::ffff:255.255.255.255"),
            ("64:ff9b:1::", "64:ff9b:1:ffff:ffff:ffff:ffff:ffff"),
            ("100::", "100::ffff:ffff:ffff:ffff"),
            ("2001::", "2001:1ff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("2001:db8::", "2001:db8:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("2002::", "2002:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("3fff::", "3fff:fff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("fc00::", "fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("fe80::", "febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("fec0::", "feff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("ff00::", "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("::", "1fff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
            ("4000::", "fbff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
        ];
        for (first, last) in blocks {
            assert!(!is_public(ip(first)), "{first} was public");
            assert!(!is_public(ip(last)), "{last} was public");
        }
        for (address, what) in [
            ("169.254.169.254", "the cloud metadata address"),
            ("fd00:ec2::254", "the cloud metadata address, on IPv6"),
            ("100.100.100.100", "the tailnet's resolver"),
            ("fd7a:115c:a1e0::53", "the tailnet's resolver, on IPv6"),
        ] {
            assert!(!is_public(ip(address)), "{what}, {address}, was public");
        }
    }

    /// The first address past each edge is public: a block one bit too wide
    /// refuses it.
    #[test]
    fn the_first_address_past_each_edge_is_public() {
        for address in [
            "1.0.0.0",
            "9.255.255.255",
            "11.0.0.0",
            "100.63.255.255",
            "100.128.0.0",
            "126.255.255.255",
            "128.0.0.0",
            "169.253.255.255",
            "169.255.0.0",
            "172.15.255.255",
            "172.32.0.0",
            "192.0.1.0",
            "192.0.3.0",
            "192.88.98.255",
            "192.88.100.0",
            "192.167.255.255",
            "192.169.0.0",
            "198.17.255.255",
            "198.20.0.0",
            "198.51.99.255",
            "198.51.101.0",
            "203.0.112.255",
            "203.0.114.0",
            "223.255.255.255",
            "2000::",
            "2001:200::",
            "2001:db7:ffff:ffff:ffff:ffff:ffff:ffff",
            "2001:db9::",
            "2003::",
            "3ffe:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
            "3fff:1000::",
            "2606:4700:10::6814:179a",
        ] {
            assert!(is_public(ip(address)), "{address} was refused");
        }
    }

    /// A NAT64 address is as public as the IPv4 address it carries, at both
    /// edges of its prefix and inside it; the local-use prefix beside it is
    /// refused whatever it carries.
    #[test]
    fn a_nat64_address_is_as_public_as_the_ipv4_address_it_carries() {
        for (address, public) in [
            ("64:ff9b::808:808", true),
            ("64:ff9b::8.8.8.8", true),
            ("64:ff9b::101:101", true),
            ("64:ff9b::a00:1", false),
            ("64:ff9b::7f00:1", false),
            ("64:ff9b::a9fe:a9fe", false),
            ("64:ff9b::6464:6464", false),
            ("64:ff9b::", false),
            ("64:ff9b::ffff:ffff", false),
            ("64:ff9b::100:0", true),
            ("64:ff9b::dfff:ffff", true),
            ("64:ff9a:ffff:ffff:ffff:ffff:808:808", false),
            ("64:ff9b:0:0:0:1:808:808", false),
            ("64:ff9b:1::808:808", false),
        ] {
            assert_eq!(is_public(ip(address)), public, "{address}");
        }
    }

    #[test]
    fn a_scripted_transport_answers_as_it_is_told_and_keeps_what_it_was_handed() {
        let transport = Scripted::default();
        let request = judged(get("https://example.com/a"), &declared(), false).expect("judged");
        assert_eq!(
            transport.fetch(&request),
            Ok(Response {
                status: 200,
                headers: Vec::new(),
                body: Vec::new()
            })
        );
        transport.answering(Err(TransportError::Unavailable("down".to_owned())));
        let shared = std::sync::Arc::new(transport);
        assert_eq!(
            shared.fetch(&request),
            Err(TransportError::Unavailable("down".to_owned())),
            "a transport shared answers as the one it shares"
        );
        assert_eq!(shared.asked(), [request.clone(), request]);
    }
}
