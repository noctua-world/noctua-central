//! The transport a daemon sends a plugin's fetch with: `ureq`, over `rustls`,
//! configured so that it can do what the kernel judged and nothing more.
//!
//! **Every default that reads the world is turned off.** `Config::default()`
//! reads `ALL_PROXY`, `HTTPS_PROXY` and `HTTP_PROXY` in either case, and
//! tunnels through the first it finds, so the proxy is set to none; redirects
//! are set to none followed, which answers a `3xx` as it came — `location`
//! and all — rather than refusing it as one redirect too many; a `4xx` and a
//! `5xx` are answers the plugin reads, not errors; `https_only` refuses
//! anything else again behind the policy; every one of the eleven timeouts
//! ureq 3.5 has — the nine budgets of a call and the two that end a read or a
//! write that does not progress — is `net_timeout_ms`, which the catalog holds
//! below the host call's deadline, so no part of a request waits longer than
//! the whole may; the head of an answer is held to [`HEAD_BYTES`], inside
//! the [`INPUT_BYTES`] ureq buffers of what it reads; and a request says it is
//! from `noctua-kernel`, where ureq's own would name its version to every
//! host — unless the plugin set a `User-Agent` of its own, which ureq sends
//! instead and the kernel lets through, as it does any header a plugin may
//! write. **What it trusts is written out as well**, each
//! the default ureq ships, so that a release moving one moves nothing here:
//! `rustls`, with Mozilla's roots compiled in — `RootCerts::WebPki`, never a
//! store read from the platform, which reads `SSL_CERT_FILE` and
//! `SSL_CERT_DIR` — every server's certificate verified against them, the
//! name asked for sent, and no certificate of its own presented.
//!
//! **An answer's body is read to `net_response_body_bytes` and no further**:
//! ureq's limit is exclusive — a body of exactly `n` bytes under `limit(n)`
//! fails as past it, measured — so it is given the bound and one byte more,
//! and a body of exactly the bound is read whole. **And handed on as the wire
//! carried it**: ureq decodes whatever `content-encoding` an answer names once
//! a decoder is compiled in — `gzip` and `brotli` are its features, and its
//! defaults turn the first on — and its limit counts the bytes under the
//! decoder, so one answer at the bound would be read whole however large it
//! inflates. Measured with `gzip` on: 33,665 bytes on the wire, 34,603,008
//! handed on against a bound of 33,554,432, at 1027 to 1. So no
//! decoder is built, no encoding is asked for, and both are held — by the wire
//! test below, and by `xtask` reading the features cargo resolves for ureq. **An answer with more
//! headers than ureq's parser holds is too large**, as one past
//! `net_header_count` is once it is read: the parser's bound, which
//! `ureq-proto` sets above the catalog's, refuses it as a protocol error, and
//! told as any other it would be a network that did not answer, which a
//! plugin is told to ask again. **A request `http` cannot write is the
//! plugin's**, told by the part it refused, its URL or a header, and never as
//! a network that did not answer: the bound on a request's head and the
//! rule that its host is a DNS name keep a request the kernel judged inside
//! what `http` and ureq's buffer carry, and this is what a request past them
//! would meet.
//!
//! **What it dials is what [`super::is_public`] accepts.** `PublicOnly` wraps
//! ureq's own resolver and keeps only the addresses it accepts, refusing a
//! name that leaves none as `TransportError::PrivateAddress` — a name like
//! `localtest.me` resolves to loopback from public DNS, which is how an
//! address hides behind a name. Read in ureq's source, and why that holds:
//! ureq resolves on every request before its pool is consulted, so a pooled
//! connection never skips the filter; its connector dials exactly the
//! addresses the resolver returned and never resolves again; and a proxy's
//! own host goes through the same resolver. The resolver belongs to the
//! agent, which a request's own configuration cannot replace, and the kernel
//! never hands a plugin a request to configure: it sends the text of
//! `Request::url`, the parser's, never the plugin's. `Resolver` lives in
//! `ureq::unversioned`, which a minor release may change.
//!
//! **Measured on the public network, which no test of this tree reaches**, in
//! a spike against ureq 3.4.2 (the gate must not depend on the
//! network, so the evidence is written here): google.com answered `301` with
//! its `location`, and nothing was followed; github.com's `404` was an answer;
//! example.com under a head bound of 256 bytes was refused as a head too
//! large; `localtest.me`, `any.localtest.me` and `127.0.0.1.nip.io`, public
//! names that resolve to loopback, were refused and nothing was dialled; and on
//! a network whose resolver synthesises NAT64, github.com came back both as
//! its IPv4 address and as that address inside `64:ff9b::` — which is why a NAT64 address is
//! judged by the IPv4 address it carries. What a local socket can show is held
//! by the tests below: the configuration as built, what it trusts among it,
//! the body's bound at its edge, the resolver against a trap on loopback
//! beside the same transport resolving anything and against an answer that
//! mixes public addresses with this machine's, what `fetch` writes on a wire
//! that dials nothing — a request at the bound on its head among it, and one
//! `http` cannot write — and what it reads back from one, an answer with more
//! headers than the parser holds among it, and the environment's proxy
//! against a trap a child process is pointed at. No test here completes a
//! TLS handshake: what holds the certificate's verification is the
//! configuration read back as built.
//!
//! What this answers (`NOCTUA.md §2.4`): a request reaching anything but a
//! public address of a name the kernel judged — through the environment's
//! proxy, a redirect, a pooled connection or a name that resolves to this
//! machine's loopback or to a private or link-local network; a request read
//! or answered by anything between the kernel and that host, which a
//! certificate verified against roots the environment cannot name keeps out;
//! and a failure that tells what is listening, since every request that did
//! not complete is told in one sentence ([`UNANSWERED`]). What it does not
//! answer: an address of this machine that is public — a global IPv6 address
//! of its interfaces, and every neighbour's — or a NAT's own public IPv4
//! address reached by hairpin, which [`super::is_public`] cannot tell from any
//! other public address, so a name resolving to one is dialled and only the
//! certificate's verification keeps out what answers there (`super`'s header
//! says what refusing them would need); a lookup that outlives its
//! timeout, which ureq's own resolver runs on a thread of its own that it
//! does not stop, and which the bounds on how often a plugin may ask bound;
//! what a public host a name resolves to is, which DNS says and the kernel
//! believes; and a certificate authority among Mozilla's that signs for a
//! name it should not, which the bundle compiled in is as good as.

use core::fmt;
use std::io;
use std::time::Duration;

use ::ureq::config::Config;
use ::ureq::http::{self, Uri};
use ::ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ::ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ::ureq::unversioned::transport::{DefaultConnector, NextTimeout};
use ::ureq::{Agent, Body};
use nc_catalog::KERNEL;

use super::{Request, Response, Transport, TransportError, is_public};

/// What every request says it is from: the kernel's name, and no version.
const USER_AGENT: &str = "noctua-kernel";

/// The most bytes the head of an answer may hold: its status line and its
/// headers.
///
/// It is the default ureq ships, written out so that a release moving it
/// moves nothing here; `net_header_count` bounds how many headers that may
/// be.
const HEAD_BYTES: usize = 65_536;

/// The most bytes ureq holds of what it has read and not yet handed on,
/// which the head of an answer is read inside.
///
/// The default ureq ships, written out beside [`HEAD_BYTES`] because the two
/// are one bound: **a head past this is never refused as one** — ureq stops
/// making progress, and the read fails as a network that did not answer, which
/// a plugin is told it may retry. Measured with `HEAD_BYTES` raised to a
/// megabyte: a 70,000-byte head is read whole, and a 200,000-byte one told
/// as `[NC-PLUG-0015]`.
const INPUT_BYTES: usize = 131_072;

/// The transport a daemon sends a plugin's fetch with: HTTPS to a public
/// address, as this module's header says.
#[derive(Debug)]
pub struct UreqTransport {
    agent: Agent,
}

impl UreqTransport {
    /// The transport, dialling only what [`super::is_public`] accepts.
    #[must_use]
    pub fn new() -> Self {
        Self::resolving_with(PublicOnly::<DefaultResolver>::default())
    }

    /// The same transport, resolving names with `resolver`: what a test needs
    /// to show that the resolver is what stops a request.
    fn resolving_with(resolver: impl Resolver) -> Self {
        Self {
            agent: Agent::with_parts(configured(), DefaultConnector::default(), resolver),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// The agent's configuration, every bound set and every default that reads
/// the world turned off.
fn configured() -> Config {
    let budget = Some(Duration::from_millis(KERNEL.net_timeout_ms));
    Config::builder()
        .proxy(None)
        .max_redirects(0)
        .https_only(true)
        .http_status_as_error(false)
        .timeout_global(budget)
        .timeout_per_call(budget)
        .timeout_per_read(budget)
        .timeout_per_write(budget)
        .timeout_resolve(budget)
        .timeout_connect(budget)
        .timeout_send_request(budget)
        .timeout_await_100(budget)
        .timeout_send_body(budget)
        .timeout_recv_response(budget)
        .timeout_recv_body(budget)
        .max_response_header_size(HEAD_BYTES)
        .input_buffer_size(INPUT_BYTES)
        .user_agent(USER_AGENT)
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::Rustls)
                .root_certs(RootCerts::WebPki)
                .client_cert(None)
                .use_sni(true)
                .disable_verification(false)
                .build(),
        )
        .build()
}

impl Transport for UreqTransport {
    fn fetch(&self, request: &Request) -> Result<Response, TransportError> {
        let (head, body) = sent(request)
            .map_err(|refused| unwritable(&refused))?
            .into_parts();
        let mut response = match body {
            Some(body) => self.agent.run(http::Request::from_parts(head, body)),
            None => self.agent.run(http::Request::from_parts(head, ())),
        }
        .map_err(|error| unanswered(&error))?;
        answer_of(&mut response)
    }
}

/// What ureq is handed for `request`: the head [`head_of`] writes, and the
/// body the plugin gave, when it gave one.
///
/// # Errors
///
/// What `http` refuses in the head, which a request the kernel judged does
/// not carry.
fn sent(request: &Request) -> Result<http::Request<Option<Vec<u8>>>, http::Error> {
    head_of(request).body(request.body().map(<[u8]>::to_vec))
}

/// The request line and the headers `request` is sent with: its method, the
/// parser's text of its URL and the headers the plugin set — grouped by name,
/// as `http`'s map holds them, each name's values in the plugin's order and
/// the names in the order each first appears.
fn head_of(request: &Request) -> http::request::Builder {
    request.headers().iter().fold(
        http::Request::builder()
            .method(request.method().as_str())
            .uri(request.url().as_str()),
        |head, (name, value)| head.header(name.as_str(), value.as_str()),
    )
}

/// What `response` answered: its body, read whole when it is no longer
/// than `net_response_body_bytes`.
///
/// **The bound is the catalog's, read here and taken from no caller**, so
/// the one place a body is read is the one place its bound is named.
///
/// # Errors
///
/// [`TransportError::TooLarge`] for a body past `net_response_body_bytes`,
/// and what reading it met otherwise.
fn answer_of(response: &mut http::Response<Body>) -> Result<Response, TransportError> {
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = response
        .body_mut()
        .with_config()
        .limit(KERNEL.net_response_body_bytes.saturating_add(1))
        .read_to_vec()
        .map_err(|error| unanswered(&error))?;
    Ok(Response {
        status,
        headers,
        body,
    })
}

/// What `http` refusing to write a request is to the kernel: a URL or a
/// header it cannot carry, the plugin's to change since asking again is
/// refused again, and anything else a request that did not complete.
fn unwritable(refused: &http::Error) -> TransportError {
    let why = format!("the request could not be written: {refused}");
    if refused.is::<http::uri::InvalidUri>() {
        TransportError::UnwritableUrl(why)
    } else if refused.is::<http::header::InvalidHeaderName>()
        || refused.is::<http::header::InvalidHeaderValue>()
    {
        TransportError::UnwritableHeader(why)
    } else {
        TransportError::Unavailable(why)
    }
}

/// What a request that did not complete is told, whatever stopped it.
///
/// **One sentence, so the failure tells nothing of the address.** ureq's own
/// text tells a closed port (`Connection refused`) from an open one that speaks
/// no TLS to it (`received corrupt message`), so with a declared name resolving
/// to this machine's own global IPv6 address, which ports of it are listening
/// can be read from ureq's sentence.
const UNANSWERED: &str = "the request did not complete: no answer came from the host it names, \
                          in time and in HTTP over TLS";

/// What ureq's `error` is to the kernel.
fn unanswered(error: &::ureq::Error) -> TransportError {
    if let ::ureq::Error::Io(io) = error
        && let Some(refused) = io
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<NonPublicAddress>())
    {
        return TransportError::PrivateAddress {
            host: refused.host.clone(),
        };
    }
    match error {
        ::ureq::Error::BodyExceedsLimit(_) => TransportError::TooLarge(format!(
            "an answer's body is at most {} bytes, and this one was longer",
            KERNEL.net_response_body_bytes
        )),
        ::ureq::Error::Protocol(::ureq_proto::Error::HttpParseTooManyHeaders) => {
            TransportError::TooLarge(format!(
                "an answer carries at most {} header(s), and this one carried more",
                KERNEL.net_header_count
            ))
        }
        ::ureq::Error::LargeResponseHeader(_, _) => TransportError::TooLarge(format!(
            "an answer's head is at most {HEAD_BYTES} bytes, and this one was longer"
        )),
        _ => TransportError::Unavailable(UNANSWERED.to_owned()),
    }
}

/// Why a name was not dialled: every address it resolved to is outside the
/// public internet.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NonPublicAddress {
    host: String,
}

impl fmt::Display for NonPublicAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`{}` resolves only to addresses outside the public internet",
            self.host
        )
    }
}

impl std::error::Error for NonPublicAddress {}

/// The resolver ureq ships, with every address [`super::is_public`] refuses
/// taken out of what it answers.
///
/// **What it answers is what is dialled**: ureq's connector dials the
/// addresses a resolver returns, in order, and never resolves again — so an
/// answer that mixes a public address with this machine's loses the second
/// and keeps the first, where it was; refusing the whole answer would only
/// be stricter, and keeping it whole would dial loopback once the public
/// address refused the connection. The resolver it filters is a parameter so
/// that a test can hand it an answer no DNS of its own would give.
#[derive(Debug, Default)]
struct PublicOnly<R = DefaultResolver> {
    inner: R,
}

impl<R: Resolver> Resolver for PublicOnly<R> {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ::ureq::Error> {
        let resolved = self.inner.resolve(uri, config, timeout)?;
        let mut kept = self.empty();
        for address in resolved.iter().filter(|address| is_public(address.ip())) {
            kept.push(*address);
        }
        if kept.is_empty() {
            return Err(::ureq::Error::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                NonPublicAddress {
                    host: uri.host().unwrap_or_default().to_owned(),
                },
            )));
        }
        Ok(kept)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use ::ureq::config::{AutoHeaderValue, Config};
    use ::ureq::tls::{RootCerts, TlsProvider};
    use ::ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
    use ::ureq::unversioned::transport::{
        Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout,
    };
    use ::ureq::{Agent, Body, http};
    use nc_catalog::{ErrorCode, KERNEL};

    use super::{
        HEAD_BYTES, INPUT_BYTES, PublicOnly, UNANSWERED, USER_AGENT, UreqTransport, answer_of,
        configured, sent, unanswered, unwritable,
    };
    use crate::net::{Asked, Method, Reason, Request, Response, Transport, TransportError, judged};

    /// A request the policy would never judge, built by hand: what lets these
    /// tests aim the transport at loopback, which the policy refuses first.
    fn aimed_at(url: &str) -> Request {
        let url = url::Url::parse(url).expect("a URL");
        Request {
            method: Method::Get,
            host: url.host_str().expect("a host").to_owned(),
            url,
            headers: Vec::new(),
            body: None,
        }
    }

    #[test]
    fn the_agent_is_built_with_every_bound_and_no_default_that_reads_the_world() {
        let transport = UreqTransport::new();
        let config: &Config = transport.agent.config();
        assert!(config.proxy().is_none(), "a proxy is read from somewhere");
        assert_eq!(config.max_redirects(), 0, "a redirect is followed");
        assert!(config.https_only(), "plain HTTP is sent");
        assert!(!config.http_status_as_error(), "a 4xx is an error");
        let budget = Some(Duration::from_millis(KERNEL.net_timeout_ms));
        let timeouts = config.timeouts();
        assert_eq!(
            [
                timeouts.global,
                timeouts.per_call,
                timeouts.resolve,
                timeouts.connect,
                timeouts.send_request,
                timeouts.await_100,
                timeouts.send_body,
                timeouts.recv_response,
                timeouts.recv_body,
                config.timeout_per_read(),
                config.timeout_per_write(),
            ],
            [budget; 11],
            "a part of a request waits without a bound, or past the catalog's"
        );
        assert_eq!(config.max_response_header_size(), HEAD_BYTES);
        assert_eq!(config.input_buffer_size(), INPUT_BYTES);
        assert!(
            matches!(config.user_agent(), AutoHeaderValue::Provided(said) if said.as_str() == USER_AGENT),
            "{:?}",
            config.user_agent()
        );
        assert_eq!(USER_AGENT, "noctua-kernel");
        let tls = config.tls_config();
        assert!(
            !tls.disable_verification(),
            "a server's certificate is taken unchecked"
        );
        assert!(
            matches!(tls.root_certs(), RootCerts::WebPki),
            "the roots trusted are not the ones compiled in: {:?}",
            tls.root_certs()
        );
        assert_eq!(tls.provider(), TlsProvider::Rustls);
        assert!(
            tls.client_cert().is_none(),
            "the transport presents a certificate of its own"
        );
        assert!(tls.use_sni(), "the name asked for is not sent");
    }

    /// A body of exactly `net_response_body_bytes` is read whole, and one byte
    /// more is refused: the catalog's bound, which no caller hands the reader.
    #[test]
    fn a_body_of_exactly_the_bound_is_read_and_one_byte_more_is_refused() {
        let bound = usize::try_from(KERNEL.net_response_body_bytes).expect("fits");
        let answered = |bytes: usize| {
            let mut response = http::Response::builder()
                .status(200)
                .body(Body::builder().data(vec![7_u8; bytes]))
                .expect("a response");
            answer_of(&mut response).map(|response| response.body.len())
        };
        assert_eq!(
            answered(bound),
            Ok(bound),
            "exactly the bound is read whole"
        );
        let past = answered(bound + 1);
        assert!(matches!(past, Err(TransportError::TooLarge(_))), "{past:?}");
        assert_eq!(answered(0), Ok(0));
    }

    /// What ureq could not do is told as the kernel tells it.
    ///
    /// A head or a body past its bound, or more headers than ureq's parser
    /// holds, is too large, and anything else — no name, a timeout, a
    /// connection that failed, an answer that is not HTTP — is a request that
    /// did not complete, told in the one sentence and never in ureq's words,
    /// which tell a closed port from an open one.
    #[test]
    fn what_ureq_could_not_do_is_told_as_the_kernel_tells_it() {
        assert!(matches!(
            unanswered(&::ureq::Error::LargeResponseHeader(70_000, HEAD_BYTES)),
            TransportError::TooLarge(why) if why.contains("head")
        ));
        assert!(matches!(
            unanswered(&::ureq::Error::BodyExceedsLimit(10)),
            TransportError::TooLarge(why) if why.contains("body")
        ));
        assert!(matches!(
            unanswered(&::ureq::Error::Protocol(::ureq_proto::Error::HttpParseTooManyHeaders)),
            TransportError::TooLarge(why) if why.contains("header")
        ));
        for error in [
            ::ureq::Error::HostNotFound,
            ::ureq::Error::ConnectionFailed,
            ::ureq::Error::Timeout(::ureq::Timeout::Global),
            ::ureq::Error::Io(std::io::Error::other("reset by the host")),
            ::ureq::Error::Protocol(::ureq_proto::Error::HttpParseFail("not HTTP".to_owned())),
        ] {
            let said = error.to_string();
            assert!(
                matches!(unanswered(&error), TransportError::Unavailable(ref why) if why == UNANSWERED),
                "{said}"
            );
        }
    }

    /// A redirect is answered as it came, with its `location`, and the head
    /// keeps each name's values in the host's order.
    #[test]
    fn a_redirect_is_an_answer_with_its_location() {
        let mut response = http::Response::builder()
            .status(302)
            .header("location", "https://elsewhere.example/")
            .header("x-two", "a")
            .header("x-two", "b")
            .body(Body::builder().data(b"moved".to_vec()))
            .expect("a response");
        let answered = answer_of(&mut response).expect("an answer");
        assert_eq!(answered.status, 302);
        assert_eq!(
            answered.headers,
            [
                (
                    "location".to_owned(),
                    "https://elsewhere.example/".to_owned()
                ),
                ("x-two".to_owned(), "a".to_owned()),
                ("x-two".to_owned(), "b".to_owned()),
            ]
        );
        assert_eq!(answered.body, b"moved");
    }

    /// The headers of a request and of an answer are kept grouped by name:
    /// each name's values in the order they were written, the names in the
    /// order each first appears — what `http`'s map holds, and the one order
    /// HTTP gives a meaning to (RFC 9110 §5.3).
    ///
    /// No more than that: a name written around another is regrouped both ways,
    /// so neither the plugin's order nor the host's is kept.
    #[test]
    fn headers_are_kept_grouped_by_name_with_each_names_values_in_order() {
        let request = to_example(
            Method::Get,
            &[("x-a", "1"), ("x-b", "2"), ("x-a", "3")],
            None,
        );
        let (answered, written) = answered_with(
            &request,
            b"HTTP/1.1 200 OK\r\nset-cookie: one\r\nx-between: 1\r\nset-cookie: two\r\n\
              content-length: 0\r\n\r\n"
                .to_vec(),
        );
        let answered = answered.expect("answered");
        let read: Vec<(&str, &str)> = answered
            .headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        assert_eq!(
            read,
            [
                ("set-cookie", "one"),
                ("set-cookie", "two"),
                ("x-between", "1"),
                ("content-length", "0"),
            ]
        );
        let sent: Vec<&str> = written
            .lines()
            .filter(|line| line.starts_with("x-"))
            .collect();
        assert_eq!(sent, ["x-a: 1", "x-a: 3", "x-b: 2"]);
    }

    /// A `method` of a path on example.com carrying `headers` and `body`,
    /// as the policy would hand it on.
    fn to_example(method: Method, headers: &[(&str, &str)], body: Option<&[u8]>) -> Request {
        Request {
            method,
            host: "example.com".to_owned(),
            url: url::Url::parse("https://example.com/a?b=1").expect("a URL"),
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body: body.map(<[u8]>::to_vec),
        }
    }

    /// What ureq is handed is the request the kernel judged: its method, the
    /// parser's URL, every header in the plugin's order and the body it gave
    /// — and no body at all where it gave none.
    #[test]
    fn a_request_is_written_as_the_kernel_judged_it() {
        let written = sent(&to_example(
            Method::Patch,
            &[
                ("Content-Type", "text/plain"),
                ("X-Twice", "1"),
                ("X-Twice", "2"),
            ],
            Some(b"hi"),
        ))
        .expect("a request");
        assert_eq!(written.method(), http::Method::PATCH);
        assert_eq!(written.uri().to_string(), "https://example.com/a?b=1");
        assert_eq!(written.headers()["content-type"], "text/plain");
        assert_eq!(
            written
                .headers()
                .get_all("x-twice")
                .iter()
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert_eq!(written.body().as_deref(), Some(&b"hi"[..]));
        let written = sent(&to_example(Method::Get, &[], None)).expect("a request");
        assert_eq!(written.method(), http::Method::GET);
        assert_eq!(written.body(), &None, "a GET carries nothing");
    }

    /// What `http` refuses to write is told by the part it refused: a URL, or
    /// a header's name or value — the plugin's to change — and anything else
    /// as a request that did not complete.
    #[test]
    fn what_http_cannot_write_is_told_by_the_part_it_refused() {
        let refused = |head: http::request::Builder| {
            unwritable(&head.body(()).expect_err("`http` refuses the head"))
        };
        let long = format!("https://example.com/{}", "a".repeat(70_000));
        assert!(matches!(
            refused(http::Request::builder().uri(long.as_str())),
            TransportError::UnwritableUrl(why) if why.contains("too long")
        ));
        for head in [
            http::Request::builder().header("a b", "x"),
            http::Request::builder().header("x-a", "a\nb"),
        ] {
            assert!(matches!(refused(head), TransportError::UnwritableHeader(_)));
        }
        assert!(matches!(
            refused(http::Request::builder().method("NOT A METHOD")),
            TransportError::Unavailable(_)
        ));
    }

    /// The budget a resolver below is given: one no test reaches.
    fn next() -> NextTimeout {
        NextTimeout {
            after: Duration::from_secs(5).into(),
            reason: ::ureq::Timeout::Resolve,
            per_read: None,
            per_write: None,
        }
    }

    #[test]
    fn a_name_that_resolves_to_no_public_address_is_refused_by_name() {
        let next = next();
        for uri in [
            "https://localhost/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://169.254.169.254/",
            "https://[::ffff:127.0.0.1]/",
            "https://[64:ff9b::7f00:1]/",
        ] {
            let refused = PublicOnly::<DefaultResolver>::default()
                .resolve(&uri.parse().expect("a URI"), &Config::default(), next)
                .expect_err(uri);
            let host = uri
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .to_owned();
            assert_eq!(
                unanswered(&refused),
                TransportError::PrivateAddress { host },
                "{uri}"
            );
        }
        let public = DefaultResolver::default()
            .resolve(
                &"https://[64:ff9b::808:808]/".parse().expect("a URI"),
                &Config::default(),
                next,
            )
            .expect("an address resolves to itself");
        let kept = PublicOnly::<DefaultResolver>::default()
            .resolve(
                &"https://[64:ff9b::808:808]/".parse().expect("a URI"),
                &Config::default(),
                next,
            )
            .expect("8.8.8.8 behind NAT64 is public");
        assert_eq!(
            kept.iter().collect::<Vec<_>>(),
            public.iter().collect::<Vec<_>>()
        );
    }

    /// A resolver that answers every name with the same addresses, as the
    /// owner of a declared name can make DNS answer it.
    #[derive(Debug)]
    struct Answering(Vec<SocketAddr>);

    impl Resolver for Answering {
        fn resolve(
            &self,
            _uri: &http::Uri,
            _config: &Config,
            _timeout: NextTimeout,
        ) -> Result<ResolvedSocketAddrs, ::ureq::Error> {
            let mut answered = self.empty();
            for address in &self.0 {
                answered.push(*address);
            }
            Ok(answered)
        }
    }

    /// Of an answer that mixes public addresses with others, the public ones
    /// are kept, in the order they came, and nothing else.
    ///
    /// **The shape DNS rebinding takes**: a name its owner points at a public
    /// address that refuses the connection and at this machine beside it, so
    /// that the connector, trying them in order, dials the second. An answer
    /// that is all private or all public cannot tell a filter of each address
    /// from a filter of the whole, which is what every other test here hands
    /// it.
    #[test]
    fn of_a_mixed_answer_only_the_public_addresses_are_kept_in_order() {
        let at = |text: &str| -> SocketAddr { text.parse().expect("an address") };
        let resolver = PublicOnly {
            inner: Answering(vec![
                at("8.8.8.8:443"),
                at("127.0.0.1:443"),
                at("[::1]:443"),
                at("[64:ff9b::a00:1]:443"),
                at("[2606:4700::1]:443"),
            ]),
        };
        let kept = resolver
            .resolve(
                &"https://rebinding.example.com/".parse().expect("a URI"),
                &Config::default(),
                next(),
            )
            .expect("two of the addresses are public");
        assert_eq!(
            kept.iter().copied().collect::<Vec<_>>(),
            [at("8.8.8.8:443"), at("[2606:4700::1]:443")]
        );
    }

    /// A connector that dials nothing: each connection it makes keeps every
    /// byte ureq writes to it in `written`, and answers with `answer`.
    #[derive(Debug)]
    struct Wire {
        answer: Vec<u8>,
        written: Arc<Mutex<Vec<u8>>>,
    }

    impl Connector for Wire {
        type Out = Recorded;

        fn connect(
            &self,
            details: &ConnectionDetails,
            _chained: Option<()>,
        ) -> Result<Option<Recorded>, ::ureq::Error> {
            Ok(Some(Recorded {
                buffers: LazyBuffers::new(
                    details.config.input_buffer_size(),
                    details.config.output_buffer_size(),
                ),
                answer: self.answer.clone(),
                read: 0,
                written: Arc::clone(&self.written),
            }))
        }
    }

    /// One connection a [`Wire`] made, and how much of its answer it has
    /// handed back.
    #[derive(Debug)]
    struct Recorded {
        buffers: LazyBuffers,
        answer: Vec<u8>,
        read: usize,
        written: Arc<Mutex<Vec<u8>>>,
    }

    impl ::ureq::unversioned::transport::Transport for Recorded {
        fn buffers(&mut self) -> &mut dyn Buffers {
            &mut self.buffers
        }

        fn transmit_output(
            &mut self,
            amount: usize,
            _timeout: NextTimeout,
        ) -> Result<(), ::ureq::Error> {
            let output = &self.buffers.output()[..amount];
            self.written
                .lock()
                .expect("the wire")
                .extend_from_slice(output);
            Ok(())
        }

        fn await_input(&mut self, _timeout: NextTimeout) -> Result<bool, ::ureq::Error> {
            let left = &self.answer[self.read..];
            let free = self.buffers.input_append_buf();
            let amount = free.len().min(left.len());
            free[..amount].copy_from_slice(&left[..amount]);
            self.buffers.input_appended(amount);
            self.read += amount;
            Ok(amount > 0)
        }

        /// Spent once it has answered: a request takes a connection of its
        /// own, and none is pooled.
        fn is_open(&mut self) -> bool {
            false
        }

        /// As a TLS transport says, since ureq sends an `https` request over
        /// nothing else.
        fn is_tls(&self) -> bool {
            true
        }
    }

    /// What `request` puts on the wire, and what the answer is read as.
    ///
    /// The transport is built as a daemon's is and sends to a public address
    /// over a [`Wire`] answering `answer`, which it reads as a host's.
    fn answered_with(
        request: &Request,
        answer: Vec<u8>,
    ) -> (Result<Response, TransportError>, String) {
        let written = Arc::new(Mutex::new(Vec::new()));
        let transport = UreqTransport {
            agent: Agent::with_parts(
                configured(),
                Wire {
                    answer,
                    written: Arc::clone(&written),
                },
                Answering(vec![SocketAddr::from(([8, 8, 8, 8], 443))]),
            ),
        };
        let answered = transport.fetch(request);
        let written = written.lock().expect("the wire").clone();
        (
            answered,
            String::from_utf8(written).expect("a request is text"),
        )
    }

    /// What `request` puts on the wire, as [`answered_with`] says, when the
    /// host answers a `201` with one header and a body of two bytes.
    fn on_the_wire(request: &Request) -> (Result<Response, TransportError>, String) {
        answered_with(
            request,
            b"HTTP/1.1 201 Created\r\nx-answer: yes\r\ncontent-length: 2\r\n\r\nok".to_vec(),
        )
    }

    /// What `fetch` writes on the wire is the request the kernel judged — its
    /// request line, the plugin's headers, and its body framed by its length,
    /// or no body and no length where there is none — and what came back is
    /// the answer.
    ///
    /// The one test here that drives `fetch` whole, through ureq's own agent
    /// with the daemon's configuration: a body dropped between the kernel and
    /// the agent is seen here, and nowhere a part is tested alone.
    #[test]
    fn what_reaches_the_wire_is_the_request_the_kernel_judged() {
        let (answered, written) = on_the_wire(&to_example(
            Method::Patch,
            &[("Content-Type", "text/plain")],
            Some(b"hi"),
        ));
        let answered = answered.expect("an answer");
        assert_eq!(
            (answered.status, answered.body.as_slice()),
            (201, &b"ok"[..])
        );
        assert!(
            answered
                .headers
                .contains(&("x-answer".to_owned(), "yes".to_owned())),
            "{:?}",
            answered.headers
        );
        let (head, body) = written.split_once("\r\n\r\n").expect("a head, then a body");
        assert!(head.starts_with("PATCH /a?b=1 HTTP/1.1\r\n"), "{head}");
        let lines: Vec<String> = head.lines().map(str::to_ascii_lowercase).collect();
        for line in [
            "host: example.com",
            "content-type: text/plain",
            "content-length: 2",
            "user-agent: noctua-kernel",
        ] {
            assert!(
                lines.iter().any(|written| written == line),
                "`{line}` is not in {head}"
            );
        }
        assert_eq!(body, "hi", "the body the plugin gave");
        let (answered, written) = on_the_wire(&to_example(Method::Get, &[], None));
        assert_eq!(answered.map(|answered| answered.status), Ok(201));
        let (head, body) = written.split_once("\r\n\r\n").expect("a head, then a body");
        assert!(head.starts_with("GET /a?b=1 HTTP/1.1\r\n"), "{head}");
        let head = head.to_ascii_lowercase();
        assert!(
            !head.contains("content-length") && !head.contains("transfer-encoding"),
            "a GET is framed as carrying a body: {head}"
        );
        assert_eq!(body, "", "a GET carries nothing");
    }

    /// An answer whose head is past [`HEAD_BYTES`] is too large over the wire,
    /// naming the head — one just past it, and one past [`INPUT_BYTES`] too.
    ///
    /// **Sent over the wire, not built by hand**: with `HEAD_BYTES` raised past
    /// ureq's input buffer, a test building the error by hand would still pass,
    /// and this fails.
    #[test]
    fn an_answer_whose_head_is_past_its_bound_is_too_large_on_the_wire() {
        let get = to_example(Method::Get, &[], None);
        for bytes in [70_000_usize, 200_000] {
            let mut answer = b"HTTP/1.1 200 OK\r\nx-long: ".to_vec();
            answer.extend(std::iter::repeat_n(b'a', bytes));
            answer.extend_from_slice(b"\r\ncontent-length: 0\r\n\r\n");
            let (answered, _) = answered_with(&get, answer);
            assert!(
                matches!(&answered, Err(TransportError::TooLarge(why)) if why.contains("head")),
                "a head of {bytes} bytes: {answered:?}"
            );
        }
    }

    /// A `User-Agent` the plugin sets is the one sent, and the kernel adds
    /// none beside it: the kernel's name is what a request says it is from
    /// only when the plugin says nothing.
    ///
    /// **What `AGENTS.md` says is sent**: the kernel's name only when the plugin
    /// sets no `User-Agent`, and this holds the transport to it.
    #[test]
    fn a_user_agent_the_plugin_sets_is_sent_instead_of_the_kernels_name() {
        let request = judged(
            Asked {
                method: "GET".to_owned(),
                url: "https://example.com/a".to_owned(),
                headers: vec![("User-Agent".to_owned(), "curl/8.0 (anything)".to_owned())],
                body: None,
            },
            &["example.com".to_owned()],
            false,
        )
        .expect("the policy judges it");
        let (answered, written) = on_the_wire(&request);
        assert_eq!(answered.map(|answered| answered.status), Ok(201));
        let head = written.to_ascii_lowercase();
        assert!(
            head.contains("\r\nuser-agent: curl/8.0 (anything)\r\n"),
            "{written}"
        );
        assert!(
            !head.contains("noctua-kernel"),
            "the kernel's name was sent beside the plugin's: {written}"
        );
    }

    /// An answer the host encoded reaches the plugin byte for byte as the
    /// wire carried it, whatever encoding it names, and no request asks for
    /// one: the body bound counts what the plugin is handed.
    ///
    /// **Held on the wire, beside `xtask`'s check of ureq's features**: with
    /// ureq's `gzip` feature turned on, a compressed answer would be read whole
    /// through the bound, which counts the bytes under the decoder.
    #[test]
    fn an_encoded_answer_is_handed_on_as_the_wire_carried_it() {
        // `what the host compressed`, as `gzip` writes it with no time in it.
        const GZIP: [u8; 44] = [
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x2b, 0xcf, 0x48, 0x2c,
            0x51, 0x28, 0xc9, 0x48, 0x55, 0xc8, 0xc8, 0x2f, 0x2e, 0x51, 0x48, 0xce, 0xcf, 0x2d,
            0x28, 0x4a, 0x2d, 0x2e, 0x4e, 0x4d, 0x01, 0x00, 0x28, 0x95, 0x9b, 0x33, 0x18, 0x00,
            0x00, 0x00,
        ];
        for encoding in ["gzip", "br", "deflate", "zstd"] {
            let mut answer = format!(
                "HTTP/1.1 200 OK\r\ncontent-encoding: {encoding}\r\ncontent-length: {}\r\n\r\n",
                GZIP.len()
            )
            .into_bytes();
            answer.extend_from_slice(&GZIP);
            let (answered, written) = answered_with(&to_example(Method::Get, &[], None), answer);
            let answered = answered.unwrap_or_else(|failed| panic!("{encoding}: {failed:?}"));
            assert_eq!(
                answered.body, GZIP,
                "{encoding}: the body was not the wire's"
            );
            let (head, _) = written.split_once("\r\n\r\n").expect("a head");
            assert!(
                !head.to_ascii_lowercase().contains("accept-encoding"),
                "{encoding}: the request asked for an encoding: {head}"
            );
        }
    }

    /// A request whose head is exactly `net_request_head_bytes` — its URL
    /// alone, or its URL and a header together — is judged and then written
    /// whole, and its answer read: the bound sits inside what `http` and
    /// ureq's buffer can carry.
    ///
    /// **Through the kernel's own judgement and the daemon's configuration**,
    /// so that a bound raised past either, or a buffer that shrank under it,
    /// is a request the kernel accepts and cannot send, failing here.
    #[test]
    fn a_request_whose_head_is_exactly_the_bound_is_written_whole() {
        let bound = usize::try_from(KERNEL.net_request_head_bytes).expect("fits");
        let base = "https://example.com/";
        let judged_as = |url: String, headers: Vec<(String, String)>| {
            judged(
                Asked {
                    method: "GET".to_owned(),
                    url,
                    headers,
                    body: None,
                },
                &["example.com".to_owned()],
                false,
            )
            .expect("exactly the bound is sent")
        };
        let path = "a".repeat(bound - base.len());
        let (answered, written) = on_the_wire(&judged_as(format!("{base}{path}"), Vec::new()));
        assert_eq!(answered.map(|answered| answered.status), Ok(201));
        assert!(
            written.starts_with(&format!("GET /{path} HTTP/1.1\r\n")),
            "the URL was not written whole"
        );
        let half = (bound - base.len()) / 2;
        let path = "b".repeat(half);
        let value = "c".repeat(bound - base.len() - half - "x-fill".len());
        let (answered, written) = on_the_wire(&judged_as(
            format!("{base}{path}"),
            vec![("x-fill".to_owned(), value.clone())],
        ));
        assert_eq!(answered.map(|answered| answered.status), Ok(201));
        assert!(written.starts_with(&format!("GET /{path} HTTP/1.1\r\n")));
        assert!(
            written.contains(&format!("\r\nx-fill: {value}\r\n")),
            "the header was not written whole"
        );
    }

    /// A request `http` cannot write reaches no wire, and is refused by the
    /// part it could not write: what a policy that let it through would hand
    /// the transport.
    #[test]
    fn a_request_http_cannot_write_is_refused_by_its_part_and_nothing_is_written() {
        let long = format!("https://example.com/{}", "a".repeat(70_000));
        let (answered, written) = on_the_wire(&aimed_at(&long));
        assert!(
            matches!(answered, Err(TransportError::UnwritableUrl(_))),
            "{answered:?}"
        );
        assert_eq!(written, "", "a request was written");
        let named = to_example(Method::Get, &[(&"x".repeat(70_000), "v")], None);
        let (answered, written) = on_the_wire(&named);
        assert!(
            matches!(answered, Err(TransportError::UnwritableHeader(_))),
            "{answered:?}"
        );
        assert_eq!(written, "", "a request was written");
    }

    /// An answer with more headers than `net_header_count` is too large,
    /// whichever side of ureq's parser it falls on.
    ///
    /// **Up to the most headers the parser holds**, ureq reads the answer and
    /// the kernel refuses it again; past that, ureq refuses to parse it, and
    /// the refusal is told as an answer too large — the code the kernel's own
    /// bound gives — and never as a network that did not answer, which a
    /// plugin is told to ask again for ever. Told as a protocol error, an
    /// answer of 129 headers would be `[NC-PLUG-0015]`, and logged as nothing.
    #[test]
    fn an_answer_with_more_headers_than_the_transport_parses_is_too_large() {
        let bound = usize::try_from(KERNEL.net_header_count).expect("fits");
        let answer = |headers: usize| {
            let mut answer = b"HTTP/1.1 200 OK\r\n".to_vec();
            for at in 1..headers {
                answer.extend_from_slice(format!("x-{at}: v\r\n").as_bytes());
            }
            answer.extend_from_slice(b"content-length: 0\r\n\r\n");
            answer
        };
        let get = to_example(Method::Get, &[], None);
        for headers in [100, 128] {
            assert!(headers > bound, "{headers} headers are within the bound");
            let (read, _) = answered_with(&get, answer(headers));
            let read = read.expect("the parser holds this many headers");
            assert_eq!(read.headers.len(), headers);
            let refused = crate::net::answered(read).expect_err("more headers than the bound");
            assert_eq!(refused.refusal.code(), ErrorCode::PluginResponseTooLarge);
        }
        for headers in [129, 1_000] {
            let (refused, _) = answered_with(&get, answer(headers));
            assert!(
                matches!(&refused, Err(TransportError::TooLarge(why)) if why.contains("header")),
                "{headers} headers: {refused:?}"
            );
            let told = crate::net::failed(&refused.expect_err("refused"));
            assert_eq!(
                (told.reason, told.refusal.code()),
                (Some(Reason::TooLarge), ErrorCode::PluginResponseTooLarge),
                "{headers} headers"
            );
        }
    }

    /// A listener on loopback that counts every connection made to it and
    /// answers each with a line no TLS client reads, until it is finished.
    #[derive(Debug)]
    struct Trap {
        at: SocketAddr,
        stopping: Arc<AtomicBool>,
        dialled: Arc<AtomicU64>,
        thread: JoinHandle<()>,
    }

    impl Trap {
        /// A trap on `listener`, which it owns from here on.
        fn on(listener: TcpListener) -> Self {
            let at = listener.local_addr().expect("the trap's address");
            let stopping = Arc::new(AtomicBool::new(false));
            let dialled = Arc::new(AtomicU64::new(0));
            let thread = {
                let (stopping, dialled) = (Arc::clone(&stopping), Arc::clone(&dialled));
                std::thread::spawn(move || {
                    for stream in listener.incoming() {
                        if stopping.load(Ordering::SeqCst) {
                            break;
                        }
                        dialled.fetch_add(1, Ordering::SeqCst);
                        if let Ok(mut stream) = stream {
                            let _gone = std::io::Write::write_all(
                                &mut stream,
                                b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n",
                            );
                        }
                    }
                })
            };
            Self {
                at,
                stopping,
                dialled,
                thread,
            }
        }

        /// A trap on a port of loopback the system chose.
        fn new() -> Self {
            Self::on(TcpListener::bind("127.0.0.1:0").expect("a port on loopback"))
        }

        /// Stops the trap, and answers how many connections it was dialled.
        fn finished(self) -> u64 {
            self.stopping.store(true, Ordering::SeqCst);
            let _woken = TcpStream::connect(self.at);
            self.thread.join().expect("the trap's thread");
            self.dialled.load(Ordering::SeqCst)
        }
    }

    /// Traps on one port of both loopbacks — the second only where this
    /// machine has IPv6 — so a name resolving to either lands in one.
    fn on_both_loopbacks() -> (Trap, Option<Trap>) {
        let v4 = Trap::new();
        let v6 = TcpListener::bind(("::1", v4.at.port())).ok().map(Trap::on);
        (v4, v6)
    }

    /// How many connections the traps were dialled, together.
    fn dialled((v4, v6): (Trap, Option<Trap>)) -> u64 {
        v4.finished() + v6.map_or(0, Trap::finished)
    }

    /// The resolver is what keeps the transport off loopback: the same
    /// transport resolving with ureq's own resolver dials the trap.
    #[test]
    fn the_resolver_dials_no_address_off_the_public_internet() {
        let traps = on_both_loopbacks();
        let url = format!("https://localhost:{}/", traps.0.at.port());
        assert_eq!(
            UreqTransport::new().fetch(&aimed_at(&url)),
            Err(TransportError::PrivateAddress {
                host: "localhost".to_owned()
            })
        );
        assert_eq!(dialled(traps), 0, "the transport dialled loopback");
        let traps = on_both_loopbacks();
        let url = format!("https://localhost:{}/", traps.0.at.port());
        let resolving_anything = UreqTransport::resolving_with(DefaultResolver::default());
        assert!(
            matches!(
                resolving_anything.fetch(&aimed_at(&url)),
                Err(TransportError::Unavailable(_))
            ),
            "the trap is not a TLS server"
        );
        assert!(
            dialled(traps) > 0,
            "the control dialled nothing, so the refusal above shows nothing"
        );
    }

    /// The variable a child of [`the_proxy_the_environment_names_is_never_dialled`]
    /// reads: the port of the trap it fetches from, and how it fetches.
    const CHILD: &str = "NC_KERNEL_NET_PROXY_CHILD";

    /// Every variable ureq reads a proxy from.
    const PROXIES: [&str; 8] = [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "NO_PROXY",
        "no_proxy",
    ];

    /// The proxy the environment names is never dialled.
    ///
    /// **In a child process**, because the environment is the process's and
    /// setting a variable here is `unsafe` in this edition: the child is this
    /// test binary, given `HTTPS_PROXY` and `ALL_PROXY` naming a trap, and
    /// fetching from a second trap with the kernel's configuration — resolving
    /// anything, so that only the configuration stands between the request and
    /// the proxy. The proxy's trap is dialled by nothing, and the target's is,
    /// directly; and the same child fetching with ureq's own defaults dials the
    /// proxy, which is what makes the first half show something.
    #[test]
    fn the_proxy_the_environment_names_is_never_dialled() {
        if let Some(asked) = std::env::var_os(CHILD) {
            return child(&asked.to_string_lossy());
        }
        for (how, through_the_proxy) in [("kernel", false), ("defaults", true)] {
            let proxy = Trap::new();
            let target = on_both_loopbacks();
            let ran = spawned(
                &format!("{how}:{}", target.0.at.port()),
                &format!("http://127.0.0.1:{}", proxy.at.port()),
            );
            assert!(ran.success(), "the `{how}` child failed: {ran}");
            let (proxied, direct) = (proxy.finished(), dialled(target));
            assert_eq!(
                (proxied > 0, direct > 0),
                (through_the_proxy, !through_the_proxy),
                "`{how}`: the proxy was dialled {proxied} time(s), the target {direct}"
            );
        }
    }

    /// What a child does, as `asked` says.
    ///
    /// `kernel:<port>` fetches from that port of loopback with this module's
    /// configuration, and `defaults:<port>` with ureq's own.
    fn child(asked: &str) {
        let (how, port) = asked.split_once(':').expect("how and a port");
        let url = format!("https://localhost:{port}/");
        if how == "kernel" {
            let answered =
                UreqTransport::resolving_with(DefaultResolver::default()).fetch(&aimed_at(&url));
            assert!(answered.is_err(), "a trap answered TLS: {answered:?}");
        } else {
            let agent = ::ureq::Agent::new_with_config(
                Config::builder()
                    .timeout_global(Some(Duration::from_secs(10)))
                    .build(),
            );
            let _refused_by_the_trap = agent.get(&url).call();
        }
    }

    /// This test binary, run again as [`CHILD`] says, with the environment's
    /// proxy set to `proxy` and nothing it prints shown.
    #[expect(
        clippy::disallowed_methods,
        reason = "the proxy's child is this same test binary, its output discarded: the one \
                  way to give a process an environment without setting it in this one"
    )]
    fn spawned(asked: &str, proxy: &str) -> std::process::ExitStatus {
        let mut command =
            std::process::Command::new(std::env::current_exe().expect("this test binary"));
        for variable in PROXIES {
            command.env_remove(variable);
        }
        command
            .args([
                "--exact",
                "net::ureq::tests::the_proxy_the_environment_names_is_never_dialled",
                "--test-threads=1",
            ])
            .env(CHILD, asked)
            .env("HTTPS_PROXY", proxy)
            .env("ALL_PROXY", proxy)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the child runs")
    }
}
