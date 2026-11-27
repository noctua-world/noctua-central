//! What the webview loads, and the one way it reaches it: the `noctua`
//! custom protocol (`NOCTUA.md §12.10`, §4), answered from bytes this
//! build embeds.
//!
//! **A closed list of paths, each answered with bytes the binary holds**
//! ([`answer`]):
//!
//! | Path | What it is |
//! |---|---|
//! | `/` | the document ([`crate::page`]), built before the webview exists |
//! | `/sheet.css` | the interface's sheet of layout utilities ([`crate::sheet`]) |
//! | `/noctua.js` | the document's own script ([`crate::page::SCRIPT`]) |
//! | `/enhances.js` | the module that hands the page to the design system's scripts ([`crate::page::ENHANCES`]) |
//! | `/dist/<file>` | each file the design system ships, by the name `noctua-design-dist` gives it — its stylesheets, its behaviour modules, its palettes |
//! | `/dioxus/interpreter.js` | Dioxus's interpreter, the module that applies a virtual DOM's edits |
//!
//! A path is the address's path exactly as the webview sends it — nothing
//! decoded, nothing joined, nothing normalized — matched whole against the
//! list, and any other is answered **404**; a method other than `GET` is
//! answered **405**. Every type a file is served as is one of a closed set,
//! read from its name's extension.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! A page, a link or a script asking for a file of the person's, or of this
//! build's install, through the protocol: **no path reaches the file system**,
//! since nothing here opens a file — every answer is a constant compiled in,
//! or the document this process built — so `..`, an escaped `%2e`, a doubled
//! `/` or a drive letter is a path no row holds, answered 404 like any other.
//! **What the page may load from anywhere else is nothing**: the document's
//! policy ([`crate::page::POLICY`]) lets it load a script, a sheet, an image
//! or a font from this protocol alone and connect, frame or submit nowhere, so
//! a script running in it can fetch no address and send nothing off the
//! machine; and where it may navigate is [`crate::host`]'s, which refuses every
//! address outside this protocol's, a new window and a download.

use std::borrow::Cow;

/// The custom protocol's scheme: every address the webview loads begins
/// with it, or, on Windows, names it as its host (`nc_platform::webview`).
pub const SCHEME: &str = "noctua";

/// Where the design system's files are, under the root.
pub const DIST: &str = "dist/";

/// Where the interface's sheet of layout utilities is, under the root.
pub const SHEET: &str = "sheet.css";

/// Where Dioxus's interpreter is, under the root.
pub const INTERPRETER: &str = "dioxus/interpreter.js";

/// Where the document's own script is, under the root.
pub const SCRIPT: &str = "noctua.js";

/// Where the module that enhances the page is, under the root.
pub const ENHANCES: &str = "enhances.js";

/// The address of the root the protocol serves, as this system's webview
/// loads it, ending in `/`.
#[must_use]
pub fn origin() -> String {
    nc_platform::webview::origin(SCHEME)
}

/// Each extension a file is served with, and the type it is served as.
const TYPES: [(&str, &str); 6] = [
    ("css", "text/css; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("json", "application/json"),
    ("html", "text/html; charset=utf-8"),
    ("svg", "image/svg+xml"),
    ("txt", "text/plain; charset=utf-8"),
];

/// The type a file named `name` is served as, read from its extension; none
/// for an extension outside the list this module serves.
#[must_use]
// allow-logging: `type_of` hands back a type written as a literal in `TYPES`, never text a caller gave
pub fn type_of(name: &str) -> Option<&'static str> {
    let (_, extension) = name.rsplit_once('.')?;
    TYPES
        .iter()
        .find(|(served, _)| *served == extension)
        .map(|(_, kind)| *kind)
}

/// What the protocol answers one request with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    /// The bytes at a path the list holds, and the type they are served as.
    Found {
        /// The type, for `Content-Type`.
        kind: &'static str,
        /// The bytes.
        body: Cow<'static, [u8]>,
    },
    /// A path the list does not hold.
    NotFound,
    /// A method other than `GET`.
    NotAllowed,
}

impl Answered {
    /// The status the answer is sent with.
    #[must_use]
    pub const fn status(&self) -> u16 {
        match self {
            Self::Found { .. } => 200,
            Self::NotFound => 404,
            Self::NotAllowed => 405,
        }
    }
}

/// The bytes `path` names under the root, from what this build embeds, and
/// the type they are served as; none for a path the list does not hold.
// allow-logging: `embedded` hands back bytes and a type compiled into this binary — the sheet, the interpreter, the design system's files and `TYPES` — never text a caller gave
fn embedded(path: &str) -> Option<(&'static str, &'static str)> {
    let under = path.strip_prefix('/')?;
    if under == SHEET {
        return Some((crate::sheet::SHEET, "text/css; charset=utf-8"));
    }
    if under == INTERPRETER {
        return Some((
            dioxus_interpreter_js::INTERPRETER_JS,
            "text/javascript; charset=utf-8",
        ));
    }
    if under == SCRIPT {
        return Some((crate::page::SCRIPT, "text/javascript; charset=utf-8"));
    }
    if under == ENHANCES {
        return Some((crate::page::ENHANCES, "text/javascript; charset=utf-8"));
    }
    let name = under.strip_prefix(DIST)?;
    let (_, contents) = noctua_design_dist::FILES
        .iter()
        .find(|(shipped, _)| *shipped == name)?;
    Some((contents, type_of(name)?))
}

/// What the protocol answers a request of `method` for `path`, the
/// address's path as the webview sent it; `document` is what `/` is.
#[must_use]
pub fn answer(method: &str, path: &str, document: &str) -> Answered {
    if method != "GET" {
        return Answered::NotAllowed;
    }
    if path == "/" {
        return Answered::Found {
            kind: "text/html; charset=utf-8",
            body: Cow::Owned(document.as_bytes().to_vec()),
        };
    }
    match embedded(path) {
        Some((contents, kind)) => Answered::Found {
            kind,
            body: Cow::Borrowed(contents.as_bytes()),
        },
        None => Answered::NotFound,
    }
}

/// `answered` as the response the webview is handed.
#[must_use]
pub fn response(answered: Answered) -> wry::http::Response<Cow<'static, [u8]>> {
    let status = answered.status();
    let (kind, body) = match answered {
        Answered::Found { kind, body } => (kind, body),
        Answered::NotFound | Answered::NotAllowed => {
            ("text/plain; charset=utf-8", Cow::Borrowed(&b""[..]))
        }
    };
    let mut response = wry::http::Response::new(body);
    *response.status_mut() =
        wry::http::StatusCode::from_u16(status).unwrap_or(wry::http::StatusCode::NOT_FOUND);
    if let Ok(kind) = wry::http::HeaderValue::from_str(kind) {
        response
            .headers_mut()
            .insert(wry::http::header::CONTENT_TYPE, kind);
    }
    response
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{Answered, DIST, ENHANCES, INTERPRETER, SCRIPT, SHEET, answer, response, type_of};

    const DOCUMENT: &str = "<!doctype html><html></html>";

    /// The bytes `path` is answered with, which must be found.
    // allow-logging: `found` hands back the type `answer` served, a literal of `TYPES`
    fn found(path: &str) -> (&'static str, Vec<u8>) {
        match answer("GET", path, DOCUMENT) {
            Answered::Found { kind, body } => (kind, body.into_owned()),
            other => panic!("{path} was answered {other:?}"),
        }
    }

    /// The root is the document, the sheet is the interface's, the
    /// interpreter is Dioxus's, and the two scripts are the document's own,
    /// each as the type it is.
    #[test]
    fn the_root_the_sheet_and_the_interpreter_are_served() {
        assert_eq!(
            found("/"),
            ("text/html; charset=utf-8", DOCUMENT.as_bytes().to_vec())
        );
        assert_eq!(
            found(&format!("/{SHEET}")),
            (
                "text/css; charset=utf-8",
                crate::sheet::SHEET.as_bytes().to_vec()
            )
        );
        assert_eq!(
            found(&format!("/{INTERPRETER}")),
            (
                "text/javascript; charset=utf-8",
                dioxus_interpreter_js::INTERPRETER_JS.as_bytes().to_vec()
            )
        );
        for (path, script) in [
            (SCRIPT, crate::page::SCRIPT),
            (ENHANCES, crate::page::ENHANCES),
        ] {
            assert_eq!(
                found(&format!("/{path}")),
                ("text/javascript; charset=utf-8", script.as_bytes().to_vec()),
                "{path}"
            );
        }
    }

    /// Every file the design system ships is served under its own name,
    /// byte for byte, as the type its extension says — and every one has a
    /// type, so a release that ships a kind this list does not know is
    /// named here rather than answered 404 in a window.
    #[test]
    fn every_file_the_design_system_ships_is_served_as_itself() {
        assert!(
            noctua_design_dist::FILES.len() > 1,
            "the design system ships its files"
        );
        for (name, contents) in noctua_design_dist::FILES {
            let kind = type_of(name)
                .unwrap_or_else(|| panic!("the design system ships {name}, of no known type"));
            assert_eq!(
                found(&format!("/{DIST}{name}")),
                (kind, contents.as_bytes().to_vec()),
                "{name}"
            );
        }
    }

    /// A type is read from the last extension alone, and an extension
    /// outside the list has none.
    #[test]
    fn a_type_is_its_extensions() {
        assert_eq!(type_of("scope.css"), Some("text/css; charset=utf-8"));
        assert_eq!(
            type_of("tokens/css/ramp.css"),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(
            type_of("vendor/three-css3d.js"),
            Some("text/javascript; charset=utf-8")
        );
        assert_eq!(
            type_of("palettes/blue-balanced.json"),
            Some("application/json")
        );
        assert_eq!(type_of("favicon.svg"), Some("image/svg+xml"));
        for none in [
            "css",
            "scope",
            "scope.css.exe",
            "fonts/x.woff2",
            "x.CSS",
            "theme.scss",
            "module.mjs",
            "",
        ] {
            assert_eq!(type_of(none), None, "{none}");
        }
    }

    /// A path the list does not hold is not found, however it is written:
    /// nothing is decoded, joined or normalized, so no path escapes into a
    /// file — and a file the design system does not ship, the typefaces its
    /// sheet asks for among them, is not found either.
    #[test]
    fn no_path_outside_the_list_is_found() {
        for path in [
            "",
            "//",
            "/index.html",
            "/dist",
            "/dist/",
            "/dist/scope.css/",
            "/dist//scope.css",
            "/dist/./scope.css",
            "/dist/../sheet.css",
            "/dist/tokens/css/../../scope.css",
            "/dist/%2e%2e/sheet.css",
            "/dist/scope%2ecss",
            "/DIST/scope.css",
            "/dist/SCOPE.CSS",
            "/dist/\\scope.css",
            "dist/scope.css",
            "/../Cargo.toml",
            "/etc/passwd",
            "//etc/passwd",
            "/C:/Windows/win.ini",
            "/sheet.css/",
            "/sheet.css%00",
            "/dioxus/",
            "/dioxus/native.js",
            "/assets/fonts/NotoSans.woff2",
            "/dist/fonts.css?x",
            "/ ",
        ] {
            assert_eq!(
                answer("GET", path, DOCUMENT),
                Answered::NotFound,
                "{path:?}"
            );
        }
    }

    /// Only `GET` is answered with bytes; any other method is not allowed,
    /// whatever its path.
    #[test]
    fn only_get_is_answered() {
        for method in ["POST", "PUT", "DELETE", "HEAD", "OPTIONS", "get", ""] {
            for path in ["/", "/sheet.css", "/nothing"] {
                assert_eq!(
                    answer(method, path, DOCUMENT),
                    Answered::NotAllowed,
                    "{method} {path}"
                );
            }
        }
    }

    /// The response carries the answer's status, and the type of what was
    /// found; what was not found is sent with no body.
    #[test]
    fn a_response_carries_the_status_and_the_type() {
        let sent = response(answer("GET", "/sheet.css", DOCUMENT));
        assert_eq!(sent.status().as_u16(), 200);
        assert_eq!(
            sent.headers()
                .get(wry::http::header::CONTENT_TYPE)
                .and_then(|kind| kind.to_str().ok()),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(sent.body(), &Cow::Borrowed(crate::sheet::SHEET.as_bytes()));
        for (answered, status) in [(Answered::NotFound, 404), (Answered::NotAllowed, 405)] {
            let sent = response(answered);
            assert_eq!(sent.status().as_u16(), status);
            assert!(sent.body().is_empty());
        }
    }
}
