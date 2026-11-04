//! The other half of the type wall: proving it holds, and closing the one way
//! round it.
//!
//! `NOCTUA.md §15.4` describes one guarantee in two halves. The first is
//! `nc-log`'s `LogValue`, which is implemented for no string type at all —
//! `String`, `&str` and `&'static str` alike — because `String::leak`
//! manufactures a real `&'static str` out of anything. What carries a literal is
//! `nc_log::Literal`, whose field is private and whose constructor takes **no
//! value at all** — it reads an associated `const`, and const evaluation happens
//! before any run-time value exists. A macro matching a literal *token* does
//! not hold; `crates/nc-log/src/value.rs` says why. The second half is here:
//!
//! * **A compile-fail gate**, because acceptance criterion 2 asks for the wall
//!   to be proven "*by a compile-fail test, not by a comment*". It compiles each
//!   fixture that must not build and reads the failure.
//!
//!   **One fixture per shape, and the reason is not tidiness.** Four shapes in
//!   one file give three trait errors and nothing at all about the fourth — the
//!   shape the wall most depends on is the one its neighbours hide. Compiled
//!   separately, all four are refused and each names `LogValue`.
//!
//!   The fourth does not fail in *borrow* checking, which would not run once
//!   type checking has failed: compiled alone it answers `error[E0277]`, which
//!   is trait selection. What is true is the simpler thing — rustc stops reporting after enough errors
//!   in one crate, and a fixture that must fail for a *stated* reason has to be
//!   the only thing in its file that can fail.
//! * **A source gate**, because a value can be shaped into a literal's type
//!   before it reaches the call. §15.4 names this exactly: "a source gate
//!   additionally rejects `format!` and interpolation inside logging call sites,
//!   so the trait cannot be circumvented by pre-formatting". This one also
//!   refuses `.leak()` anywhere near a logging call; that `Literal` has no
//!   `from_macro` door to reach is the compile-fail fixture
//!   `a_leaked_string_cannot_reach_a_literal`.
//!
//! Either half alone is a rule. Together they are a guarantee.

use std::path::Path;
use std::process::Command;

use crate::gates::Finding;

/// The same-line marker that excuses a logging call, with a mandatory reason.
///
/// Same line only, for the reason the reference gate's is: rustfmt may move a
/// comment that sits above a statement, which would silently separate a marker
/// from what it excuses.
pub(crate) const ALLOW: &str = "allow-logging:";

/// The whole call beginning at `needle` on line `index`, comments removed.
///
/// **A call is not a line.** rustfmt breaks one past 100 columns, and a gate
/// that read a line saw `.record(` on one and the `format!` on the next. The
/// window ends where the call's parentheses balance, or at the end of the file
/// if they never do — an unbalanced call is not something to read past.
fn call_from(lines: &[&str], index: usize, needle: &str) -> Option<Read> {
    let first = without_comment(lines.get(index)?);
    let at = first.find(needle)?;
    let mut call = String::from(&first[at..]);
    // **Carried across the lines, because a string may be.** Scanned from a
    // fresh state on every line, the second half of a multi-line string reads
    // as code: a `)` in it closes the window early, the `format!` on the line
    // after is never read, and the call is still counted as inspected.
    let mut quoting = Quoting::default();
    let mut depth = balance_from(&first[at..], &mut quoting);
    let mut here = index;
    // A call this long is not a call; stopping is better than reading a file.
    while depth > 0 && here + 1 < lines.len() && here < index + LONGEST_CALL {
        here += 1;
        // A `//` inside an open string is not a comment either, so the comment
        // stripper is skipped while one is open.
        let next = if quoting.string.is_some() {
            lines[here]
        } else {
            without_comment(lines[here])
        };
        call.push(' ');
        call.push_str(next);
        depth += balance_from(next, &mut quoting);
    }
    Some(Read {
        // **Whether the window closed, which the caller has to know.** A call
        // longer than `LONGEST_CALL`, or one whose parentheses never balance,
        // returned as though it had been read whole is counted as inspected at
        // the call site. A gate that reports having read a call
        // it gave up on is the shape `§19.3` is about, one report along:
        //
        // ```text
        // log.record(&E {   +21 padding lines+   table: format!("{user}"), });
        //   -> findings=0 calls=1
        // ```
        whole: depth <= 0,
        text: call,
        ends: here,
    })
}

/// A call the window reached, and whether it reached the end of it.
struct Read {
    /// The call's text, comments removed.
    text: String,
    /// Whether the parentheses closed inside the window.
    whole: bool,
    /// The index of the last line the window read.
    ///
    /// The marker is tested on the line the needle is *on*, and without this the
    /// window has no lines — only their join.
    ends: usize,
}

/// Where a scan of one line left off.
///
/// **A string may span two lines, so the scan's state does too.** `call_from`
/// calls `balance_from` once per line, and with every call starting fresh the
/// second half of a multi-line string is read as code — a `)` in it closes the
/// window early, the `format!` on the line after is never read, and the call is
/// still counted as inspected:
///
/// ```text
/// log.record(&E {
///     note: "he said (stop
///           now)",
///     table: format!("{user}"),
/// });                              -> findings=0 calls=1
/// ```
///
/// That is the same `calls=1, findings=0` state the `whole` flag prevents,
/// reached from the other side: the window closes *successfully* on a call it
/// has not read.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Quoting {
    /// Inside a `"…"`, or inside a `r#"…"#` with this many hashes.
    string: Option<usize>,
    /// Inside a `'x'`.
    character: bool,
    /// The last character was a backslash inside a string.
    escaped: bool,
}

/// How much a piece of code opens minus how much it closes, continuing from
/// where a previous line left off.
///
/// Quotes are honoured, so a `(` inside a string literal does not hold the call
/// open — which is exactly the shape a logging call carries.
///
/// **A summary that stands on its own.** One written relative to a
/// neighbour's — "The same, continuing from where a previous line left off" —
/// loses its antecedent when that neighbour is folded in, and merged into the
/// block above it renders as that item's summary.
///
/// The two-number doc ratchet cannot see this shape: both items keep a block,
/// so blocks and bare are conserved.
fn balance_from(code: &str, quoting: &mut Quoting) -> i32 {
    let mut depth = 0;
    let mut in_a_char = quoting.character;
    let mut escaped = quoting.escaped;
    let mut raw = quoting.string;
    let mut quoted = quoting.string.is_some();
    let characters: Vec<char> = code.chars().collect();
    for (at, character) in characters.iter().enumerate() {
        match character {
            _ if escaped => escaped = false,
            // A raw string has no escapes: `r#"a \"# is a backslash and a
            // quote, and the string ends at `"#`.
            '\\' if (quoted && raw.is_none()) || in_a_char => escaped = true,
            // **A raw string ends at its own hash count, not at the next
            // quote.** `r#"let r = r#"a "b )" c"#;"#` unbalances by one a scan
            // in which every `"` flips the state.
            '"' if quoted && raw.is_some() => {
                let hashes = raw.unwrap_or_default();
                let closes = characters[at + 1..]
                    .iter()
                    .take(hashes)
                    .filter(|character| **character == '#')
                    .count();
                if closes == hashes {
                    quoted = false;
                    raw = None;
                }
            }
            '"' if !quoted && !in_a_char => {
                // `r"…"`, `r#"…"#`, `br#"…"#`: count the hashes behind it.
                let mut back = at;
                let mut hashes = 0;
                while back > 0 && characters[back - 1] == '#' {
                    hashes += 1;
                    back -= 1;
                }
                let opener = back > 0 && matches!(characters[back - 1], 'r' | 'R');
                quoted = true;
                raw = opener.then_some(hashes);
            }
            '"' if !in_a_char => quoted = false,
            // **A character literal holds parentheses too.** Honouring `"` and
            // not `'`, a scan closes the window at `delim: ')'` and never reads
            // a `format!` on the *next* line — with the call still counted as
            // inspected. Measured:
            //
            // ```text
            // log.record(&E {
            //     delim: ')',
            //     table: format!("{user}"),
            // });                              -> findings=0 calls=1
            // ```
            //
            // A lifetime is an apostrophe that opens nothing — `&'static str`
            // and `<'a>` are ordinary in these call sites — so what counts is
            // an apostrophe followed by a closing one within three characters,
            // which is every character literal and no lifetime.
            '\'' if !quoted => {
                if in_a_char {
                    in_a_char = false;
                } else {
                    in_a_char = closes_a_char(&characters[at..]);
                }
            }
            '(' if !quoted && !in_a_char => depth += 1,
            ')' if !quoted && !in_a_char => depth -= 1,
            _ => {}
        }
    }
    quoting.string = if quoted { raw.or(Some(0)) } else { None };
    quoting.character = in_a_char;
    quoting.escaped = escaped;
    depth
}

/// Whether an apostrophe opens a character literal rather than a lifetime.
///
/// `'x'`, `'\''` and `'\u{7f}'` are literals; `'static` and `'a` are lifetimes.
/// The difference is a closing apostrophe, and a literal's is within a few
/// characters — a lifetime has none at all.
fn closes_a_char(from: &[char]) -> bool {
    let mut at = 1;
    if from.get(at) == Some(&'\\') {
        // An escape: `'\''`, `'\n'`, `'\u{7f}'`. The closing quote is the next
        // apostrophe that is not itself escaped.
        at += 1;
        while at < from.len() && at < 12 {
            if from[at] == '\'' {
                return true;
            }
            at += 1;
        }
        return false;
    }
    from.get(at + 1) == Some(&'\'')
}

/// How many lines one call may span before this stops following it.
///
/// Twenty is far past anything rustfmt produces for a call, and it bounds the
/// walk on a file whose parentheses never balance.
const LONGEST_CALL: usize = 20;

/// Whether a needle applies to this file.
///
/// **One exemption, and it is the fixtures' own definition rather than a hole:**
/// `crates/nc-log/fixtures/` holds files whose entire purpose is to be the
/// attack, and a gate that reported them would be reporting its own proof.
///
/// **And no carve-out for `crates/nc-log/src/`.** The door the macros expand
/// to is `Literal::of::<S>()` reading an associated `const`, which takes no
/// value at all, so nothing there needs excusing — and a carve-out would hide
/// a reintroduced `from_macro` inside the one crate that could reintroduce it.
fn applies(needle: &str, file: &str) -> bool {
    let _ = needle;
    // **Widening this moves no printed number**, because `read` and the file
    // count both sit outside it: measured, one `&& !file.starts_with("crates/nc-journal/")`
    // and a real `Box::leak` in the journal leave every number —
    // `15`, `4`, `86`, `9`, `29` — byte-identical to a clean run. The exemption
    // is counted, and the count is held against the directory it is for.
    !file.starts_with(EXEMPT)
}

/// Whether the signature rule judges `file`.
///
/// **Its own predicate, because a test of [`applies`] cannot keep these
/// exclusions honest.** `every_crate_but_the_fixtures_is_judged` drives
/// [`applies`], which knows only [`EXEMPT`] — and `applies` discards its
/// needle, so a loop over `LEAKS` there repeats one assertion six times while
/// this rule has three exclusions. Measured, with a real
/// `pub fn remembered(text: String) -> &'static str` in `nc-journal`:
///
/// ```text
/// control                                       findings=["crates/nc-journal/src/lib.rs:830"]
/// + && !file.starts_with("crates/nc-journal/")  findings=[]
///   read=1911 either way, 316 passed
/// ```
///
/// One line, the crate out of the rule, and every printed number identical.
/// Here so a case per member can drive it.
pub(crate) fn the_signature_rule_judges(file: &str) -> bool {
    !file.starts_with(EXEMPT)
        // The crate the wall is *for*: `Literal` is built here, and the rule
        // would report the door it is written to be.
        && !file.starts_with("crates/nc-log/")
        // **And not the generated crate.** Every `&'static str` in
        // `crates/nc-catalog/src/` is a literal the emitter wrote out of the
        // registry — `message` hands back a row of `MESSAGES`, a table in the
        // same file — and what the emitter reads is gated by `determinism.rs`
        // rather than here. A marker cannot go on those lines: they are
        // regenerated, and `cargo xtask build` would take it straight back out.
        && !file.starts_with("crates/nc-catalog/src/")
}

/// The one directory the leak scan does not judge.
///
/// The compile-fail fixtures are files whose whole purpose is to hold the defect
/// rustc must refuse, so a scan that reported them would report its own evidence.
pub(crate) const EXEMPT: &str = "crates/nc-log/fixtures/";

/// Whether the signature at `index` carries a marker naming what it hands back.
///
/// **On the line above, not on the line itself.** The three sibling rules in
/// this file require the marker on the *same* line, and their reason is that
/// `cargo fmt` may move a comment that sits above a statement. A signature is
/// not a statement: rustfmt leaves a comment above an item where the author put
/// it, and a signature is often already near the column limit — appending to one
/// makes rustfmt reflow the line and carry the marker into the body, which is
/// what happens when it is written the other way.
///
/// **The reason must name the item**, so a marker about something else does not
/// excuse a signature beside it — `'static` is the one token every plausible
/// reason for such a function already holds, so a rule asking for it excuses
/// whatever the marker is written about. The body below carries the
/// measurement.
fn excuses_a_signature(lines: &[&str], index: usize) -> bool {
    // **The item, not the lifetime.** Requiring the reason to name `'static` —
    // the one token every plausible reason for such a function already holds —
    // excuses whatever the marker is written about.
    // Measured, in `crates/nc-prune/src/lib.rs`: a marker above
    // `pub fn probe(which: bool) -> &'static str` whose reason held the word
    // `'static` and said nothing else about the function gave
    // `ok … 88 source file(s) manufacture no static string …`, and the same
    // function with the marker gone is
    // `error … this function hands back a `&'static str` it was not given`.
    // Its three siblings in this file each require the reason to name the
    // *needle* — `from_crockford`, `OpaqueId::new` — for the reason
    // `no_identifier_is_minted_from_text` writes out: `cargo fmt` joins lines,
    // and this reads the line above as well as the line itself, so a marker
    // written about a neighbour lands here.
    //
    // What is being excused is this item, and its own name is what identifies
    // it. A reason that cannot name it is a reason that is not about it.
    let named = the_item_named_on(lines.get(index).copied().unwrap_or_default());
    let above = index.checked_sub(1).and_then(|above| lines.get(above));
    [lines.get(index), above].into_iter().flatten().any(|line| {
        crate::references::marked_in(line, ALLOW, crate::references::Comments::Slashes)
            && crate::references::reason_after(line, ALLOW, crate::references::Comments::Slashes)
                .is_some_and(|reason| {
                    named
                        .as_deref()
                        .is_some_and(|name| crate::references::names(reason, name))
                })
    })
}

/// The name a signature line declares, when it declares one.
///
/// `fn`, and the `static`/`const` bindings [`promotes_to_static`] also reads —
/// a binding whose type is a function returning a `&'static str` is the same
/// door, and it is excused the same way.
fn the_item_named_on(signature: &str) -> Option<String> {
    // **The visibility's own paren is a paren.** The head is cut at the first
    // `(`, which for `pub(crate) fn only_for(` is the one in `pub(crate)` — so
    // with the visibility left on, the head is `pub`, no keyword splits it, and
    // the item has no name a reason can be matched against: every
    // `pub(crate)`, `pub(super)` and `pub(in path)` item in the tree falls
    // outside the hatch it documents.
    // Measured, in `crates/nc-prune/src/postgres.rs`: `async fn only_for(`
    // under its existing and correct marker is
    // `check passed`, and the same line as `pub(crate) async fn only_for(` is
    // `error … this function hands back a &'static str it was not given`.
    // [`promotes_to_static`] strips exactly these, and both borrow the one
    // spelling `crate::gates` owns rather than each growing its own.
    let signature = crate::gates::without_a_visibility(signature.trim_start());
    // The declaration head only — up to the argument list or the type. Past
    // that lies `&'static str` itself, and `split_once("static ")` over the
    // whole line reads `str` out of it as the name of a `const` binding.
    let head = signature
        .find(['(', ':'])
        .map_or(signature, |at| &signature[..at]);
    ["fn ", "static ", "const "]
        .into_iter()
        .filter_map(|keyword| head.split_once(keyword))
        .map(|(_, rest)| {
            rest.trim_start()
                .chars()
                .take_while(|character| character.is_alphanumeric() || *character == '_')
                .collect::<String>()
        })
        .find(|name| !name.is_empty())
}

/// Whether this line declares a function that returns a run-time `&'static str`.
///
/// A `const fn` is excluded: its result is a compile-time value, so it cannot
/// carry anything a caller supplied. A trait method in a generated event struct
/// is why `nc-log` itself is outside the rule rather than outside this
/// predicate — the predicate is about the shape and the caller decides the
/// scope.
fn promotes_to_static(code: &str, aliases: &[String]) -> bool {
    // **A `static` or a `const` holding a function is one too.**
    // `pub static EIGHT: fn(String) -> &'static str = |text| …` hands back
    // exactly what a `fn` would and starts with neither `pub fn` nor `fn`.
    // **The visibility behind the one spelling `crate::gates` owns.** Four
    // hand-matched literal prefixes — `pub static `, `static `, `pub const `,
    // `const ` — cover `pub ` and bare and nothing else, while the `fn` branch
    // below strips every visibility Rust writes, in a loop: two shapes read by
    // one function need the rule for both. With the prefixes,
    // `pub(crate) static LEAK: fn(String) -> &'static str = …` is a door the
    // log's type wall cannot see — measured, one row above its own passing
    // control — while this file's own prose cites *this function* as the one
    // that strips those visibilities. `the_item_named_on` borrows the same
    // shared spelling.
    let held = crate::gates::without_a_visibility(code.trim_start());
    if (held.starts_with("static ") || held.starts_with("const ")) && !held.contains(" fn ") {
        // A binding whose *type* is a function returning one. `const fn` is
        // excluded by the `" fn "` test above, and is excluded for the reason
        // the caller's own comment gives: its result is a compile-time value.
        return code
            .split_once("->")
            .is_some_and(|(_, returns)| hands_back_a_static(returns, aliases));
    }
    let mut bare = code.trim_start();
    // `pub`, `pub(crate)`, `pub(in …)`, and `async` in either order — stripped
    // in a loop rather than once, because a signature carries as many of them as
    // it likes and `strip_prefix("pub ")` applied a single time misses
    // `pub(crate) async fn` and `pub async fn` alike.
    loop {
        let next = bare
            .strip_prefix("pub ")
            .or_else(|| bare.strip_prefix("async "))
            // `unsafe` and `extern "C"` are qualifiers like the two above, and
            // a function is no less a door for carrying one.
            .or_else(|| bare.strip_prefix("unsafe "))
            .or_else(|| {
                bare.strip_prefix("extern ")
                    .map(|rest| rest.split_once(' ').map_or(rest, |(_, rest)| rest))
            })
            .or_else(|| {
                bare.strip_prefix("pub(")
                    .and_then(|rest| rest.split_once(american_paren_space()))
                    .map(|(_, rest)| rest)
            });
        match next {
            Some(rest) => bare = rest.trim_start(),
            None => break,
        }
    }
    // **Anywhere in the return type, not the whole of it.** Matching the
    // literal `-> &'static str` lets `-> Option<&'static str>` and
    // `-> impl Iterator<Item = &'static str>` both walk through while handing
    // back the same thing — a `&'static str` the caller did not supply. What
    // decides is whether the type after the arrow mentions one at all.
    bare.starts_with("fn ")
        && code
            .split_once("->")
            .is_some_and(|(_, returns)| hands_back_a_static(returns, aliases))
}

/// Whether this return type hands back a `&'static str`, written or named.
///
/// **A `pub type` would overturn criterion 2.** A rule that reads the return
/// type as written lets a file that gives the type another name walk straight
/// through it — measured, probe and control in one file and one run in
/// `crates/nc-prune/src/lib.rs`:
///
/// ```text
/// pub type Forever = &'static str;
/// static KEPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
/// pub fn kept_aliased(text: String) -> Forever  { KEPT.get_or_init(|| text).as_str() }
/// pub fn kept_plain(text: String) -> &'static str { KEPT.get_or_init(|| text).as_str() }
/// ```
///
/// `kept_plain` was reported and `kept_aliased` was not; with the control taken
/// out the gate printed `88 source file(s) manufacture no static string` over a
/// function manufacturing one from its caller's `String`. The `LEAKS` needles do
/// not help — `OnceLock::get_or_init(…).as_str()` names none of them, which is
/// the shape this file's own doc records as needle-proof — and the argument
/// `signatures_joined` already makes about a wrapped signature is the same one:
/// "no list of paths holds a name the caller invented", and a return type is a
/// spelling too.
///
/// **One level, and one crate — not one file.** An `aliases` holding only what
/// the file being read declares misses that a `pub type` is visible to every
/// module of the crate that declares it.
/// Measured, one `pub type` and one `crate::Name` in a sibling module — no
/// contrivance, and both files inside the walk's own scope:
///
/// ```text
/// crates/nc-prune/src/lib.rs       pub type Forever = &'static str;
/// crates/nc-prune/src/postgres.rs  pub fn probe(text: String) -> crate::Forever
///   ->  ok  88 source file(s) manufacture no static string
///   control, the same signature written `-> &'static str`  ->  reported
/// ```
///
/// So `aliases` is the union over every source file of the member being
/// walked, and `an_alias_two_levels_deep_or_a_crate_away_is_a_stated_hole`
/// names what is still outside: an alias *of an alias*, and one imported from
/// another crate. Both are decisions rather than accidents, and each has a case
/// in that test saying so.
fn hands_back_a_static(returns: &str, aliases: &[String]) -> bool {
    returns.contains("&'static str")
        || aliases
            .iter()
            .any(|alias| crate::references::names(returns, alias))
}

/// The names this file gives to a type that holds a `&'static str`.
///
/// Read from the same text the signatures are read from, so a name written in a
/// comment or inside a string literal is not a declaration. The whole
/// declaration is joined the way [`signatures_joined`] joins a signature, and
/// for the same reason: rustfmt wraps a long one, and an alias whose
/// right-hand side sits on the next line is one alias however it is spelled.
///
/// **Containing one, not being one**, which is the width the rule beside it
/// already has: `-> Vec<&'static str>` is judged, so `type Names =
/// Vec<&'static str>;` and `-> Names` is the same signature under another name.
fn static_aliases(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut aliases = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        // The same qualifiers a signature carries, stripped the same way: an
        // alias is `pub`, `pub(crate)` or bare, and all three name a type.
        let mut bare = line.trim_start();
        loop {
            let next = bare.strip_prefix("pub ").or_else(|| {
                bare.strip_prefix("pub(")
                    .and_then(|rest| rest.split_once(american_paren_space()))
                    .map(|(_, rest)| rest)
            });
            match next {
                Some(rest) => bare = rest.trim_start(),
                None => break,
            }
        }
        let Some(rest) = bare.strip_prefix("type ") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let mut whole = rest.to_owned();
        let mut at = index + 1;
        while !whole.contains(';') && at < lines.len() && at - index <= SIGNATURE_LINES {
            whole.push(' ');
            whole.push_str(lines[at].trim());
            at += 1;
        }
        let declared = whole
            .split_once(';')
            .map_or(whole.as_str(), |(declared, _)| declared);
        // **What it is assigned, and not the line it is on.** A declaration
        // without a `=` is not an alias at all, and a name taken from a line
        // that merely opens with the word would carry whatever followed it.
        if declared
            .split_once('=')
            .is_some_and(|(_, assigned)| assigned.contains("&'static str"))
        {
            aliases.push(name);
        }
    }
    aliases
}

/// The names one source file gives to a type holding a `&'static str`.
///
/// The caller's half of the crate-wide union: it reads every file of a member
/// through here and hands the answer back to
/// [`nothing_manufactures_a_static_string`] for each of them. Through
/// [`readable`] for the reason that function's own header gives — a `type`
/// written in a comment or a raw string declares nothing.
pub(crate) fn static_aliases_in(text: &str) -> Vec<String> {
    static_aliases(&readable(text))
}

/// Which workspace member a source file belongs to, longest path first.
///
/// `xtask` is a prefix of `xtask/src/wall.rs` and `crates/nc-prune` of
/// `crates/nc-prune/src/lib.rs`; a member nested inside another would match
/// both, so the longest wins. The separator is required, or a member whose
/// directory name is a prefix of another's would claim that other's files.
pub(crate) fn member_of(members: &[String], file: &str) -> Option<String> {
    members
        .iter()
        .filter(|member| file.starts_with(&format!("{member}/")))
        .max_by_key(|member| member.len())
        .cloned()
}

/// `") "`, written where it can be seen.
pub(crate) const fn american_paren_space() -> &'static str {
    ") "
}

/// Each line of `text` joined with the rest of its function signature.
///
/// **A signature is not one line: rustfmt writes it across several.** Any
/// signature past 100 columns is wrapped, so the return type lands on a line
/// beginning `)` and `promotes_to_static` — which asks whether the *same* line
/// starts with `fn` and mentions `-> &'static str` — sees neither half on its
/// own. This tree holds many such signatures, so it is the house style the
/// formatter produces rather than anything contrived. Measured, in a
/// crate this rule reads:
///
/// ```text
/// type Keeper = Box<str>;
/// pub fn remembered_label_for_the_lifetime_of_this_process(
///     supplied: String,
///     _scope: usize,
/// ) -> &'static str { Keeper::leak(supplied.into()) }
///   ->  ok  88 source file(s) manufacture no static string …
///   ->  check passed
/// ```
///
/// The alias hides the leak needle and the wrap hides the signature; either one
/// alone is caught. The same alias is why a needle list cannot be the whole
/// rule: `Keeper::leak` renames the call, and no list of paths holds a name the
/// caller invented.
///
/// Returns one entry per line: that line, joined forward to the end of its
/// signature when it opens one, and otherwise itself.
fn signatures_joined(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut joined = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        // Only a line that opens a signature and does not close it needs
        // joining; everything else is itself, so a line's own meaning never
        // changes.
        let opens = line.contains("fn ") && !line.contains('{') && !line.contains(';');
        if !opens {
            joined.push((*line).to_owned());
            continue;
        }
        // **A `;` inside brackets does not end a signature.** Breaking at the
        // first one stopped the join at `digest: [u8; 32],`, so everything after
        // it — including the return type — was never read. Depth is counted over
        // `[`, `(` and `<`; a `;` at depth zero is the end of a declaration and
        // anywhere else is an array length or a slice pattern.
        let mut whole = (*line).to_owned();
        let mut depth = 0_i32;
        for following in lines.iter().skip(index + 1).take(SIGNATURE_LINES) {
            whole.push(' ');
            whole.push_str(following.trim());
            let mut ended = following.contains('{');
            for character in following.chars() {
                match character {
                    '[' | '(' => depth += 1,
                    ']' | ')' => depth -= 1,
                    ';' if depth <= 0 => ended = true,
                    _ => {}
                }
            }
            if ended {
                break;
            }
        }
        joined.push(whole);
    }
    joined
}

/// How many lines past its first a wrapped signature may run.
///
/// rustfmt puts one parameter per line, and a function with more than this many
/// is a different problem.
///
/// **The number decides, and so a test holds it.** Set to `0`, one token, a
/// planted `OnceLock` leak in `nc-canon` — rustfmt-wrapped, because the one-line
/// form is past a hundred columns — went from
///
/// ```text
///   error crates/nc-canon/src/lib.rs:549 — this function hands back a
///         `&'static str` it was not given
/// ```
///
/// to `ok 88 source file(s) manufacture no static string`, with every other
/// count unchanged. The wrapped signature is the ordinary case: the leak the
/// rule is written for takes a `String` and returns a `&'static str`, which is a
/// long signature by construction.
///
/// `the_number_of_lines_a_signature_may_wrap_over_is_the_written_one` restates
/// it, so changing it here means changing it twice.
const SIGNATURE_LINES: usize = 24;

/// A line with every `::<…>` removed.
///
/// A turbofish names a type and never a path segment, so taking it out leaves
/// the call spelled the way the needles are written: `Box::<str>::leak` becomes
/// `Box::leak`. Nested arguments are followed by depth, because
/// `Vec::<Box<str>>::leak` has a `>` that does not close the turbofish.
///
/// **The fifth walk in this file that counts angle brackets, and the one the
/// leak scan runs first.** `nothing_manufactures_a_static_string` strips the
/// turbofish before any of the six needles is consulted, precisely so
/// `Box::<str>::leak` cannot hide the call — so a walk that loses its place
/// here makes the whole scan blind for that line. It needs the shift guard the
/// other walks carry and the `->` guard this file writes three times: without
/// them `Vec::<[u8; 1 << 3]>::leak(v)` raises `depth` by two with one `>` to
/// lower it, the walk runs off the end of the line, and everything from `::` on
/// is dropped — the `Vec::leak` after it invisible to all six needles, while
/// `Vec::<[u8; 8]>::leak(v)` is reported. One shift apart, opposite answers.
///
/// **A character walk can tell a right shift from two closing angles.** A
/// guard symmetric in `>>` does trade `Vec::<[u8; 8 >> 1]>::leak(v)` for
/// `Box::<[Box<Box<str>>; 2]>::leak(v)`, which reads like a limit that wants a
/// parser — and a test pinning that as a hole **forbids** the repair while a
/// real `Vec::leak` goes unreported. It closes at no cost.
///
/// The trade is a property of a guard keyed on *adjacency*, not of character
/// walks. Brackets nest: a `>` inside `[`, `{` or `(` cannot close a `<` opened
/// outside that group, so remembering the angle depth at each bracket's opening
/// decides `8 >> 1`, `(1 >= 0) as usize`, `{ 1 > 0 }` and `A::<u8>::N >> 1`
/// while leaving the nested generic green. Measured: sixteen probes gained four
/// and lost none, and no gate reported anything new anywhere in the repository.
///
/// The one shape that could defeat it — an unbalanced bracket inside a literal,
/// `Vec::<[u8; b'[' as usize]>::leak(v)` — is unreachable: the caller runs
/// `readable` first, which blanks a literal's body while preserving its length,
/// so this walk never sees a bracket inside one.
///
/// **A claimed limit is stronger than an unexamined one and more dangerous
/// when wrong**, because the test that records it stops being a floor and
/// becomes a lock. So a hole is pinned as a hole only when the argument for it
/// has been attacked, and the pin is worded so that closing the hole fails
/// loudly.
fn without_type_arguments(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let characters: Vec<char> = line.chars().collect();
    let mut index = 0_usize;
    while index < characters.len() {
        if characters[index] == ':'
            && characters.get(index + 1) == Some(&':')
            && characters.get(index + 2) == Some(&'<')
        {
            let mut depth = 0_usize;
            let mut inner = 0_usize;
            // The angle depth each open bracket was reached at. Brackets nest,
            // so a `>` inside one cannot close a `<` opened outside it — which
            // is what tells a right shift from two closing angles without any
            // rule about adjacency.
            let mut bases: Vec<usize> = Vec::new();
            index += 2;
            while index < characters.len() {
                match characters[index] {
                    // A shift inside the type's own brackets is one token. It
                    // can only be there — an array length or a const argument —
                    // which is what `inner > 0` says.
                    // **The mirror of the `>` arm.** The `>` arm forgives any
                    // `>` a bracket group holds; a `<` arm forgiving only one
                    // adjacent to another `<` lets an ordinary comparison
                    // inside an array length —
                    // `Vec::<[u8; (1 <= 0) as usize]>::leak(v)` — raise a
                    // depth nothing lowers, and the walk runs off the line,
                    // hiding the call, while its own mirror, `(1 >= 0) as
                    // usize`, is green: a rule describing two shapes and
                    // implementing one. A `<` inside a bracket is the bracket's
                    // business, exactly as a `>` is.
                    '<' if inner > 0 => {}
                    '<' => depth += 1,
                    '[' | '{' | '(' => {
                        bases.push(depth);
                        inner += 1;
                    }
                    ']' | '}' | ')' => {
                        bases.pop();
                        inner = inner.saturating_sub(1);
                    }
                    // **A `>` inside a bracket cannot close an angle opened
                    // outside it.** This is what closes the right shift, the
                    // comparison in a const expression and every other `>` a
                    // type argument's own group holds — see this function's
                    // doc.
                    '>' if bases.last().is_some_and(|base| depth <= *base) => {}
                    // `->` is not a closing angle; this file says so in
                    // `closing`, in `the_type_ends` and in
                    // `the_return_type_ends`, and this is the fourth place
                    // that needs it: without it `Box::<fn() -> u8>::leak(v)`
                    // closes the turbofish at the arrow and hides the call.
                    '>' if index > 0 && characters[index - 1] == '-' => {}
                    '>' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            index += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                index += 1;
            }
            continue;
        }
        out.push(characters[index]);
        index += 1;
    }
    out
}

/// A line with its trailing comment removed, respecting string literals.
///
/// `split("//")` is not the rule: a URL in a string literal blinds such a gate
/// for the rest of the line — `let _e = "http://127.0.0.1:9428"; Box::leak(x)`
/// goes unseen. This crate's own default endpoint is a URL, so the carrier is
/// not hypothetical.
///
/// **And not a toggle on every `"`, which a character literal is one of.**
/// `let quote = '"'; // `.line(` writes one of these` leaves a toggling reader
/// believing a string is open, so the comment after it is read as code and the
/// gate opens a call window on it — `error crates/nc-log/src/lib.rs:393 — this
/// logging call is too long for the gate to read whole`, about a call that does
/// not exist. Measured.
///
/// So the literals are not read here at all. [`crate::gates::blanked`] is the
/// one reader in this crate that knows strings, raw strings, character literals
/// and nested block comments, and it answers a text whose *characters* line up
/// with the original — so a `"` that survives into it is a real delimiter, and
/// a `//` with an even number of them behind it is a real comment.
fn without_comment(line: &str) -> &str {
    let blanked = crate::gates::blanked(line);
    let mut shadow = blanked.chars();
    let mut quotes = 0_usize;
    for (at, character) in line.char_indices() {
        let Some(neutral) = shadow.next() else {
            break;
        };
        if neutral == '"' {
            quotes += 1;
        }
        if character == '/' && line[at..].starts_with("//") && quotes.is_multiple_of(2) {
            return &line[..at];
        }
    }
    line
}

/// The calls that manufacture a `&'static str` at run time.
///
/// **A needle list loses this race and is the second line, not the first.**
/// `String::leak` has been stable since Rust 1.72 and does exactly what
/// `Box::leak` does under another name; it walks straight through a list that
/// knows only `Box::leak`. The wall itself is the `Literal` newtype, whose
/// only constructor is a macro matching `$text:literal` — so a leaked value does
/// not reach a log field however it was spelled. This list is what still catches
/// the attempt at the source, where a reader can be told why.
///
/// **Named rather than written inline**, because this gate reads its own source
/// and a literal in a code position is a finding. A trailing marker on the `if`
/// does not survive: rustfmt moves a comment off the end of an `if` onto the
/// next line, which is exactly the hazard the same-line rule is about, met here
/// by the gate that documents it.
const LEAKS: [&str; 6] = [
    concat!("Box", "::leak"),
    concat!("String", "::leak"),
    concat!("Vec", "::leak"),
    // **The method call, not only the path form.** The measured attack is
    // `format!("…{email}").leak()`, and a list holding only the paths misses
    // the spelling the fixture beside it is named for.
    concat!(".lea", "k()"),
    concat!("into_boxed", "_str"),
    concat!("Box", "::into_raw"),
]; // allow-logging: the needles themselves

/// Nothing in the tree manufactures a `&'static str` at run time.
///
/// **A rule about every source file, not about the crates that may log.**
/// Inside [`no_formatting_at_a_logging_call`], whose scope is the crates that
/// may log, `pub fn forever(text: String) -> &'static str` wrapping
/// `Box::leak`, added to `nc-canon` and called from `nc-log`, is invisible.
/// `applies` says the rule is tree-wide (it excludes only the fixtures), and
/// so is the walk it sits in.
///
/// A leaked string is a real `&'static str`, which is why `Literal` does not
/// accept one — but the leak is still worth refusing wherever it is written,
/// because the next type to trust a `'static` lifetime will trust this one too.
///
/// Returns the findings and how many lines it read.
pub(crate) fn nothing_manufactures_a_static_string(
    file: &str,
    text: &str,
    elsewhere_in_the_crate: &[String],
) -> (Vec<Finding>, usize, bool) {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    // **The whole file through the stripper, not each line on its own.** A
    // block comment and a multi-line raw string both span lines, and reading
    // either as code makes this report a `Box::leak` in a *sentence about*
    // `Box::leak` — in a repository whose headers are long prose about the very
    // identifiers its gates refuse. `readable` carries that state; the marker is
    // still read from the original line, because a comment is where a marker
    // lives.
    // **What this file is exempt from, asked of `applies` itself.** Counting
    // `file.starts_with(EXEMPT)` at the call site leaves the check comparing one
    // constant with another, so widening `applies` moves nothing. This asks
    // the predicate, so a second `starts_with` inside it raises the number the
    // comparison holds against the directory.
    if LEAKS.iter().all(|needle| !applies(needle, file)) {
        return (findings, read, true);
    }
    let stripped = readable(text);
    let original: Vec<&str> = text.lines().collect();
    // Each line joined to the rest of its signature, so a wrapped one is judged
    // whole. Built once for the file rather than per line.
    let whole = signatures_joined(&stripped);
    // And the names this *crate* gives to a `&'static str`, for the same reason
    // and once for the same file. A `pub type` is visible to every module of the
    // crate that declares it, so the caller unions them; see
    // [`hands_back_a_static`] for the probe a read of one file misses.
    let mut aliases = static_aliases(&stripped);
    for alias in elsewhere_in_the_crate {
        if !aliases.contains(alias) {
            aliases.push(alias.clone());
        }
    }
    for (index, line) in stripped.lines().enumerate() {
        // **Counted where the verdict is, not at the top of the loop.** A line
        // floor over a counter bumped on entry cannot see a walk that reads every
        // line and judges none: measured, three lines added after an entry
        // increment make the scan skip everything but a file's first line, and
        // both the count and `check` stay byte-identical to a clean run with a
        // `Box::leak` planted in `nc-ids`. That is the accumulator shape
        // `xtask/canaries/README.md` records, in a second gate — so the counter
        // sits below every `continue`, and says what its name says.
        // **Through the literal scanner first.** A comment stripper that
        // toggles on every `"` and knows nothing of raw strings takes
        // `r#"a "b // c"#` for the start of a comment and blanks a `Box::leak`
        // on the same line — measured, `83 source file(s) manufacture no static
        // string` with one that does. `readable` is the scanner that does know,
        // and these lines are its output.
        // **And without a turbofish, because a type argument is not a spelling.**
        // The needles are paths, and `Box::<str>::leak(text.into())` is the same
        // call with `::<str>` between the type and the method — safe, ordinary,
        // `cargo fmt` and clippy-pedantic clean, and it walks past all six
        // needles: measured in `nc-canon`, with
        // `88 source file(s) manufacture no static string` printed over it, while
        // the control, the same line spelled `Box::leak(text.into_boxed_str())`,
        // was reported. A list of spellings cannot enumerate the type arguments Rust
        // allows, so the argument comes out before the list is consulted.
        let without_turbofish = without_type_arguments(line);
        let code = without_turbofish.as_str();
        if original.get(index).is_some_and(|raw| {
            crate::references::marked_in(raw, ALLOW, crate::references::Comments::Slashes)
                // **And the reason names the needle**, as the three sibling
                // hatches in this file already require. Accepting any non-empty
                // reason lets a marker about something else excuse a leak
                // beside it: measured, `// allow-logging: this module is about
                // JSON, not logging` on a `Box::leak` line gave `check passed`,
                // and removing only the marker reported it. `cargo fmt` joins
                // lines, so a marker that names nothing excuses whatever lands
                // next to it.
                // **And the needle that fired, not any needle this file could
                // hold.** Requiring the reason to name one of the six that
                // `applies` to the crate asks about the *file*: a marker whose
                // reason names `Box::leak` then excuses a `String::leak` on the
                // line it sits on, and a `Vec::leak` on the next one after
                // `cargo fmt` joins them.
                // Six spellings and one hatch is five wrong reasons a line can
                // carry. What is required is the spelling on that line.
                // **Every needle on the line, not one of them.**
                // `Box::leak(text.into_boxed_str())` — the canonical spelling,
                // and the one the control below is named for — carries two of
                // the six, so under `.any` a reason about the harmless one
                // excuses the leak: *"into_boxed_str only moves the allocation,
                // it leaks nothing"* is **true**, which is what makes it the
                // reason somebody would actually write. `.any` asks whether the
                // reason is about *something* the line does; what it has to be
                // about is the thing being excused.
                && crate::references::reason_after(raw, ALLOW, crate::references::Comments::Slashes).is_some_and(|reason| {
                    let mut on_this_line = LEAKS
                        .iter()
                        .filter(|needle| applies(needle, file))
                        .filter(|needle| code.contains(**needle))
                        .peekable();
                    on_this_line.peek().is_some()
                        && on_this_line
                            .all(|needle| crate::references::names(reason, needle))
                })
        }) {
            read += 1;
            continue;
        }
        // **The shape, not only the spellings.** A list of needles cannot name
        // a promotion that never says "leak": measured, in a crate this rule
        // reads,
        //
        // ```rust
        // static REMEMBERED: OnceLock<String> = OnceLock::new();
        // pub fn held(text: String) -> &'static str { REMEMBERED.get_or_init(|| text).as_str() }
        // ```
        //
        // — safe, `cargo fmt` and clippy-pedantic clean, and no needle names
        // it: `88 source file(s) manufacture no static string` is printed over
        // it. `crates/nc-log/src/lib.rs` records this shape too and answers it
        // by *typing* `Origin`, not by teaching the list.
        //
        // What the two have in common is not a call: it is a function that hands
        // back a `&'static str` it did not have when it started. A `const fn`
        // cannot — its result is a compile-time value — and `nc-log` itself is
        // where a static string is the currency, so the rule is about the other
        // crates.
        // **And the hatch this rule needs.** Judging a type *containing*
        // `&'static str`, not only a bare `-> &'static str` — which a leaked
        // string handed back as `Option<&'static str>` requires — brings in
        // functions that return collections of literals written in their own
        // source. Those are
        // ordinary Rust for "a list of names", and each carries a marker
        // naming what it hands back, in the shape the three sibling rules in
        // this file already use.
        if whole
            .get(index)
            .is_some_and(|signature| promotes_to_static(signature, &aliases))
            && the_signature_rule_judges(file)
            && !excuses_a_signature(&original, index)
        {
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: "this function hands back a `&'static str` it was not given".to_owned(),
                fix: "a `&'static str` is what the log's type wall accepts without \
                      question, so a function that produces one at run time is a door \
                      into it whatever it calls — a leak, a `OnceLock`, an arena, a \
                      lookup that happens to be exhaustive. Make it `const fn` if it \
                      returns a literal, return `String` if it does not, or mark the line \
                      `allow-logging: <reason>` naming what it hands back"
                    .to_owned(),
            });
            read += 1;
            continue;
        }
        if let Some(leak) = LEAKS
            .iter()
            .filter(|needle| applies(needle, file))
            .find(|needle| code.contains(*needle))
        {
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: format!("`{leak}` manufactures a `&'static str` at run time"),
                fix: "that is the one way past the log's type wall: a leaked `format!` \
                      satisfies `LogValue` and carries whatever was interpolated into a \
                      store that stays active in production (NOCTUA.md §15.4). If the \
                      value is a state, declare it as an `enum` field; if it is a count \
                      or a code, it already has a type"
                    .to_owned(),
            });
        }
        read += 1;
    }
    (findings, read, false)
}

/// `Literal` and `Variant` have no public constructor that takes a value.
///
/// **A fixture cannot forbid a function that does not exist yet.** The
/// compile-fail fixtures prove what rustc refuses *today*: a `String` is not a
/// `LogValue`, the tuple field is private, the macro will not take a non-literal,
/// a leaked value cannot reach a `const`. None of them says anything about a
/// *new* public constructor — measured by putting back the `from_macro` that
/// `crates/nc-log/src/value.rs` records `of::<S>()` replacing:
///
/// ```text
/// pub const fn from_macro(text: &'static str) -> Self { Self(text) }
///   -> let smuggled = nc_log::Literal::from_macro(email.leak());
///      println!("{}", smuggled.get());   ->  person@example.com
///   -> ok 11 fixture(s) refuse to compile, for the stated reason
/// ```
///
/// One line, no `unsafe`, no macro, and every fixture still green — because
/// every fixture is about a door that is already shut.
///
/// The rule is the shape, not the name: `of::<S>()` takes **no parameter**, and
/// that is what makes it safe (`S::TEXT` is a `const`). A public associated
/// function on either type that takes anything is a value going in at run time.
fn no_public_constructor_takes_a_value(
    file: &str,
    text: &str,
    compiled_into_the_log: bool,
) -> Vec<Finding> {
    /// The two types whose whole guarantee is that a caller cannot build one.
    const SEALED: [&str; 2] = ["Literal", "Variant"];

    // **What the crate compiles, not where the file sits.** A private tuple
    // field is in scope for every module of the crate, wherever its file
    // lives, so a test of `file.contains("nc-log/src/")` misses two ways in,
    // both measured with `check passed`:
    //
    // ```text
    // #[path = "../doors.rs"] pub mod doors;                     crates/nc-log/doors.rs
    // #[cfg_attr(all(), path = "../../nc-canon/src/x.rs")] …     crates/nc-canon/src/x.rs
    //
    //   pub fn remembered(text: String) -> Literal {
    //       Literal(HELD.get_or_init(|| text).as_str())          // a OnceLock, no `leak`
    //   }
    // ```
    //
    // The second is compiled into `nc-log` from another crate's directory, so no
    // filter written over paths can be right: what decides is the module tree,
    // and the caller already computes it — `scope_of` resolves the same
    // `#[path]` attributes to know which files may log at all.
    if !compiled_into_the_log {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for (line, signature, within) in segments(text) {
        // Only the ones that declare a function. A segment is every statement,
        // because the rule below it reads `static`, `const` and `type`
        // declarations out of the same walk.
        if !signature
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "fn")
        {
            continue;
        }
        // A `static` or `const` whose *type* is a function pointer declares no
        // function; it is `no_other_public_door`'s, and reporting it twice sends
        // a reader to one line from two rules.
        if is_public(&signature)
            && (signature.starts_with("pub static ")
                || signature.starts_with("pub const ") && signature.contains(": fn("))
        {
            continue;
        }
        // A public function anywhere in this crate whose answer is one of these
        // types, and which takes something. Where it is written does not matter
        // — an inherent method, a `From` impl, a trait method, a builder on
        // another type and a free function all hand a caller the same value.
        let public = is_public(&signature) || within.as_deref().is_some_and(is_a_trait_impl);
        if !public {
            continue;
        }
        let Some(returns) = signature.split_once("->").map(|(_, after)| after) else {
            continue;
        };
        let returns = returns.split(" where ").next().unwrap_or(returns);
        let kind = SEALED.iter().find(|kind| names(returns, kind)).or_else(|| {
            // `-> Self` is one of these two only when the block is theirs. In
            // `impl Value` it is a `Value`, and refusing that would make the
            // crate unwritable.
            names(returns, "Self")
                .then(|| within.as_deref().and_then(target_of))
                .flatten()
                .and_then(|target| SEALED.iter().find(|kind| **kind == target))
        });
        let Some(kind) = kind else { continue };
        if takes_nothing_but_itself(&signature) {
            continue;
        }
        findings.push(Finding {
            file: file.to_owned(),
            line,
            what: format!("this builds a `{kind}` out of something a caller can hand it"),
            fix: format!(
                "`{kind}::of::<S>()` takes no parameter, and that is the whole \
                 guarantee: the only thing it can read is `S::TEXT`, a `const`. A \
                 constructor with a parameter is a run-time value entering a type \
                 whose entire purpose is that none can (NOCTUA.md §15.4). The \
                 compile-fail fixtures cannot see this — they prove what rustc \
                 refuses today, and a new public function is not a refusal"
            ),
        });
    }
    findings.extend(no_public_field(file, text, &SEALED));
    findings.extend(no_other_public_door(file, text, &SEALED));
    findings
}

/// The doors that declare no function: a value, an alias, a deref target.
///
/// Three more shapes, each measured handing a caller a
/// `Literal` with no `pub fn` anywhere:
///
/// ```text
/// pub static MINT: fn(&'static str) -> Literal = Literal::of_text;
/// pub type Text = Literal;            // then `impl Text { pub fn made(…) }`
/// impl Deref for Wrapper { type Target = Literal; … }   // `*wrapper` is a Literal
/// ```
///
/// The last one works because `Literal` is `Copy`: a `&Literal` a caller can
/// obtain is a `Literal` a caller can have. An alias works because the rule
/// above resolves the type in an `impl` header by name, and `Text` is not one of
/// the two names.
fn no_other_public_door(file: &str, text: &str, sealed: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (line, declaration, within) in segments(text) {
        let public = is_public(&declaration)
            || within.as_deref().is_some_and(is_a_trait_impl) && declaration.contains("type ");
        if !public {
            continue;
        }
        // **The declaration without its attributes**, which is the text
        // `is_public` two lines up already reads. Read raw here, a single
        // `#[doc(hidden)]` makes a `pub type` alias public to one half of this
        // rule and invisible to the other — see [`without_attributes`].
        let bare = without_attributes(&declaration);
        let opens = bare.starts_with("pub static ")
            || bare.starts_with("pub const ")
            || bare.starts_with("pub type ")
            || bare.starts_with("type ");
        if !opens {
            continue;
        }
        let Some(kind) = sealed.iter().find(|kind| names(&declaration, kind)) else {
            continue;
        };
        findings.push(Finding {
            file: file.to_owned(),
            line,
            what: format!("this hands a caller a `{kind}` without declaring a function"),
            fix: format!(
                "a `static` or `const` of the type is a value a caller can copy; a \
                 `pub type` alias opens an inherent impl the constructor rule resolves \
                 by name and would not recognise; and a `Deref` whose target is \
                 `{kind}` gives a caller `*wrapper`, which is a `{kind}` because it is \
                 `Copy`. `{kind}::of::<S>()` is the only way in (NOCTUA.md §15.4)"
            ),
        });
    }
    findings
}

/// A public field is a constructor with no `fn` in it.
///
/// `pub struct Literal(pub &'static str)` is one character, it declares no
/// function, and it hands every caller `Literal(user_text.leak())`. The rule
/// above reads signatures and cannot see it; the compile-fail fixtures
/// keep compiling, because a tuple struct with a public field is exactly what
/// rustc is being asked to allow.
fn no_public_field(file: &str, text: &str, sealed: &[&str]) -> Vec<Finding> {
    let stripped = readable(text);
    let mut findings = Vec::new();
    for kind in sealed {
        let mut looking = stripped.as_str();
        while let Some(at) = names_at(looking, &format!("struct {kind}")) {
            let rest = &looking[at..];
            let body = rest.find([';', '}']).map_or(rest, |ends| &rest[..=ends]);
            if body.contains("pub ") {
                let line = stripped[..stripped.len() - looking.len() + at]
                    .lines()
                    .count();
                findings.push(Finding {
                    file: file.to_owned(),
                    line,
                    what: format!("`{kind}` has a public field, so anyone can build one"),
                    fix: format!(
                        "the field is the constructor. `{kind}(a string the caller leaked)` compiles \
                         the moment it is `pub`, and no fixture can see that happen — a \
                         tuple struct with a public field is valid Rust, so rustc has \
                         nothing to refuse. Keep the field private and let \
                         `{kind}::of::<S>()` be the only way in (NOCTUA.md §15.4)"
                    ),
                });
            }
            looking = &looking[at + 6..];
        }
    }
    findings
}

/// Every `fn` signature in a file, with the block header it sits inside.
///
/// **Line matching is the hole.** A rule of `code.strip_prefix("pub fn ")`
/// against one file is walked past twelve ways, measured: a free function, a
/// `From` impl, a trait method, a builder on another type, a `where` clause,
/// the brace on the next line, `-> Option<Self>`, a name split across lines,
/// and another file. The one that settles it is that `cargo fmt` *itself*
/// produces the split form as soon as a signature is long — so such a gate is
/// one formatter run away from blind, on code nobody wrote to evade it.
///
/// A signature is the text between the delimiter before it and its own `{` or
/// `;`, which is true however it is wrapped. Returned as `(line, signature,
/// enclosing header)` with every run of whitespace flattened to one space, so a
/// signature reads the same whatever the formatter did to it.
fn segments(text: &str) -> Vec<(usize, String, Option<String>)> {
    let stripped = readable(text);
    let mut found = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut segment = String::new();
    let mut at = 1_usize;
    let mut began = 1_usize;
    for character in stripped.chars() {
        match character {
            '\n' => {
                at += 1;
                segment.push(' ');
            }
            '{' | ';' | '}' => {
                let whole = flattened(&segment);
                if !whole.is_empty() {
                    found.push((began, whole.clone(), stack.last().cloned()));
                }
                match character {
                    '{' => stack.push(whole),
                    '}' => {
                        stack.pop();
                    }
                    _ => {}
                }
                segment.clear();
                began = at;
            }
            _ => {
                if segment.trim().is_empty() && !character.is_whitespace() {
                    began = at;
                }
                segment.push(character);
            }
        }
    }
    found
}

/// One file's source with everything that is not code blanked out.
///
/// **One pass, one state machine, because three patches are three holes.**
/// Stripping line comments, then blanking literals, then looking for `/*` in
/// what is left makes each step lie to the next. Measured, every one of them
/// `cargo fmt` clean:
///
/// ```text
/// /* a canonical string is wrapped in a " on the wire */
///     the odd quote blanked the comment's own `*/`, so every line below it
///     was swallowed and a `Box::leak` on the next one was invisible — to this
///     rule and to the four others that read through here, with no count moving
///
/// /* built by the macro */ before `pub const fn from_macro(&'static str)`
///     `is_public` saw the comment, not `pub`
///
/// /* the sealed type's own impl */ above `impl Literal {`
///     `target_of` saw the comment, so `-> Self` resolved to nothing
///
/// /* nested /* comments */ are legal Rust */
///     this closed at the inner `*/` and reported the prose after it as code
/// ```
///
/// So it is a scanner, and it knows what rustc knows: line comments, **nested**
/// block comments, strings with escapes, raw strings with any number of hashes,
/// byte and raw-byte strings, and character literals — with lifetimes, which
/// open like a character and never close. What comes back has the same number of
/// lines, so a finding still knows where it is.
fn readable(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0_usize;
    let mut depth = 0_usize;
    while at < characters.len() {
        let character = characters[at];
        if character == '\n' {
            out.push('\n');
            at += 1;
            continue;
        }
        // Inside a block comment, everything but a newline goes — and `/*`
        // opens another, because Rust's comments nest.
        if depth > 0 {
            if character == '/' && characters.get(at + 1) == Some(&'*') {
                depth += 1;
                out.push_str("  ");
                at += 2;
                continue;
            }
            if character == '*' && characters.get(at + 1) == Some(&'/') {
                depth -= 1;
                out.push_str("  ");
                at += 2;
                continue;
            }
            out.push(' ');
            at += 1;
            continue;
        }
        if character == '/' && characters.get(at + 1) == Some(&'*') {
            depth = 1;
            out.push_str("  ");
            at += 2;
            continue;
        }
        // A line comment: the rest of the line, whatever is in it.
        if character == '/' && characters.get(at + 1) == Some(&'/') {
            while at < characters.len() && characters[at] != '\n' {
                out.push(' ');
                at += 1;
            }
            continue;
        }
        // A raw string, with or without a `b`, with any number of hashes. The
        // delimiters stay so the line keeps its length and its shape.
        if let Some((from, hashes)) = raw_string_at(&characters, at) {
            at = blank_raw_string(&characters, at, from, hashes, &mut out);
            continue;
        }
        if character == '"' {
            out.push('"');
            at += 1;
            let mut escaped = false;
            while at < characters.len() {
                let inside = characters[at];
                if !escaped && inside == '"' {
                    break;
                }
                escaped = !escaped && inside == '\\';
                out.push(if inside == '\n' { '\n' } else { ' ' });
                at += 1;
            }
            if at < characters.len() {
                out.push('"');
                at += 1;
            }
            continue;
        }
        // A character literal, which is where `'"'` hid. A lifetime opens the
        // same way and never closes, so what makes it a literal is that it does
        // — within three characters.
        // **A character literal, with its escapes read.** A scan four
        // characters ahead for a closing quote that knows nothing about `\`
        // consumes `'\'` of `'\''` — one idiomatic line — and leaves a stray
        // quote that pairs with the next one in range, opening a string that
        // runs to the end of the file. Measured: a real `Box::leak` below it
        // invisible, *and* the body of a string constant above it exposed as
        // code, so the gate reported a sentence about `Box::leak` and hid the
        // call. The line count is preserved either way, so no counter moved. A
        // fixed four-character window does not recognise `'\u{22}'` as a
        // literal at all.
        if character == '\''
            && let Some(closes) = closing_quote(&characters, at)
        {
            out.push('\'');
            at += 1;
            while at < closes {
                out.push(' ');
                at += 1;
            }
            out.push('\'');
            at += 1;
            continue;
        }
        out.push(character);
        at += 1;
    }
    out
}

/// Copies a raw string's delimiters and blanks its body, answering where it ends.
///
/// Its own function because `readable` is at clippy's line limit, and because
/// this is the one branch with two cursors in it — the opener's characters are
/// copied through, and everything to the closer is blanked.
fn blank_raw_string(
    characters: &[char],
    mut at: usize,
    from: usize,
    hashes: usize,
    out: &mut String,
) -> usize {
    while at < from {
        out.push(characters[at]);
        at += 1;
    }
    let closes: Vec<char> = std::iter::once('"')
        .chain(std::iter::repeat_n('#', hashes))
        .collect();
    while at < characters.len() && !characters[at..].starts_with(closes.as_slice()) {
        out.push(if characters[at] == '\n' { '\n' } else { ' ' });
        at += 1;
    }
    let ends = (at + closes.len()).min(characters.len());
    while at < ends {
        out.push(characters[at]);
        at += 1;
    }
    at
}

/// Where a character literal opened at `at` closes, if it is one.
///
/// A lifetime opens the same way and never closes, so what makes it a literal is
/// that it does — and an escape is what decides how far away that is. `'a'` is
/// two characters, `'\''` and `'\\'` are three, `'\u{22}'` is seven, and
/// `&'static str` is none of them.
fn closing_quote(characters: &[char], at: usize) -> Option<usize> {
    let escaped = characters.get(at + 1) == Some(&'\\');
    if escaped {
        // `'\u{...}'` and `'\x41'` are as long as their escape; everything else
        // is one character after the backslash.
        let ends = if characters.get(at + 2) == Some(&'u') || characters.get(at + 2) == Some(&'x') {
            characters[at + 2..]
                .iter()
                .position(|c| *c == '\'')
                .map(|to| at + 2 + to)?
        } else {
            at + 3
        };
        return (characters.get(ends) == Some(&'\'')).then_some(ends);
    }
    // Unescaped: exactly one character, or it is a lifetime.
    (characters.get(at + 2) == Some(&'\'')).then_some(at + 2)
}

/// Where a raw string's body starts and how many hashes close it, if one opens
/// at `at`.
fn raw_string_at(characters: &[char], at: usize) -> Option<(usize, usize)> {
    // **`gates.rs`'s reader, not a second one.** The one in `gates.rs` knows
    // `r`, `br` and `cr`; a copy knowing only `r` and `br` takes `readable`
    // out of a C raw string at the bare quotes, the `//` in its body opens a
    // comment, and the rest of the line is blanked: measured, one line of
    // `cr#"a "b//c"#` in `nc-canon` left the static-string gate saying
    // `89 source file(s) manufacture no static string` with a `Box::leak` in
    // plain sight, and the same line written `r#` reported it.
    //
    // `readable` also feeds the logging rule, the identifier rule and the
    // fixture-subject walk, so one gap here is four gates wide. One reader,
    // and the absolute offset this caller wants is the width plus where it
    // started.
    let (width, hashes) = crate::gates::raw_string_at(characters, at)?;
    Some((at + width, hashes))
}

/// Every run of whitespace as one space, so a wrapped signature reads as one.
fn flattened(segment: &str) -> String {
    segment.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `text` names `word` as a word, not as part of a longer name.
///
/// `-> Option<Self>` names `Self` and `-> SelfDescribing` does not; `-> Literal`
/// names `Literal` and `-> LiteralParts` does not.
fn names(text: &str, word: &str) -> bool {
    names_at(text, word).is_some()
}

/// Where `text` first names `word` as a word, rather than as a substring.
///
/// **`names` answers the question and cannot say where.** A rule asking
/// `find(&format!("struct {kind}"))` instead — a raw substring — reports any
/// type in this crate whose name merely *begins* with `Literal` or `Variant` as
/// the sealed newtype having a public field. Measured, an ordinary documented
/// `pub struct VariantRendering { pub
/// quoted: bool }` in `crates/nc-log/src/value.rs`:
///
/// ```text
/// error crates/nc-log/src/value.rs:353 — `Variant` has a public field, so
///       anyone can build one
/// ---- wall::tests::the_crate_as_written_hands_a_caller_nothing ----
///       panicked: the crate as committed does not pass its own rule
/// ```
///
/// The message is self-refuting on its own subject — that struct is not a tuple
/// struct, its field is a `bool`, and `Variant::of::<S>()` is not its
/// constructor — and this rule carries no logging hatch of its own, so the only
/// way past it is to rename the type. A refusal of correct code with no way out
/// is what this file's siblings call the worse direction.
///
/// One rule, one place: `names` is this with the position thrown away.
fn names_at(text: &str, word: &str) -> Option<usize> {
    let mut from = 0_usize;
    while let Some(at) = text[from..].find(word) {
        let here = from + at;
        let before = text[..here].chars().next_back();
        let after = text[here + word.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if boundary(before) && boundary(after) {
            return Some(here);
        }
        from = here + word.len();
    }
    None
}

/// Whether an item's signature makes it public outside this crate.
///
/// **Not `starts_with("pub ")`.** A signature runs from the delimiter before it,
/// and an attribute is part of that text — so `#[must_use]`, `#[inline]` or
/// `#[doc(hidden)]` on the line above makes the signature start with `#[`, and a
/// `starts_with` rule skips the item. Measured: `#[doc(hidden)] #[must_use] pub const
/// fn from_macro(&'static str) -> Self` — verbatim the door `of::<S>()`
/// replaced — back in `value.rs`, `check passed`, and a crate outside the
/// workspace printing a caller's email out of a `Literal`.
///
/// `pub(crate)` and `pub(super)` are deliberately not public: the wall is about
/// what a caller outside can reach, and the crate needs a private constructor of
/// its own to build one at all.
fn is_public(signature: &str) -> bool {
    // **The attributes, not the last `]`.** Taking the text after the final
    // bracket lets any `[` `]` in the signature itself move the visibility out
    // of view: `pub fn first_of(texts: &[&'static str]) -> Self` leaves
    // `) -> Self`, which does not start with `pub`, and the rule skips a
    // public constructor of a sealed type entirely — measured, with a caller's
    // email printed out of a `Literal` from a crate outside the workspace and
    // `check passed`.
    //
    // An attribute is `#[` to its matching `]`, and only a run of them at the
    // start is skipped. Strings are already blanked by `readable`, so a `]`
    // inside `#[doc = "a]b"]` cannot end one early.
    without_attributes(signature).starts_with("pub ")
}

/// A declaration with the run of attributes in front of it removed.
///
/// **Its own function because two rules need it.** With [`is_public`]
/// stripping attributes and `no_other_public_door`, three lines below the call
/// to it, testing the *raw* text with `starts_with("pub type ")`,
/// `#[doc(hidden)] pub type Text = Literal;` is public to one half and not a
/// door to the other — measured, an alias opening an inherent `impl` on a
/// sealed type with every count *up* and `check passed`:
///
/// ```text
/// #[doc(hidden)]
/// pub type Text = Literal;
/// impl Text { pub fn made(text: &'static str) -> Self { Literal(text) } }
///
///   ok    0 crate(s) outside nc-log may log; 16 file(s) hold 16 logging call site(s)
///   check passed
/// ```
///
/// Deleting the one attribute made it a finding. The constructor rule cannot
/// see it either — `target_of("impl Text")` is `Text`, which is not a sealed
/// name — which is exactly what the alias buys.
fn without_attributes(signature: &str) -> &str {
    let mut rest = signature.trim_start();
    while let Some(after) = rest.strip_prefix("#[") {
        let mut depth = 1_i32;
        let mut ends = None;
        for (at, character) in after.char_indices() {
            match character {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        ends = Some(at + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(ends) = ends else { break };
        rest = after[ends..].trim_start();
    }
    rest
}

/// Whether a block header is a trait implementation, whose methods are public.
fn is_a_trait_impl(header: &str) -> bool {
    // **Past the attributes, because `is_public` two functions up already is.**
    // Read raw, `#[doc(hidden)] pub trait X` does not open with `pub trait `
    // and the clause below walks past it — and `#[doc(hidden)]` is precisely
    // what a door written to be invisible carries. `without_attributes` is this
    // file's helper for exactly this defect, with its own paragraph about a
    // `[` inside a signature moving the visibility out of view; a hand-written
    // copy here would be one rule written twice, the second copy narrower,
    // which is a shape this tree's own catalogue names. Measured with
    // `#[doc(hidden)] pub trait Minted` handing a caller a `Literal` and both
    // of the wall's lines byte-identical.
    let header = without_attributes(header).trim_start();
    // **A trait declaration's methods are as public as the trait is.** A method
    // written in `pub trait X { fn f(&self) -> Literal { … } }` carries no `pub`
    // of its own — its visibility is the trait's — and its enclosing header is
    // not an `impl … for …`, so asking only for that answers `false` and the
    // sealed-type rules walk past it. A default body is then a public
    // constructor of `Literal` that any type outside `nc-log` reaches by
    // writing `impl X for Mine {}`: measured, a consumer crate printed a
    // run-time string out of a `Literal` and out of `Origin.service` — the
    // field `NOCTUA.md §15.4` puts on every line the log emits — with both of
    // this step's lines byte-identical and the whole gate green.
    //
    // **The same rule, two thousand lines down.**
    // `the_routes_into_an_identifier` sets `through_a_trait` from
    // `line.starts_with("pub trait ")` and its comment says exactly this, for
    // `nc-ids`. Neither the compile-fail fixtures nor mutation can find its
    // absence here: a fixture proves what rustc refuses *today*, and a missing
    // clause is not a weakened one.
    (header.starts_with("impl") && header.contains(" for "))
        || header.starts_with("pub trait ")
        || header.starts_with("pub unsafe trait ")
}

/// The type a block header implements: after `for` if there is one, else after
/// `impl` and its generic parameters.
fn target_of(header: &str) -> Option<String> {
    let rest = header.strip_prefix("impl")?;
    let rest = rest.rsplit(" for ").next().unwrap_or(rest);
    let rest = rest.trim_start();
    // `impl<T> Literal` — the generic parameter list, if it is still here.
    let rest = if rest.starts_with('<') {
        let mut depth = 0_i32;
        let mut ends = 0_usize;
        for (index, character) in rest.char_indices() {
            match character {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        ends = index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        &rest[ends..]
    } else {
        rest
    };
    let name: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Whether a signature's parameter list holds nothing but a `self` receiver.
///
/// `pub fn clone(&self) -> Self` takes nothing a caller chose, and reporting it
/// is the rule's own false positive: every `Clone` in the crate.
fn takes_nothing_but_itself(signature: &str) -> bool {
    let Some((_, after)) = signature.split_once('(') else {
        return true;
    };
    let mut depth = 1_i32;
    let mut parameters = String::new();
    for character in after.chars() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        parameters.push(character);
    }
    parameters
        .split(',')
        .map(str::trim)
        .filter(|parameter| !parameter.is_empty())
        .all(|parameter| {
            matches!(
                parameter,
                "self" | "&self" | "&mut self" | "mut self" | "self: Self"
            )
        })
}

/// The same two constructors, spelled so they can be read off a whole file.
///
/// **Not `MINTED_FROM_TEXT`.** Inside a logging call's parentheses `::new(`
/// is narrow enough to mean one thing; over a whole file it is
/// `BTreeMap::new()`, `String::new()`, `Vec::new()` — measured, four
/// findings in `nc-log`'s own source and fixtures. The
/// window rule keeps the bare form and still catches an alias at the call
/// site; an alias away from a call is what this one gives up.
///
pub(crate) const AWAY_FROM_A_CALL: [&str; 3] = [
    "from_crockford",
    concat!("OpaqueId", "::new"),
    // Spelled with its type, because away from a call `::parse` is every
    // parser there is. The route is two hops — `FileName::parse(text)` then
    // `FileName::id()` — and it is the first hop that takes the text, so it
    // is the hop to name.
    concat!("FileName", "::parse"),
];

/// Nothing in a crate that may log turns text into an identifier, anywhere.
///
/// **The window is the hole.** The rule below refuses `from_crockford(` and
/// `::new(` inside a logging call's parentheses, and it is walked past eight
/// ways, measured — a `let` on the line immediately above the call;
/// a helper in the same file, in another module, in another crate, behind a
/// trait, behind a generic, behind a closure. Eighteen call sites carrying the
/// same twelve caller-chosen characters, every one reported as `build no text`:
///
/// ```text
/// {"_msg":"journal.appended",…,"work_unit":"CPF123456789"}
/// ```
///
/// So the scan is the file, not the call. That closes the shapes that need no
/// second file, which are the cheap ones.
///
/// **What it does not close, and what would.** A helper in another crate is
/// still invisible — and a crate that does not itself depend on `nc-log` is
/// outside this scope entirely. `clippy.toml`'s `disallowed-methods` would reach
/// it by resolved path, and measured, it flags 43 ordinary uses in `nc-ids` and
/// `nc-journal`, where these constructors are how an identifier is read back
/// from a database. The rule cannot be spelled as "this method, anywhere".
///
/// The close that works is a type: a minted-only `Snowflake` that
/// `Minter::mint` alone produces, with `LogValue` implemented for that and not
/// for one a caller can spell out of text. That is a change to `nc-ids`'
/// public surface and it is written down here rather than made in passing.
fn no_identifier_is_minted_from_text(file: &str, text: &str) -> Vec<Finding> {
    //
    // **And without the parenthesis.** With it, the needle is the *call* form
    // and a constructor is a value in Rust: `let mint = Snowflake::from_crockford;`
    // then `mint(text)` passes, and so do `Snowflake::from_crockford::<_>(text)`,
    // `use ...from_crockford as mint;` and a `const fn` pointer. Four spellings,
    // one character. Dropping the parenthesis reaches every one of them, because
    // all four have to name the constructor somewhere.
    let mut findings = Vec::new();
    // Through the same stateful stripper as the leak scan: a block comment and a
    // multi-line raw string both span lines, and a sentence *about*
    // `from_crockford` inside either is prose. Measured, in this crate's own
    // `value.rs`: a `pub const NOTE: &str = r"… read back … with from_crockford
    // …"` reported as a mint.
    let stripped = readable(text);
    let original: Vec<&str> = text.lines().collect();
    for (index, code) in stripped.lines().enumerate() {
        let line = original.get(index).copied().unwrap_or_default();
        for minted in AWAY_FROM_A_CALL
            .iter()
            .filter(|needle| code.contains(*needle))
        {
            // **The reason has to name what it excuses**, which is the rule
            // `nc:narrows:` already follows. A bare same-line marker is a
            // blanket excuse for whatever else is on the line — and `cargo fmt`
            // *joins* lines: a marker written about `.ok()` on its own line
            //
            // ```text
            // nc_ids::Snowflake::from_crockford(text)
            //     .ok() // allow-logging: the `ok` discards the parse error
            // ```
            //
            // becomes one line under the formatter, and then excuses the mint.
            // The gate requires `cargo fmt --check` to pass, so the formatter is
            // guaranteed to run. Naming the needle makes an excuse that arrived
            // by reformatting fail rather than widen.
            if crate::references::marked_in(line, ALLOW, crate::references::Comments::Slashes)
                && crate::references::reason_after(
                    line,
                    ALLOW,
                    crate::references::Comments::Slashes,
                )
                .is_some_and(|reason| crate::references::names(reason, minted))
            {
                continue;
            }
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: format!("`{minted}` turns text into an identifier in a crate that may log"),
                fix: format!(
                    "a declared field type is not a guarantee when the value is built out \
                     of text — `Snowflake::from_crockford` takes twelve caller-chosen \
                     characters and writes them to the store verbatim, which is what \
                     `type = \"id\"` is refused for. The value need not be built at the \
                     call: a `let` on the line above, or a helper in this file, reaches \
                     the same field. Log an identifier the platform minted. If this one \
                     cannot carry caller text, say so with `allow-logging:` on this line \
                     — and name `{minted}` in the reason, because `cargo fmt` joins lines \
                     and a marker that names nothing excuses whatever lands beside it"
                ),
            });
        }
    }
    findings
}

/// What "building text" is spelled as at a call site.
///
/// **Not three needles, because a caller has eight other ways.** With only
/// `format!(`, `.to_string()` and `.to_owned()`, `String::from(subject)` and
/// `line.push_str(subject)` are ordinary code to this gate. `§15.4` asks for a
/// gate rejecting pre-formatting at a logging call site, not one rejecting
/// three spellings of it.
///
/// `+` is deliberately absent: it is arithmetic far more often than it is
/// concatenation, and a needle that fires on every sum is a needle somebody
/// turns off.
const BUILDS_TEXT: [&str; 15] = [
    "format!(",
    ".to_string()",
    ".to_owned()",
    concat!("String", "::from("),
    ".push_str(",
    "write!(",
    ".join(",
    ".replace(",
    ".concat()",
    // **A longer name is not the shorter one.** Every needle here is matched as
    // a substring, so a spelling whose name merely *begins* with one is
    // invisible: `writeln!(` does not hold `write!(`, and
    // `String::from_utf8_lossy(` does not hold `String::from(`. Both build text
    // and both are unseen by the shorter needle.
    "writeln!(",
    concat!("String", "::from_utf8_lossy("),
    // **And the same argument applied to the rest of the list.** `.replacen(`
    // is `str::replacen`, it returns a `String`, and `.replace(` is not a
    // substring of it. The other eight needles, walked the same way and each
    // measured against the tree: `String::from_utf8(`, `from_utf16(`,
    // `from_iter(` and `from_utf8_unchecked(` are the same shape and are
    // **written nowhere in this repository**, so a needle for each would be a
    // rule with no floor — they are named here instead, for the reader who adds
    // one. Two more are refused deliberately: `format_args!(` returns
    // `Arguments` and builds no `String` (`nc-policy`'s crate root writes
    // it), and `.to_ascii_uppercase()` is `char`'s in two of the three places
    // this tree writes it — in `nc-ids`' base32 reader and `nc-journal`'s entry
    // — and returns a `char`. A needle for either
    // refuses a call that builds nothing.
    ".replacen(",
    // **And the same argument a third time, for the spelling it came from.**
    // "A longer name is not the shorter one" holds for `.to_string()` as it
    // does for `write!(` and `String::from(`: `.to_string_lossy()` does not
    // contain it, turns a caller-chosen path into text, and is written three
    // times in this tree — so a needle for it has the floor that the four absent
    // `String::from_*` spellings lack. `.to_lowercase()` and `.to_uppercase()`
    // on `str` return a `String` by the same argument and are written here too.
    ".to_string_lossy(",
    ".to_lowercase()",
    ".to_uppercase()",
]; // allow-logging: the needles themselves

/// Where the fixtures live.
const FIXTURES: &str = "crates/nc-log/fixtures";

/// Runs every compile-fail fixture and checks each one fails for its reason.
///
/// Returns how many were run, so a directory that lost its fixtures is visible
/// rather than green — `NOCTUA.md §19.3`'s first rule.
///
/// # Errors
///
/// Returns why a fixture could not be compiled at all, which is different from a
/// fixture that compiled when it should not have.
pub(crate) fn compile_fail(root: &Path) -> Result<(Vec<Finding>, usize, usize), String> {
    let directory = root.join(FIXTURES);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Err(format!(
            "{FIXTURES} could not be read, so the wall's compile-fail proof did not run. \
         Acceptance criterion 2 asks for it by name"
        ));
    };
    let mut fixtures: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    fixtures.sort();

    let mut findings = Vec::new();
    // **Counted where a fixture reaches its verdict, not as `fixtures.len()`.**
    // With `inspected_all`, the printed line and
    // `the_wall_holds_and_the_proof_actually_ran` all reading a count taken
    // before anything is compiled, nothing counts what reaches a verdict — and
    // a `continue` in this loop body turns acceptance criterion 2 off with
    // `11 fixture(s) refuse to compile` and the test count byte-identical to a
    // clean run, while `impl LogValue for String` ships. Measured, exactly that
    // way.
    let mut judged = 0;
    // **How many controls actually decided.** `judged` counts fixtures, and a
    // fixture is judged whether or not the empty-crate control ran — so a
    // sentence printed from `judged` claims a rebuild for eleven while the
    // control has been asked of seven, and no number in the run moves. A gate that reports a
    // count must report the count of the thing it is claiming.
    let mut decided: Vec<Outcome> = Vec::new();
    for fixture in &fixtures {
        let name = fixture
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let output = build_fixture(root, fixture)?;
        let output_kind = output.clone();
        match output {
            Outcome::Compiled => findings.push(Finding {
                file: crate::relative_to(root, fixture),
                line: 1,
                what: format!("`{name}` compiled, and it must not"),
                fix: "the type wall is what stops a dynamic string reaching a log that \
                  stays active in production (NOCTUA.md §15.4). If `LogValue` gained \
                  an implementation for `String`, `&str` or anything they deref to, \
                  that is the defect — this fixture is the alarm"
                    .to_owned(),
            }),
            // Failing is what it is for — but failing for the *wrong* reason is
            // a fixture that has quietly stopped proving anything. A typo in it
            // would fail to compile just as reliably.
            Outcome::FailedWithout(message) => findings.push(Finding {
                file: crate::relative_to(root, fixture),
                line: 1,
                what: format!("`{name}` did not fail for the reason it declares"),
                fix: format!(
                    "a fixture that fails for an unrelated reason still passes a naive \
                 check, and then proves nothing. rustc said: {}",
                    // **Three lines of a one-line message is no bound at all.**
                    // `--message-format=json` writes each diagnostic on a single
                    // line, so `take(3)` took all of it — and this field carried
                    // two kilobytes of escaped JSON, rustc's whole explanation of
                    // E0425 included, into a field whose contract in `gates.rs`
                    // is "what to do about it. Never a restatement of `what`".
                    what_rustc_said(&message)
                ),
            }),
            Outcome::WouldFailWithoutTheCrate => findings.push(Finding {
                file: crate::relative_to(root, fixture),
                line: 1,
                what: format!("`{name}` fails the same way with no `nc-log` behind it"),
                fix: "the sentence this fixture declares came back when it was built \
                      against an empty crate of the same name, so the wall is not what \
                      refused it and the fixture proves nothing about the wall. Two \
                      lines satisfy `cannot call non-const method` with no log in \
                      sight. Write the refusal so it needs the surface it is named \
                      for — an `impl nc_log::Source`, a `nc_log::literal!`, a call \
                      that takes a `LogValue` — and it will stop resolving rather \
                      than stop const-evaluating"
                    .to_owned(),
            }),
            // **Not a finding, and not silence either.** The skip is already
            // announced on the line above; what this stops is the `ok` sentence
            // claiming a rebuild that did not happen. The tally lives in
            // `controlled_by` so that merging these two arms — the natural way
            // to write them — cannot make a control that did not run count as
            // one that did.
            Outcome::FailedCorrectly | Outcome::ControlDidNotRun => {}
        }
        judged += 1;
        decided.push(output_kind);
    }
    let controlled = controlled_by(&decided);
    crate::gates::everything_reached_a_verdict(
        "fixture",
        FIXTURES,
        fixtures.len(),
        judged,
        &mut findings,
    );
    Ok((findings, judged, controlled))
}

/// The verdict a control's answer becomes.
///
/// **`Some` only when the control decided**, and the boolean carried through.
/// A hand-written match beside the one `answer` already holds is one token from
/// `None => Outcome::FailedCorrectly`, which makes a control that never ran
/// score as one that did, with every other test in this module green —
/// measured.
fn outcome(answer: Option<bool>) -> Outcome {
    match answer {
        Some(true) => Outcome::WouldFailWithoutTheCrate,
        Some(false) => Outcome::FailedCorrectly,
        None => Outcome::ControlDidNotRun,
    }
}

/// How many of these verdicts came from a control that actually ran.
///
/// **The tally, separated from the reporting loop.** Merging
/// `Outcome::FailedCorrectly` and `Outcome::ControlDidNotRun` into one arm is
/// the natural way to write two arms that do nothing — and it makes
/// `controlled == checked == 11` over eleven rebuilds that never happened,
/// measured with the gate printing that each had been rebuilt directly beneath
/// eleven lines saying the control could not be asked.
fn controlled_by(outcomes: &[Outcome]) -> usize {
    outcomes
        .iter()
        .filter(|outcome| **outcome == Outcome::FailedCorrectly)
        .count()
}
/// A crate of the same name declaring the same doors, with no guarantee behind it.
///
/// **A stub, not an empty crate.** Against an empty crate `only_resolution` is
/// true for every fixture and the control decides none of them — an announced
/// skip per fixture in every run, on every machine. Measured per fixture
/// against empty crates: `E0405`, `E0422`, `E0425`, `E0433` and nothing else.
///
/// With the guarantee removed rather than the crate, **every fixture compiles**
/// — its refusal really is the wall's, and the control says so.
///
/// **The shape of each door, not only its name.** A stub that declares a
/// door's *name* is not a stub that declares its *shape*: with `Field` written
/// as a struct where the real one is an enum, `Origin` carrying `file`/`line`
/// where the real one carries `version`, `Event` a struct where the real one is
/// a sealed trait, and `Shipper` without `start`, four hollow builds died at
/// `E0599`, `E0560` and `E0404` — none of them a resolution failure, so nothing
/// announced — and were scored as though the control had decided them.
///
/// `nc-catalog` is stubbed too: `an_outside_crate_cannot_be_an_event` names `nc_catalog::Level`
/// twice, because `Event::level` returns it.
fn stub_for(name: &str, fixture: &str) -> String {
    if name == "nc-catalog" {
        // **Not empty.** The seal fixture names `nc_catalog::Level` twice,
        // because `Event` does, so against an empty `nc-catalog` the hollow
        // build dies at `E0433` before reaching the seal it exists to test.
        return "#![allow(missing_docs, unreachable_pub, dead_code)]\n\
                pub enum Level { Info }\n"
            .to_owned();
    }
    if name != "nc-log" {
        return String::new();
    }
    let (origin, open) = the_origin_a_stub_declares(fixture);
    let (origin, open) = (origin.as_str(), open.as_str());
    concat!(
        "#![allow(missing_docs, unreachable_pub, dead_code)]\n",
        "pub trait LogValue {}\n",
        "impl<T> LogValue for T {}\n",
        "pub mod sealed { pub trait Sealed {} impl<T> Sealed for T {} }\n",
        "pub trait Source { fn text() -> &'static str; }\n",
        "pub struct Literal(pub &'static str);\n",
        // `from_macro` is public here and does not exist in the real crate:
        // that absence *is* the guarantee, so a stub that has the door is
        // what the fixture must compile against.
        "impl Literal { pub fn of<S: Source>() -> Self { Self(S::text()) }\n",
        "  pub const fn from_macro(t: &'static str) -> Self { Self(t) }\n",
        "  pub const fn get(self) -> &'static str { self.0 } }\n",
        "#[macro_export] macro_rules! literal { ($t:expr) => { $crate::Literal($t) } }\n",
        // **Each of these mirrors the real item's *shape*, not just its name.**
        // With a name apiece, four fixtures die at `E0599`, `E0560` and
        // `E0404` — the door is declared and cannot be opened — and the control
        // is asked of seven of eleven while `check` prints eleven, measured.
        //
        // `Field` is an enum whose `Id` is private in the real crate; `Origin`
        // takes a `Literal` where this takes a `&'static str`; `Event` is
        // sealed there and bare here; `Shipper` is private there and public
        // here. Each difference *is* the guarantee, which is why the stub has
        // to be the same shape without it.
        "pub enum Field { Id(String), Text(&'static str) }\n",
        // **Generic over both fields, because two fixtures need two types.**
        // A stub keeping `version: Literal` while `service` loses its
        // guarantee makes the `version` fixture impossible: a fixture that
        // puts a run-time string in `version` still fails against a stub that
        // types it `Literal`, so the control answers "this proves nothing"
        // about a fixture that proves something. A field with no guarantee at
        // all is a type variable — it accepts the `Literal` the one fixture
        // hands it and the `&'static str` the other does.
        "{origin}",
        // **In the module a consumer reaches it through, not only at the root.**
        // The real crate's root holds a *private* `use ship::Shipper;`, so
        // `nc_log::Shipper` is `E0603` whatever `ship.rs` declares — a fixture
        // naming that path is baselined where it cannot fail. The stub is
        // the same shape without the guarantee, so its `Shipper` lives in `ship`
        // with its two methods and is re-exported at the root.
        "pub mod ship { pub struct Shipper;\n",
        "  impl Shipper { pub fn start(_at: &str) -> Result<Self, String> { Ok(Self) }\n",
        "    pub fn send(&self, _line: String) {} }\n",
        "  pub const DEFAULT_ENDPOINT: &str = \"\"; }\n",
        "pub use ship::Shipper;\n",
        "pub trait Event { fn name(&self) -> &'static str;\n",
        "  fn level(&self) -> nc_catalog::Level;\n",
        "  fn fields(&self) -> Vec<(&'static str, Field)>; }\n",
        "pub struct Variant(pub &'static str);\n",
        "pub struct Log;\n",
        "{open}",
    )
    .replace("{origin}", origin)
    .replace("{open}", open)
}

/// The `Origin` a hollow crate declares, and the `Log::open` that takes it.
///
/// **A stub that removes *every* guarantee at once lets a forgery survive the
/// control.** With `Origin` generic over both fields, a fixture named for
/// `version` that in fact attacks `service` compiles against the stub exactly
/// as the honest one does — and the control's question is whether the fixture
/// still refuses without the guarantee. Measured: the `version` fixture given
/// the `service` fixture's body, paying for its token with a string literal,
/// left `check passed` and a transcript
/// **byte-identical** to a clean run, with `Origin::version` demoted from
/// `Literal` to `&'static str` and a consumer crate putting a tenant's address
/// in the `version` field of every line the log emits.
///
/// So each row's stub removes **only that row's** guarantee and keeps the
/// other. A fixture that attacks the field it is named for still compiles here;
/// one that attacks the neighbour does not, and the control says so.
///
/// Two owned strings rather than two `&'static str`, because this crate's own
/// type-wall sweep refuses a function that hands back a static string it was
/// not given — and it is right to: that is the shape a run-time value is
/// laundered into a log line through. Nothing here is run-time, and paying the
/// allocation is cheaper than an excuse.
fn the_origin_a_stub_declares(fixture: &str) -> (String, String) {
    match fixture {
        "an_origin_cannot_hold_a_runtime_string" => (
            "pub struct Origin<S> { pub service: S, pub version: Literal }\n".to_owned(),
            "impl Log { pub fn open<S>(_o: Origin<S>, _at: &str) -> Result<(), String> { Ok(()) } }\n".to_owned(),
        ),
        "an_origin_version_cannot_hold_a_runtime_string" => (
            "pub struct Origin<V> { pub service: Literal, pub version: V }\n".to_owned(),
            "impl Log { pub fn open<V>(_o: Origin<V>, _at: &str) -> Result<(), String> { Ok(()) } }\n".to_owned(),
        ),
        // Every other fixture names neither field, so neither guarantee is what
        // its refusal is about and both are removed as before.
        _ => (
            "pub struct Origin<S, V> { pub service: S, pub version: V }\n".to_owned(),
            "impl Log { pub fn open<S, V>(_o: Origin<S, V>, _at: &str) -> Result<(), String> { Ok(()) } }\n".to_owned(),
        ),
    }
}

/// Every error rustc reported, as rustc phrased it.
///
/// **The `message` field of each diagnostic, and nothing else from the line.**
/// `cargo build --message-format=json` writes one JSON object per line to
/// stdout; the compiler's own sentence is the `message` of the `message`
/// object, and the `rendered` field beside it is the same text with the
/// offending *source* pasted under it. Reading `rendered` — or the plain-text
/// build output, which is `rendered` concatenated — lets a fixture supply its
/// own expected reason by writing that reason into the code that fails:
///
/// ```text
/// //! EXPECT: the trait bound `String: LogValue` is not satisfied
/// fn main() {
///     undefined_helper("the trait bound `String: LogValue` is not satisfied");
/// }
/// ```
///
/// `error[E0425]: cannot find function` — and a gate reading `rendered` counts
/// it as proof that a `String` cannot be a log value. Changing *which* string
/// is looked for leaves the fixture able to supply it.
///
/// Parsed by hand rather than with `serde_json`: the shape needed is three
/// string fields of one object per line, and `AGENTS.md` rule 3 asks before a
/// dependency is taken for something this small.
fn diagnostics(rendered: &str, fixture_code: &str) -> Vec<Said> {
    let mut found = Vec::new();
    for line in rendered.lines() {
        let Some(message) = field(line, "message") else {
            continue;
        };
        // Only diagnostics of level `error`: a warning is not why a fixture
        // failed to compile.
        if field(&message, "level").as_deref() != Some("error") {
            continue;
        }
        // **Everything rustc wrote, and nothing the fixture wrote.** The
        // headline alone is too narrow: `expected \`Literal\`, found \`&str\`` is a
        // span's label and `cannot be constructed` is a child note, and both are
        // the compiler's own words about the refusal. What has to go is the
        // *source*, which rustc echoes back in `rendered`, in each span's
        // `text` and in `file_name`.
        //
        // **And the sentence has to be rustc's.** `compile_error!` puts an
        // author-chosen string into this diagnostic's own `message` field, at
        // level `error`, so a fixture could declare its expectation and then
        // print it:
        //
        // ```text
        // //! EXPECT: the trait bound `String: LogValue` is not satisfied
        // fn main() {
        //     compile_error!("the trait bound `String: LogValue` is not satisfied");
        // }
        // ```
        //
        // Six lines, naming neither `nc_log` nor `LogValue` nor any log call,
        // counted as proof that a `String` cannot be a log value — measured,
        // with the count unchanged. A rule asking "does the output hold this
        // string" rather than "whose words are these" only moves the channel,
        // from stderr to `rendered` to here.
        //
        // Requiring an error code is too strong:
        // `literal!` refuses a non-literal with `no rules expected \`leaked\``,
        // a macro-rules match failure that carries no code, and that is the
        // wall doing its job. What separates the two is authorship, and a
        // fixture's own code is where a fixture's words are.
        let Some(said) = field(&message, "message") else {
            continue;
        };
        if !fixture_code.is_empty() && fixture_code.contains(said.trim()) {
            continue;
        }
        let code = field(&message, "code")
            .and_then(|code| field(&code, "code"))
            .unwrap_or_default();
        found.push(Said {
            sentence: format!("{code} {}", without_the_source(&message)),
            // **Where rustc pointed, which is the one thing a fixture cannot
            // write.** Every rule above is about the *words* of the diagnostic —
            // whose they are, whether the crate is behind them, whether the
            // fixture names the door somewhere in its code. None asks whether
            // the refusal happened *at* the door. So a fixture can name the
            // door in a line rustc never complained about and fail for something
            // else entirely: measured, `a_leaked_string_cannot_reach_a_literal`
            // rewritten as a valid `impl nc_log::Source` plus one unrelated
            // `const ELSEWHERE: &str = String::new().leak();` gave `ok 11
            // fixture(s) refuse to compile, for the stated reason` and
            // `check passed`, with the door contributing nothing — deleting that
            // one line made the file compile clean.
            //
            // Worse in the other direction: `an_origin_cannot_hold_a_runtime_
            // string` rewritten to fail on an unrelated `fn takes_one(_:
            // nc_log::Literal)` let `Origin`'s two fields be
            // `pub &'static str` — the exact state that fixture exists to
            // prevent — with `check passed` and a consumer crate outside the
            // workspace shipping an email address into a log line.
            at: pointed_at(&message),
        });
    }
    found
}

/// Whether the door on this line sits in a type annotation rather than in what
/// the line declares.
///
/// **Not a question about braces, whose every answer is a spelling.**
/// `let held: Option<nc_log::Variant> = { … }` is the measured attack — the
/// dead-annotation forgery: a dead annotation on an expression block, less
/// indented than the caret, paying for a door the fixture never touches.
/// Stepping over `= {`, and excusing `const`/`static` by an opener list behind
/// an `=` test, leave the same forgery written four more ways — `= vec![`,
/// `= [`, `= match text {`, `= if flag {`, `= Some(Wrapper {` — each measured
/// walking past both. `before.contains('=')` is the obvious widening and it is
/// **too strong**: `struct S<T = u8> {` is a genuine item header carrying an
/// `=`.
///
/// The brace is not the question. What makes those lines forgeries is where
/// the door sits: after a `:` and before the `=`, which is a *type annotation* —
/// a claim about a value's shape, not about what the line declares.
/// `impl nc_log::Source for X {` and `nc_log::Origin {` name the door as the
/// subject and have no such colon; a parameter list is the same annotation by
/// another name, and this repository's corpus has always read
/// `fn helper(_: nc_log::LogValue) {}` above a caret as carrying nothing.
///
/// `::` is not that colon, and neither is one inside `<…>` after the first — the
/// region ends at the first `=` outside the angle brackets, or at the end of the
/// line.
fn the_door_is_in_an_annotation(code: &str, door: &str) -> bool {
    // **The caller has already asked whether the line names the door.** Written
    // `any && !alive`, the `any` conjunct cannot change a verdict: its one
    // caller does the same thing with `true` as it does with a `false` that
    // names nothing, so a line with no occurrence at all reaches both arms
    // alike — and a clause nothing can decide is not one this file keeps.
    let spent = what_this_line_only_mentions(code);
    !code
        .match_indices(door)
        .any(|(found, _)| !spent.get(found).copied().unwrap_or(false))
}

/// The closing bracket that matches the one at `opens`, or the end.
///
/// `->` is not a closing angle. The same clause is written twice — here and in
/// [`the_type_ends`] — and the measurement
/// `let held: fn() -> fn() -> Box<dyn Iterator<Item = …>> = {` belongs to the
/// other copy: that line holds no macro group and no turbofish, so it never
/// reaches this function at all. The row that decides *this* copy is
/// `let held = foo::<fn() -> nc_log::LogValue>({`,
/// where the arrow sits inside the turbofish whose end this walks to.
fn closing(letters: &[(usize, char)], opens: usize) -> usize {
    let open = letters[opens].1;
    let close = match open {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        _ => return opens + 1,
    };
    let mut depth = 0_i32;
    for (index, (_, letter)) in letters.iter().enumerate().skip(opens) {
        if *letter == open {
            depth += 1;
        } else if *letter == close {
            if close == '>' && letters[index - 1].1 == '-' {
                continue;
            }
            depth -= 1;
            if depth == 0 {
                return index;
            }
        }
    }
    letters.len()
}

/// Whether a `:` here opens a type rather than naming what the line declares.
///
/// **Inside `<…>` or `(…)` it always does**: a bound and a parameter are both
/// types written for something else. At depth zero the question is the other
/// way round, and it is asked as an **exception list on purpose**: a colon
/// opens a type *unless* the line is an item header whose door is what it
/// declares. A spelling missing from an exception list refuses something
/// legitimate — which the wall says out loud, on a fixture, in a message naming
/// the line. A spelling missing from a list of *binders* would do the opposite
/// and pass a forgery in silence, and that is the trade this list is sized for.
fn opens_a_type(code: &str, at: usize, angle: i32, paren: i32) -> bool {
    // **And the parenthesis half too.** The argument that no line of Rust puts
    // a colon inside parentheses on a line the exception list excuses is false:
    // a function-pointer type may name its parameters and a `where` predicate
    // may take one as its subject, so
    // `impl Mine where fn(held: nc_log::LogValue): Copy {` is a line whose
    // colon sits at angle 0, paren 1, excused by its own `where` — and the door
    // in it is a parameter of an unrelated type, which this file calls carrying
    // nothing. Measured. An argument about what cannot be written is worth
    // what the person writing it knows about Rust.
    if angle > 0 || paren > 0 {
        return true;
    }
    // **And the word has to be the header's, not any word before the colon.**
    // Split on every non-word character, `r#trait` is the word `trait` — and
    // `impl Smuggled { const HELD: … = {` puts a real `impl` in front of an
    // ordinary binding's colon. Both were measured turning the rule
    // off for the line. So the word must open a token (start of line or after
    // whitespace) and must not be shut behind a block the line has already
    // opened: a header's own colon comes before the brace it opens, and
    // anything after that brace is a statement.
    let before = &code[..at];
    let opens_the_line = |found: usize| {
        before[..found]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    };
    let ends_the_word = |found: usize, word: &str| {
        before[found + word.len()..]
            .chars()
            .next()
            .is_none_or(|letter| !letter.is_alphanumeric() && letter != '_')
    };
    // **And the word has to be the line's own, not a macro's.** Matched against
    // the raw text, which still holds every macro's token tree — `blanked`
    // keeps those deliberately, because the walk one function away has an arm
    // for them — `m!( where ); const HELD: Option<…> = {` turns the whole rule
    // off for the line and the dead-annotation forgery walks through, measured.
    // An item header's `trait` or `where` is at bracket depth zero; a macro's
    // is not.
    let depth_at = |found: usize| {
        before[..found]
            .chars()
            .fold(0_i32, |depth, letter| match letter {
                '(' | '[' | '{' => depth + 1,
                ')' | ']' | '}' => depth - 1,
                _ => depth,
            })
    };
    // **And "shut behind a brace" is a question about a block, not about a
    // character.** A test of `!before[found..].contains('{')` argues that a
    // header's own colon comes before the brace it opens. A const generic
    // argument written as a block puts a brace *inside the type* and before the
    // colon: under that test `impl Mine where Wrapper<{ SIZE }>:
    // nc_log::LogValue {` disqualifies its own `where`, the colon reads as an
    // annotation, and a genuine predicate declaring the door is reported as
    // not proving it — the too-strong direction, one const argument away from a
    // row this corpus pins as true. Balancing the braces instead is the obvious
    // widening and it is too weak, measured: `trait A { } const HELD:
    // Option<nc_log::LogValue> = {` balances to zero and turns the rule off for
    // an ordinary binding. What shuts a header is a brace that opens a *block*,
    // and that is a brace at bracket depth zero — one inside `<…>` or `[…]` is
    // part of a type. Measured both ways.
    !["trait", "where"].iter().any(|word| {
        before.match_indices(word).any(|(found, _)| {
            opens_the_line(found)
                && ends_the_word(found, word)
                && depth_at(found) == 0
                && !a_block_opens_in(&before[found..])
        })
    })
}

/// Whether a block opens in this text — a `{` that is not part of a type.
///
/// A type may hold a brace: `Wrapper<{ SIZE }>` is a const generic argument and
/// `[(); { SIZE }]` is an array length, and both are written inside the type's
/// own brackets. A brace at bracket depth zero is not that; it is the body the
/// line is opening, and everything after it is a statement.
///
/// The angle bracket is the one that cannot be counted honestly here — a `<` in
/// this position is a generic argument list far more often than a comparison —
/// so a stray `>` is clamped rather than allowed to drive the count negative,
/// and `->` is not a closing angle. What a stray `<` can hide is one brace, on a
/// line that also holds a comparison before its own colon; that is the limit,
/// and it is narrower than a `contains`, which hides every brace on every
/// line.
///
/// **The parenthesis is decided by nothing, and here is what would decide it.**
/// A deletion sweep reaches the other two counters — `[(); { SIZE }]` for the
/// square bracket, `Wrapper<fn() -> [u8; 1], { SIZE }>` for the angle — and
/// finds no line that reaches this one. It would take a brace at round
/// depth one with **no** `<` or `[` open around it, between the excusing word
/// and the colon; and the text there is a type, where a brace is an array
/// length or a const argument and so arrives inside a bracket either way. That
/// is an argument, which is the weakest thing in this file: the parenthesis
/// half of [`opens_a_type`], and the comma arm and the `==` guard of the `=`
/// arm in [`the_type_ends`], are clauses an argument of exactly this shape
/// would delete, and **all three are reached**. It is kept rather than deleted
/// because deleting it makes the rule *stricter* — a brace in parentheses would
/// shut the header, the colon would open a type region, and a legitimate
/// predicate would be refused. Anyone who can write the line should add the row
/// and this paragraph goes.
fn a_block_opens_in(text: &str) -> bool {
    let letters: Vec<(usize, char)> = text.char_indices().collect();
    let mut angle = 0_i32;
    let mut square = 0_i32;
    let mut round = 0_i32;
    let mut index = 0_usize;
    while index < letters.len() {
        let letter = letters[index].1;
        // **A macro's token tree is stepped over, the way the walk one function
        // away steps over it.** Without that the unclosed-`<` weakness above is
        // reachable rather than theoretical: the outer walk jumps a macro
        // whole, so its `angle` never sees a `<` inside one, and `opens_a_type`
        // therefore stays at depth zero — while this function, reading raw
        // text, counts that `<` and loses the brace that should shut the
        // header. `impl Mine where m!(N < 1): Copy { const HELD:
        // Option<u8> = { None }; }` walks the dead-annotation forgery through.
        // The mirror holds too: a macro whose own delimiter is a brace,
        // `impl Mine where m!{ N }: nc_log::LogValue {`, has that brace read as
        // the line's block and a genuine predicate refused. Both directions
        // measured.
        if matches!(letter, '(' | '[' | '{')
            && letters[..index]
                .iter()
                .rev()
                .find(|(_, before)| !before.is_whitespace())
                .is_some_and(|(bang, before)| {
                    *before == '!'
                        && text[..*bang]
                            .chars()
                            .next_back()
                            .is_some_and(|name| name.is_alphanumeric() || name == '_')
                })
        {
            index = closing(&letters, index) + 1;
            continue;
        }
        match letter {
            // **The shift guard, which `the_type_ends`, `the_return_type_ends`
            // and `what_this_line_only_mentions` carry too.** Without it a
            // `1 << 3` raises `angle` by two with no `>` to lower it, the `{`
            // that opens the item body is never seen at bracket depth zero, the
            // `where` exception keeps excusing the line, and the annotation
            // after it is read as *declaring* the door: the dead-annotation
            // forgery, one shift from a row this corpus already pins.
            //
            // The guard is unreachable at depth zero, so it cannot mistake the
            // `<` of a generic argument list that opens at the top level.
            '<' if (angle > 0 || square > 0 || round > 0)
                && (letters.get(index + 1).map(|(_, next)| *next) == Some('<')
                    || (index > 0 && letters[index - 1].1 == '<')) => {}
            '<' => angle += 1,
            '>' if index == 0 || letters[index - 1].1 != '-' => angle = (angle - 1).max(0),
            '[' => square += 1,
            ']' => square = (square - 1).max(0),
            '(' => round += 1,
            ')' => round = (round - 1).max(0),
            '{' if angle == 0 && square == 0 && round == 0 => return true,
            _ => {}
        }
        index += 1;
    }
    false
}

/// Whether the `=` here is a token of its own rather than half of a longer one.
///
/// **An enumeration of four characters is not this rule.** A guard written
/// `!matches!(letters[index - 1].1, '=' | '!' | '<' | '>')` covers `==`, `!=`,
/// `<=` and `>=`, and leaves the `=` of a match arm's `=>` and of every other
/// compound assignment — `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=` —
/// ending the type region with the door written after it never spent.
/// Measured with `|=` and `=>` planted where `==` passes.
///
/// Rust's lexer is the rule. It joins `=` to the character **before** it in
/// `==`, `!=`, `<=`, `>=`, `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`,
/// `<<=`, `>>=` and `..=`, and to the character **after** it in `==` and `=>`.
/// Those two sets are written out here because they are the whole of what the
/// lexer does, not a sample of it.
///
/// **And in type position the lexer is not the whole rule.** rustc's parser
/// splits a composite token that begins with `>` when it closes a generic
/// argument list, so `let held: Option<u8>= x;` is valid Rust in which that `=`
/// is a token of its own — and with `>` in the *before* set that `=` does not
/// end the region. Taking `>` out of the set is the obvious repair and it is
/// too weak, measured: the `=` of a `>=` inside a struct literal's field value
/// then ends the region and the door after it is certified, which is the
/// dead-annotation forgery this guard exists to refuse. So `>` stays in the
/// set, and what ends that region is the `;` after it — see the statement
/// boundary in [`the_type_ends`]. Both directions measured.
///
/// **The `>` half of the after-set is decided by nothing, and it is the one to
/// keep anyway.**
/// A `=>` is a match arm and a match arm is inside the match's braces, so
/// [`the_type_ends`]'s block depth has already stepped over it by the time this
/// is asked — which is why the sweep finds no row that this half alone
/// decides. That makes it unreachable *because of another clause*, not on its
/// own account: weaken the block depth and this is what still refuses the arm.
/// A guard that is dead only while its neighbour holds is the cheapest kind
/// there is, and the corpus row that reaches it is the one nobody has written
/// yet — a match arm's `=>` at block depth zero.
fn an_equals_of_its_own(letters: &[(usize, char)], index: usize) -> bool {
    let joins_before = |letter: char| {
        matches!(
            letter,
            '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '.'
        )
    };
    index
        .checked_sub(1)
        .and_then(|at| letters.get(at))
        .is_none_or(|(_, before)| !joins_before(*before))
        && letters
            .get(index + 1)
            .is_none_or(|(_, after)| !matches!(*after, '=' | '>'))
}

/// Where the type a `:` opened stops: its own bracket, a comma beside it, the
/// `=` that begins the value, or the block it is written inside.
fn the_type_ends(letters: &[(usize, char)], from: usize, angle: i32, paren: i32) -> usize {
    let mut depth = angle;
    let mut nested = paren;
    // **The brace this walk counts.** A predicate on the characters beside the
    // `=` closes one spelling at a time. The root is here: `opens_a_type` calls a struct
    // literal's field colon a type opening, so the walk is often already inside
    // a block, and an ordinary `=` in a *statement* of that block — `{ flags =
    // nc_log::LogValue::MASK; 0 }` — ends the region with the door after it
    // alive. No rule about the `=`'s neighbours can see that, because there is
    // nothing wrong with the `=`; what is wrong is where it is. A type region
    // cannot contain a block, so a brace opens one and the region carries on
    // past everything in it to its own comma or end.
    let mut block = 0_i32;
    for (index, (_, letter)) in letters.iter().enumerate().skip(from + 1) {
        match letter {
            // **A shift is one token, not two opens.** Counted as two,
            // `let held: [u8; 1 << 3] = [0; 8]; nc_log::LogValue {` raises
            // `depth` by two with nothing on the line to lower it, and every
            // exit from this walk is then shut — the comma arm wants
            // `depth == angle`, the `=` arm `depth <= 0`, the bracket arm
            // `paren > 0` — so the region runs to the end of the line and
            // swallows the declaration after the `;`: the too-strong direction
            // the `'}' if block == 0` arm closes for braces. Ignoring any `<`
            // beside another `<` is the obvious repair and it is too weak,
            // measured: a qualified path's `<<` in
            // `impl<T: Into<<u8 as Trait>::Out> + nc_log::LogValue>` is two real
            // opens, and the bound's region would end four tokens early with
            // the door after it certified. A shift can only be inside a group
            // the type itself opened — an array length, a const argument — so
            // the guard asks for both: `nested` moves for `(` and `[`, and a
            // const argument is written in **braces**, which move `block`.
            // Asking `nested` alone, `let held: Wrapper<{ 1 << 3 }> = x;
            // nc_log::LogValue {` leaves `depth` at two, no exit holds, and the
            // declaration after the `;` is swallowed — measured.
            '<' if (nested > paren || block > 0)
                && (letters.get(index + 1).map(|(_, next)| *next) == Some('<')
                    || letters[index - 1].1 == '<') => {}
            '<' => depth += 1,
            '>' if letters[index - 1].1 != '-' => {
                depth -= 1;
                if angle > 0 && depth < angle {
                    return index;
                }
            }
            // **The comma arm, which a line reaches.** `struct S<A: Clone, B =
            // nc_log::LogValue> {` is this corpus's own "a default type
            // parameter in a genuine item header declares the door" row one
            // bounded parameter later, and without this arm the bound's
            // annotation swallows the default — one parameter apart, opposite
            // answers, which is the shape this file records for the
            // return-type arm. The doc above names it.
            ',' if depth == angle && nested == paren && block == 0 => return index,
            '(' | '[' => nested += 1,
            ')' | ']' => {
                nested -= 1;
                if paren > 0 && nested < paren {
                    return index;
                }
            }
            // **A statement boundary ends the region.**
            // `let held: Option<u8>= x;` is valid Rust whose `=` is a token of
            // its own — rustc's parser splits a composite token beginning with
            // `>` when it closes a generic argument list — and a flag telling
            // [`an_equals_of_its_own`] whether the `>` just passed closed a
            // group, read as `depth > 0`, does not hold: `depth` is raised by
            // any `<` the walk has counted, which on a struct literal's field
            // value is a **comparison**, so one `a < b` earlier in the same
            // region turns a later `>=` into a region-ender and the door after
            // it is certified — the forgery taking `>` out of the set is
            // refused for, one comparison away. Measured, and so are the
            // narrowings of that flag, each failing in one direction or the
            // other, because no rule on the characters can tell a generic `<`
            // from a less-than.
            //
            // The question does not have to be asked. A type annotation cannot
            // cross a statement boundary, so an unbracketed `;` ends the region
            // whatever the `=` before it was — and `Option<u8>= x; …` is
            // decided by the `;` rather than by the `=`. The bracket test is
            // what keeps an array length's own `;` — `[u8; 8]` — from ending a
            // type at its second character.
            ';' if nested == paren && block == 0 => return index,
            // **A brace opens the block, and the region walks through it.**
            // Without this the `=` arm below fires on any assignment or match
            // arm written inside a nested block, which is the dead-annotation
            // forgery one nesting level down.
            '{' => block += 1,
            // **And a brace with no opener of its own closes the region.** The
            // walk is inside the struct literal or the item body the colon was
            // written in, so the `}` that ends *that* is where the type stops;
            // without this arm a declaration written after it on the same line
            // is swallowed, which refuses something legitimate.
            '}' if block == 0 => return index,
            '}' => block -= 1,
            // **The `=` that begins the value, and not the one inside an
            // operator.** The argument that `==` cannot precede the first `=` of
            // an annotation is false: `opens_a_type` calls a struct-literal
            // field's colon a type opening, so in `Wrapper { seen: FLAG ==
            // nc_log::LogValue::Named, inner: {` the region, without this
            // guard, ends four characters in and the door after it is never
            // spent. A list of four characters is not the guard either — see
            // [`an_equals_of_its_own`], which asks the lexer's question
            // instead.
            '=' if depth <= 0
                && nested <= 0
                && block == 0
                && an_equals_of_its_own(letters, index) =>
            {
                return index;
            }
            _ => {}
        }
    }
    letters.len()
}

/// Every byte of this line that is a **mention** of a type rather than a
/// declaration of one.
///
/// Not a question about braces, or about one colon, whose every answer is one
/// spelling: a single walk, and it marks four things:
///
/// * **a type a `:` opened** — a binding's annotation, a bound inside `<…>`, a
///   parameter inside `(…)` — up to its own bracket, the comma beside it, the
///   `=` that begins the value, or the block it was written inside;
/// * **a turbofish**, `::` and then a `<` with any whitespace between them,
///   because Rust lexes those as separate tokens and one space would turn the
///   whole scan off — the sibling corpus one function away carries rows for
///   exactly that hazard;
/// * **a return type**, `->` to the `{` — measured as the shape that made
///   `fn make() -> nc_log::LogValue {` a declaration and
///   `fn make(seed: u8) -> nc_log::LogValue {` a mention, one parameter apart,
///   and at most one of those can be right;
/// * **a macro's arguments**, because what a macro is handed is not what the
///   line declares.
///
/// Every index here is a **byte** offset. Written half in character offsets
/// from `enumerate()` and half in byte offsets from `match_indices`, the
/// predicate compares two index spaces, and nineteen non-ASCII characters
/// before the door are enough to make a dead annotation read alive, measured.
fn what_this_line_only_mentions(code: &str) -> Vec<bool> {
    let letters: Vec<(usize, char)> = code.char_indices().collect();
    let ends = code.len();
    let mut spent = vec![false; ends];
    let byte = |index: usize| letters.get(index).map_or(ends, |(at, _)| *at);
    let mut angle = 0_i32;
    let mut paren = 0_i32;
    let mut index = 0_usize;
    while index < letters.len() {
        let (at, letter) = letters[index];
        match letter {
            // **And the same guard a third time.** Giving it to
            // [`the_return_type_ends`] alone does not decide the row, measured:
            // this walk counts `1 << 3` as two opens of its own, `angle` reaches
            // three, and `opens_a_type`'s first line reads a `where` predicate's
            // colon as a type opening whatever the arrow-fill did. Three copies
            // of one sentence is two too many and the third is written here
            // rather than shared, because each walk carries a different pair of
            // counters; what they must agree on is the rule, and the test below
            // is what holds them to it.
            '<' if letters
                .get(index + 1)
                .map(|(_, next)| *next)
                .is_some_and(|next| next == '<')
                || (index > 0 && letters[index - 1].1 == '<') =>
            {
                index += 1;
            }
            '<' => {
                angle += 1;
                index += 1;
            }
            '>' => {
                angle = (angle - 1).max(0);
                index += 1;
            }
            '(' | '[' | '{' => {
                // A macro is named by an identifier and then `!`; `if !(a && b)`
                // is not one, and its `!` follows a space.
                let named = letters[..index]
                    .iter()
                    .rev()
                    .find(|(_, before)| !before.is_whitespace())
                    .is_some_and(|(bang, before)| {
                        *before == '!'
                            && code[..*bang]
                                .chars()
                                .next_back()
                                .is_some_and(|name| name.is_alphanumeric() || name == '_')
                    });
                if named {
                    let closes = closing(&letters, index);
                    spent[at..byte(closes)].fill(true);
                    index = closes;
                } else {
                    if letter == '(' {
                        paren += 1;
                    }
                    index += 1;
                }
            }
            ')' => {
                paren = (paren - 1).max(0);
                index += 1;
            }
            ':' if letters.get(index + 1).map(|(_, next)| *next) == Some(':') => {
                let mut ahead = index + 2;
                while letters
                    .get(ahead)
                    .is_some_and(|(_, gap)| gap.is_whitespace())
                {
                    ahead += 1;
                }
                if letters.get(ahead).map(|(_, next)| *next) == Some('<') {
                    let closes = closing(&letters, ahead);
                    spent[byte(ahead)..byte(closes)].fill(true);
                    index = closes;
                } else {
                    index += 2;
                }
            }
            ':' => {
                if opens_a_type(code, at, angle, paren) {
                    let closes = the_type_ends(&letters, index, angle, paren);
                    spent[at..byte(closes)].fill(true);
                    index = closes;
                } else {
                    index += 1;
                }
            }
            // **A return type ends where its own group does.** A fill to the
            // next `{` on the line with no bracket depth at all lets an arrow
            // written *inside* a generic argument list swallow everything after
            // it — the door included. `impl Mine where
            // Wrapper<u8>: nc_log::LogValue {` is a declaration and
            // `impl Mine where Wrapper<fn() -> u8>: nc_log::LogValue {` would
            // not be: one arrow apart, opposite answers, which is the shape this
            // arm's own doc records for the parameter list. The angle counter's
            // row in `a_block_opens_in`'s doc is green under such a fill only
            // because its `{ SIZE }` happens to stop the fill early; take the
            // const argument away and the same line is refused.
            '-' if letters.get(index + 1).map(|(_, next)| *next) == Some('>') => {
                let ends = the_return_type_ends(&letters, index);
                spent[at..byte(ends)].fill(true);
                index += 2;
            }
            _ => index += 1,
        }
    }
    spent
}

/// Where the return type an `->` opened stops.
///
/// The brace it opens, a comma beside it, or the bracket that closes a group
/// opened **before** the arrow — because an arrow inside `<…>` or `(…)` belongs
/// to a type written for something else, and what follows the group is the line
/// again. `->`'s own `>` is not a closing angle here, the way it is not in
/// [`closing`] or [`the_type_ends`]; this is the third copy of that sentence,
/// and each walk that carries it needs it.
fn the_return_type_ends(letters: &[(usize, char)], arrow: usize) -> usize {
    let mut angle = 0_i32;
    let mut nested = 0_i32;
    for (index, (_, letter)) in letters.iter().enumerate().skip(arrow + 2) {
        match letter {
            // **The brace that ends a return type is not one written inside
            // it.** Returning at the *first* `{` with no depth test, a const
            // generic argument in the return type ends the fill at its own
            // opening brace and the door after it is left unspent:
            // `fn make() -> Result<u8, nc_log::LogValue> {` is a mention and
            // `fn make() -> Wrapper<{ SIZE }, nc_log::LogValue> {` would read as
            // a declaration — one const argument apart, opposite answers, which
            // is the shape this arm's own doc records twice. Both siblings say
            // so: `the_type_ends` counts braces because *a type region cannot
            // be ended by a brace it is written inside*, and `a_block_opens_in`
            // says a brace inside `<…>` or `[…]` is part of a type.
            // The brace that opens the body and the comma beside the type are
            // both the arrow's own depth, and neither is one written inside it.
            '{' | ',' if angle == 0 && nested == 0 => return index,
            // The shift guard — see [`the_type_ends`]. Without it this would be
            // one rule written twice with the second copy the narrower, which
            // is this file's own signature defect.
            '<' if nested > 0
                && (letters.get(index + 1).map(|(_, next)| *next) == Some('<')
                    || letters[index - 1].1 == '<') => {}
            '<' => angle += 1,
            '>' if letters[index - 1].1 != '-' => {
                if angle == 0 {
                    return index;
                }
                angle -= 1;
            }
            '(' | '[' => nested += 1,
            ')' | ']' => {
                if nested == 0 {
                    return index;
                }
                nested -= 1;
            }
            _ => {}
        }
    }
    letters.len()
}

/// Whether rustc's caret sits at a use of `door`.
///
/// Two ways, and no third — a wider window stops being decisive.
///
/// **The caret's own line names the door.** The ordinary case:
/// `fn accepts<T: nc_log::LogValue>(_: T) {}` is where the refusal happens.
///
/// **Or a block the caret is inside opens with the door.** Two real fixtures
/// need this and neither is an `impl`: `a_leaked_string_cannot_reach_a_literal`
/// is refused at its `const TEXT: … .leak()`, three lines below
/// `impl nc_log::Source for Smuggled`, and `an_origin_cannot_hold_a_runtime_
/// string` at a field three lines below `nc_log::Origin {`. In both the door is
/// what makes the code below it the door's.
///
/// The chain is built by **indentation**, and that is the whole of why it is
/// decisive: a block the caret is inside is strictly *less* indented than the
/// caret, so a sibling statement can never join it. Written the loose way —
/// "back to the last line at column zero" — it admits
///
/// ```rust,ignore
/// fn main() {
///     let _: Option<nc_log::Origin> = None;   // a dead mention…
///     takes_one("plain");                     // …and the real error, here
/// }
/// ```
///
/// which is the measured attack almost verbatim: a dead mention anywhere in the
/// same function pays for the caret, and `Origin`'s two fields can be
/// `pub &'static str` with `check passed`. At the same indent it is a sibling,
/// and a sibling proves nothing about where rustc pointed.
fn caret_uses(lines: &[&str], line: usize, door: &str) -> bool {
    // **Only the qualified path, never the bare spelling.**
    //
    // Accepting the bare `Source`/`Field`/`Shipper`, so that a caret landing
    // on `impl nc_log::Source for X` written in `use`-imported form still
    // counts, means deciding whether a bare name on a caret line is `nc-log`'s
    // or the fixture's own — and each of the ways of deciding it below fails, in a
    // different direction, measured:
    //
    // * A fixture declaring its own `trait Source` makes `impl Source for
    //   Smuggled {` satisfy the rule while naming nothing of `nc-log`'s: a
    //   fixture whose whole refusal was `cannot call non-const
    //   method `String::leak::<'_>` in constants` — a plain rustc error needing
    //   no `nc-log` at all — was counted as criterion 2's proof.
    // * A list of openers consulted to *excuse* a bare name certifies every
    //   spelling it misses as a forgery: `let Variant = …`, ten more like it, a
    //   generic parameter and three pattern binders — and it is a second, wider
    //   copy of a list this file keeps 700 lines away, which is the
    //   written-twice shape its own header warns about.
    // * Asking instead whether the file *imports* the door does not fail
    //   closed: `use nc_log::LogValue as Other;` binds another name, and a
    //   ` use ` inside a string literal sets it for a whole file.
    //
    // **No fixture in this repository imports anything from `nc-log`.** Every
    // fixture writes the qualified `nc_log::X`, so the bare spelling is
    // accepted nowhere and removing it changes no verdict — measured,
    // `12 fixture(s) refuse to compile, for the stated reason they declare`,
    // unmoved. What it removes is every one of the holes above at once, and the
    // two mirrors the import question has besides: a leading `::` and a `use`
    // wrapped across lines are real imports it calls unrelated.
    //
    // The cost is named rather than hidden: a fixture written with `use
    // nc_log::X;` and a bare `X` on its caret line will be reported as **not
    // proving its door**. That is loud, it is actionable, and the action is to
    // write the path. A rule whose misses are a fixture asked to be clearer is
    // not the same kind of rule as one whose misses certify a forgery.
    //
    // The hollow crate cannot catch any of this, because it removes every
    // guarantee at once: a fixture attacking any door compiles against it.
    let names = |text: &str| {
        // **In code, which a comment is not** — the sentence its sibling
        // `not_about_its_own_shape` already carries. Stripping `//!` and
        // nothing else lets an ordinary `//` line pay for the door: measured,
        // a forgery about nothing `Origin` guards passed the whole gate, and
        // deleting the single comment line `// nc_log::Origin` and nothing else
        // turned it red.
        //
        // `crate::gates::blanked` honours string literals and turns every
        // comment into spaces, so no special case for `//!` is needed.
        crate::gates::blanked(text).contains(door)
    };
    let indent = |text: &str| text.len() - text.trim_start().len();
    let at = line.saturating_sub(1);
    let Some(caret) = lines.get(at) else {
        return false;
    };
    if names(caret) {
        return true;
    }
    let mut deepest = indent(caret);
    for above in lines[..at].iter().rev() {
        if above.trim().is_empty() {
            continue;
        }
        if indent(above) >= deepest {
            continue;
        }
        deepest = indent(above);
        // **A block a statement opens is not the item the caret is in.** The
        // walk accepts a mention on any line less indented than the caret,
        // which is what lets `impl nc_log::Source for X {` — the item the
        // failing method is inside — carry the door. A `let … = { … };` is less
        // indented too, and it carries a *type annotation*: measured,
        // `let held: Option<nc_log::Variant> = { … nc_log::Literal(email.leak())
        // … };` made a dead annotation the "proof" that a refusal about
        // `Literal` was `Variant`'s door, and the fixture for
        // `a_variant_cannot_be_built_from_outside` was rewritten to fail
        // somewhere else entirely with every fixture still reported `ok`.
        //
        // So an expression block is stepped over and the walk goes on to the
        // item header above it. The door has to be named where the caret is, on
        // a line the caret's own expression continues onto, or on the item the
        // whole thing sits in.
        // **A door in a type annotation is not the door this line declares.**
        // See `the_door_is_in_an_annotation`: asked as a question about
        // braces, each answer is one spelling of a forgery that has five more.
        // **One blanked copy, and both questions read it.** Spending comments
        // and nothing else leaves a door inside a *string literal* on a
        // block-opening line as code: `let held = vec!["nc_log::LogValue", {`
        // names the door in a value the fixture chose and pays for the refusal
        // inside it. This table carries a row saying a ` use ` inside a literal
        // is not an import, and the door rule asks the same question.
        let code = crate::gates::blanked(above);
        if !code.contains(door) {
            continue;
        }
        if the_door_is_in_an_annotation(&code, door) {
            continue;
        }
        return true;
    }
    false
}

/// One thing rustc said, and the fixture lines it said it about.
#[derive(Debug)]
struct Said {
    sentence: String,
    at: Vec<usize>,
}

/// Every source line a diagnostic points at, its children and labels included.
///
/// Taken from the raw JSON rather than from `without_the_source`, which strips
/// the spans' `text` — the line *numbers* survive that stripping and are what
/// this reads. A fixture chooses every string in itself; it does not choose
/// where the compiler puts the caret.
fn pointed_at(diagnostic: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let needle = "\"line_start\":";
    let mut rest = diagnostic;
    while let Some(at) = rest.find(needle) {
        // **Only spans in the fixture itself.** Most of `nc-log`'s refusals
        // carry secondary spans in `value.rs` and `ship.rs` — measured at
        // lines 147, 190, 191, 304, 315, 424 and 612 — and with no file name
        // consulted those numbers are read as *the fixture's* lines. A ten-line
        // fixture reaching a door through an alias was reported as
        // *"rustc refused it at line(s) [9, 190, 191]"*, and padding its header
        // until the door landed on line 190 made the same code pass with not
        // one character of it changed.
        //
        // The span's own object carries the file; every shipped fixture's
        // primary span is in the crate's own `main.rs`, which is the one file a
        // fixture is.
        let span = &rest[..at];
        let names_the_fixture = span
            .rsplit_once("\"file_name\":\"")
            .is_none_or(|(_, after)| after.starts_with("src/main.rs"));
        rest = &rest[at + needle.len()..];
        if !names_the_fixture {
            continue;
        }
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(line) = digits.parse::<usize>() {
            found.push(line);
        }
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// A diagnostic with every echo of the fixture's own source taken out.
///
/// rustc writes the offending lines back in two places — the `rendered` block,
/// and the `text` of each span — and both are how a fixture can hand a gate the
/// reason it is supposed to be judged against. What is left is the
/// headline, the children's notes and the spans' labels: rustc's own words
/// about why it refused, which a fixture cannot write for it.
fn without_the_source(diagnostic: &str) -> String {
    let characters: Vec<char> = diagnostic.chars().collect();
    let mut kept = String::new();
    let mut at = 0;
    while at < characters.len() {
        let mut skipped = false;
        // `file_name` is the fixture's own path, which an author chooses as
        // freely as any string in it.
        for echo in ["\"rendered\":", "\"text\":", "\"file_name\":"] {
            let opens: Vec<char> = echo.chars().collect();
            if characters[at..].starts_with(opens.as_slice()) {
                at += opens.len();
                // Step over the string that follows, whatever is in it.
                if characters.get(at) == Some(&'"') {
                    at += 1;
                    let mut escaped = false;
                    while at < characters.len() {
                        match characters[at] {
                            '\\' if !escaped => escaped = true,
                            '"' if !escaped => {
                                at += 1;
                                break;
                            }
                            _ => escaped = false,
                        }
                        at += 1;
                    }
                }
                skipped = true;
                break;
            }
        }
        if skipped {
            continue;
        }
        kept.push(characters[at]);
        at += 1;
    }
    kept
}
/// The sentence rustc wrote, bounded to what fits on a line.
///
/// The composed message holds the fixture's declared reason and then rustc's
/// whole JSON diagnostic, so the bound is on characters: the object is one
/// line, and no count of lines can shorten it. Falls back to the text itself
/// when there is no `message` to find, so a change in what cargo emits degrades
/// to something a person can still read rather than to nothing.
fn what_rustc_said(message: &str) -> String {
    let said = field(message, "message").unwrap_or_else(|| message.to_owned());
    let said = said.replace('\n', " ");
    match said.char_indices().nth(160) {
        Some((at, _)) => format!("{}…", &said[..at]),
        None => said,
    }
}

/// The value of one key at the **top level** of a JSON object, as written.
///
/// **Depth-aware, because the first match is the wrong one.** A diagnostic's
/// `children` array holds sub-diagnostics with their own `"level"` and
/// `"message"`, written *before* the outer object's — so a `find` for
/// `"level":"` answers `note`, and every real error is skipped. Measured: the
/// whole compile-fail gate reporting every fixture as failing for a reason it
/// does not declare, with the diagnostics sitting in the output it had just
/// read.
///
/// Returns a string's contents, or an object with its own braces so the caller
/// can descend one level.
fn field(object: &str, key: &str) -> Option<String> {
    let opens: Vec<char> = format!("\"{key}\":").chars().collect();
    let characters: Vec<char> = object.chars().collect();
    let mut depth = 0_usize;
    let mut in_a_string = false;
    let mut escaped = false;
    let mut at = 0;
    while at < characters.len() {
        let character = characters[at];
        if in_a_string {
            match character {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => in_a_string = false,
                _ => escaped = false,
            }
            at += 1;
            continue;
        }
        match character {
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            '"' => {
                // A key of the object this call is about sits at depth 1.
                if depth == 1 && characters[at..].starts_with(opens.as_slice()) {
                    return value_at(&characters[at + opens.len()..]);
                }
                in_a_string = true;
                escaped = false;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// The value that starts here: a string's contents, or an object whole.
fn value_at(from: &[char]) -> Option<String> {
    match from.first()? {
        '"' => {
            let mut value = String::new();
            let mut escaped = false;
            for character in &from[1..] {
                match character {
                    '\\' if !escaped => escaped = true,
                    '"' if !escaped => return Some(value),
                    _ => {
                        if escaped {
                            value.push('\\');
                            escaped = false;
                        }
                        value.push(*character);
                    }
                }
            }
            None
        }
        '{' => {
            let mut depth = 0_usize;
            let mut in_a_string = false;
            let mut escaped = false;
            let mut value = String::new();
            for character in from {
                value.push(*character);
                if in_a_string {
                    match character {
                        '\\' if !escaped => escaped = true,
                        '"' if !escaped => in_a_string = false,
                        _ => escaped = false,
                    }
                    continue;
                }
                match character {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(value);
                        }
                    }
                    '"' => {
                        in_a_string = true;
                        escaped = false;
                    }
                    _ => {}
                }
            }
            None
        }
        // `null`, a number, an array — none of which this gate reads.
        _ => None,
    }
}

/// The diagnostic a fixture declares it must produce.
///
/// Written in the fixture itself, as `//! EXPECT: <substring>`, because the
/// fixture is where somebody reads what it is for. A fixture with no expectation
/// is an error rather than a pass: that is exactly the state a fixture drifts
/// into when the thing it tested was renamed.
fn expectation(fixture: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(fixture)
        .map_err(|error| format!("could not read {}: {error}", fixture.display()))?;
    let declared = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("//! EXPECT: "))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            format!(
                "{} declares no `//! EXPECT: <substring>`. Without one, \"it failed\" is \
             all this gate can check — and a fixture that stopped testing what it is \
             named for still fails, for a typo",
                fixture.display()
            )
        })?;
    if let Some(forged) = conjures_its_own_reason(&declared, &text) {
        return Err(format!(
            "{} declares `{declared}` and then {forged} — so the sentence rustc prints \
             is one this file arranged, and the fixture proves nothing about `nc-log`. \
             A fixture names what the wall refuses; it does not define it",
            fixture.display()
        ));
    }
    Ok(declared)
}

/// How long an expectation has to be before it is about one diagnostic.
///
/// The shortest rustc sentence this repository expects is
/// `` no rules expected `leaked` ``, **26** characters; the forgery this
/// refuses is `` `String ``, 7. Twenty-five sits between them, and it is
/// a floor on the *sentence* rather than on what it names — `Id` is two
/// characters and is a perfectly specific thing for a diagnostic to be about.
///
/// **The headroom is one character, not four.** The next-shortest sentences
/// are `` struct `Shipper` is private `` at 27 and
/// `` tuple variant `Id` is private `` at 29, so a floor raised by four on the
/// belief that there is room refuses two shipped fixtures — measured.
///
/// **At module scope, where a test can name it**, the way `LONGEST_PATH`,
/// `FLUSH` and `GRACE` each are — a module-level constant its test reads by
/// name, `assert_eq!(super::LONGEST_PATH, 120, …)`. As a `const` in a function
/// body, what stands in for that is a property of the fixture table: the
/// shortest shipped expectation, 26, compared against the literal 25 written a
/// second time — and measured, **every value from 5 to 26 then passes the
/// whole suite**, and at 8 the forgery `` `String` `` — the same one, with its
/// backtick closed — is accepted as an expectation. Held from above by the
/// shipped list, from below by nothing, and twenty-one values of slack between.
///
/// It is at module scope so that
/// `the_specificity_floor_is_the_number_its_paragraph_argues_for` can read it,
/// and `an_expectation_too_weak_to_be_about_one_diagnostic_is_refused` drives
/// that eight-character forgery so the number is held by what it buys and not
/// only by a pin.
const SPECIFIC_ENOUGH: usize = 25;

/// How a fixture makes rustc say the fixture's own words.
///
/// **Every fixture is forgeable past the authorship filter alone.** That
/// filter asks whether the fixture's *source* contains the diagnostic's
/// sentence — so a fixture that never writes the sentence, and instead makes
/// rustc produce it, walks through. Measured: every fixture replaced with a
/// file failing for an unrelated reason, each `EXPECT` kept unchanged, gave
/// `ok 11 fixture(s) refuse to compile, for the stated reason`.
///
/// Nine of those forgeries declared, locally, the very thing the `EXPECT` names —
/// `trait LogValue`, `struct Literal`, `struct Shipper`, `mod nc_log`, a
/// `macro_rules!` with a `$text:literal` rule. Two used
/// `#[diagnostic::on_unimplemented]`, which writes any sentence an author likes
/// into a span label.
///
/// So the rule is about the fixture, where a fixture's words are: **it may not
/// define what it claims the wall refuses.** Every backticked identifier in the
/// declaration is looked for as a local declaration, and the two attributes that
/// put an author's sentence into a diagnostic are refused outright.
///
/// `compile_error!` is here too. Reading the diagnostic's `message` rather than
/// the rendered text closes it once, and it belongs on this list as well
/// because this is the place that rule lives.
fn conjures_its_own_reason(declared: &str, code: &str) -> Option<String> {
    // **Code, not prose.** `code.contains(spelling)` reads the whole file, so a
    // fixture's own doc line — "the refusal below is rustc's own, not a
    // `compile_error!` we wrote" — refuses a fixture that is byte-identical in
    // behaviour, with a message that is actively false. This repository writes
    // exactly that kind of header.
    // **Flattened, because Rust does not require a keyword and its name on one
    // line.** A scan for `trait LogValue` as a string misses
    // `trait\nLogValue {}`, the same declaration to rustc — so a fixture can
    // declare the very trait it claims the wall refuses, keep its expected
    // sentence byte for byte, and pass. Measured with four of them passing,
    // and the wall itself deleted under them: `impl LogValue for String` in
    // `value.rs`, `ok 11 fixture(s) refuse to compile, for the stated reason`,
    // and a caller's text logged from outside the crate.
    //
    // Fixtures are not reachable from a crate root, so `cargo fmt` never joins
    // those lines for us.
    let source: String = readable(code)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    // Three spellings that put an author's sentence into a diagnostic, and one
    // that manufactures a refusal out of nothing. None has any business in a
    // file whose job is to be refused by another crate.
    for spelling in ["#[diagnostic::", "compile_error!", "macro_rules!"] {
        if source.contains(spelling) {
            return Some(format!("uses `{spelling}`"));
        }
    }
    // **And it has to touch the crate it is about — in code.** `nc_log` in a
    // comment would satisfy this, and every measured forgery put it there: a
    // `//!` header saying "this fixture does not depend on nc_log at all" was
    // enough.
    if !source.contains("nc_log") {
        return Some("never names `nc_log` outside its comments".to_owned());
    }

    // **An expectation with no backticked identifier is not one.** The rule
    // below has nothing to look for without one, so an expectation like
    // `cannot be constructed`, `attempt to use a
    // non-constant value in a constant` or `cannot initialize a tuple struct which
    // contains private fields` switches it off — sentences any file can provoke.
    // A fixture that cannot name what refuses it is a fixture whose subject is
    // a mood.
    // **Closed backticks, and a sentence long enough to be one.** A rule of
    // "at least one backticked token" is satisfied by `` EXPECT: `String `` —
    // seven characters, an unterminated backtick, and a substring of a large
    // fraction of every diagnostic rustc emits. Measured: the shipped fixture
    // kept failing, the gate kept saying `for the stated reason`, and the
    // stated reason had become "something about a String".
    if !declared.matches('`').count().is_multiple_of(2) {
        return Some("leaves a backtick unclosed, so what it names has no end".to_owned());
    }
    if declared.chars().count() < SPECIFIC_ENOUGH {
        return Some(format!(
            "is {} characters long, and a substring that short is one a diagnostic \
             about something else can hold",
            declared.chars().count()
        ));
    }
    let quoted: Vec<&str> = declared.split('`').skip(1).step_by(2).collect();
    if quoted.is_empty() {
        return Some(
            "names nothing in backticks, so there is no identifier to hold it to".to_owned(),
        );
    }
    quoted.into_iter().find_map(|quoted| {
        // **The bound, not the subject.** `Mine: nc_log::sealed::Sealed` is a
        // fixture declaring `struct Mine` — the *thing being refused*, which is
        // exactly what a fixture is for — and being refused *by*
        // `nc_log::sealed::Sealed`, which it must not declare. The left of a
        // `: ` is the fixture's own and the right is the wall's. A path is every
        // one of its segments: `mod nc_log` is the forgery that made a local
        // `sealed::Sealed` print as a fully qualified one.
        let (subject, bound) = quoted
            .rsplit_once(": ")
            .map_or(("", quoted), |(left, right)| (left, right));
        if let Some((needle, part)) = declared_in(&source, bound, &[]) {
            return Some(format!(
                "declares `{needle}` itself, so `{part}` is its own"
            ));
        }
        // **And the subject, when the subject is not the fixture's to own.**
        // Throwing away everything left of the last `": "` leaves the four
        // `nc_log::LogValue` rows' subject unread — `String`, `&str` or
        // `&String`, names any file may shadow in one line. Measured, the
        // four string fixtures rewritten to declare `struct String;` and
        // `struct str;` with every `//! EXPECT:` line byte-identical:
        //
        // ```text
        // ok    11 fixture(s) refuse to compile, for the stated reason they declare, …
        // check passed, with 9 warn line(s) above
        // ```
        //
        // and then `impl LogValue for String`, `&str` and `&String` added to
        // `crates/nc-log/src/value.rs` — the wall itself — with the same
        // `check passed` and `the_wall_holds_and_the_proof_actually_ran` at
        // `1 passed`. Acceptance criterion 2 forged away, and the guarantee
        // deleted under it, in four files that still cite `§15.4`.
        //
        // The empty-crate control cannot see it either: the stub
        // blanket-implements `LogValue`, so the forgery *compiles* there and the
        // control answers "the refusal really is the wall's".
        //
        // So the subject side is refused too — but only for the names the
        // language and its prelude already own. `Mine: …` stays legal, which is
        // what the paragraph above is about.
        declared_in(&source, subject, &NOT_A_FIXTURE_OF_ITS_OWN).map(|(needle, part)| {
            format!(
                "declares `{needle}` itself, so the `{part}` its expectation names is not \
                 the one this crate refuses"
            )
        })
    })
}

/// The names a fixture's *subject* may not be, because they are not its to own.
///
/// A fixture is a file that declares the thing being refused, so the subject of
/// its expectation is ordinarily its own — `Mine: nc_log::sealed::Sealed`. What
/// is never its own is a name the language or the prelude already has: a file
/// that writes `struct String;` has not built a subject, it has shadowed one, so
/// the sentence rustc prints is about a different type from the one the
/// fixture's own header says it is about.
const NOT_A_FIXTURE_OF_ITS_OWN: [&str; 25] = [
    "Box", "Cow", "Option", "Path", "PathBuf", "Result", "String", "Vec", "bool", "char", "f32",
    "f64", "i128", "i16", "i32", "i64", "i8", "isize", "str", "u128", "u16", "u32", "u64", "u8",
    "usize",
];

/// The item `source` declares out of the names `span` holds, if it declares one.
///
/// Written once because the bound rule and the subject rule are one question
/// asked of two halves of a sentence, and two copies of one rule is how the
/// narrower copy survives. `only`, when it is not empty, keeps the question to
/// the names in it.
///
/// **Anywhere on the line, not at its start.** A scan asking whether a line
/// *begins* with `trait LogValue` after stripping `pub ` lets
/// `pub(crate) trait LogValue {}` walk through — the string `pub ` is not a
/// prefix of `pub(crate)` — and so do `mod inner { pub trait LogValue {} }`,
/// `unsafe trait`, and an attribute written before the keyword. Five lines and
/// no dependency on the crate under test, with the shipped expectation
/// unchanged, measured.
fn declared_in(source: &str, span: &str, only: &[&str]) -> Option<(String, String)> {
    span.split(['<', '>', ',', '&', ' ', ':'])
        .filter(|part| !part.is_empty())
        .filter(|part| only.is_empty() || only.contains(part))
        .find_map(|part| {
            let written = [
                "trait ", "struct ", "enum ", "mod ", "type ", "union ", "fn ",
            ]
            .into_iter()
            .map(|keyword| format!("{keyword}{part}"))
            .find(|needle| source.contains(needle.as_str()));
            if let Some(needle) = written {
                return Some((needle, part.to_owned()));
            }
            // **And the binder with no keyword in front of it.** Rust has an
            // eighth way to bind a type name and it is the one this list cannot
            // hold: a generic parameter. `fn takes<String>(name: String)`
            // shadows the prelude `String` for that function's whole body, and
            // the source then holds no `struct String`, no `type String` and no
            // `fn String` — nothing the seven keywords look for.
            //
            // Measured: all four `LogValue` fixtures rewritten
            // this way, each keeping its `EXPECT` line byte for byte, and then
            // `impl LogValue for String`, `for &str` and `for &String` added to
            // the crate the wall is made of — the three the fixtures exist to
            // forbid. The fixtures went on refusing to compile, because the
            // `String` each refuses is the one it binds itself, and the whole
            // step printed `12 fixture(s) refuse to compile, for the stated
            // reason they declare` with `check passed`.
            //
            // This is the same shadowing the shipped corpus rows `struct String;`
            // and `struct str;` are written for, with the keyword taken away.
            bound_as_a_parameter(source, part).then(|| (format!("<{part}>"), part.to_owned()))
        })
}

/// Whether `source` binds `name` as a generic parameter of its own.
///
/// Only where an angle group *opens a declaration* — immediately after a
/// keyword and the name it declares, or after `impl`, which declares none. That
/// is what keeps `Vec<String>` an ordinary use of the prelude's type, and keeps
/// `fn accepts<T: nc_log::LogValue>(_: T)` binding `T` and nothing else.
///
/// A parameter's head is what stands before its bound or its default, so
/// `<T: LogValue>` binds `T` and `<const N: usize>` binds neither `N` nor
/// `usize` — the head there is `const N`, which is no name a fixture's
/// expectation can be about.
fn bound_as_a_parameter(source: &str, name: &str) -> bool {
    // **A comment writing *about* a binder is not one.** Read raw, a
    // fixture's text includes the `//!` prose header every shipped fixture
    // opens with, explaining what it refuses — so a sentence like
    // *"`fn takes<String>` would shadow it"* is read as the fixture binding
    // `String` itself, and the fixture is then rejected with *"declares
    // `<String>` itself, so the `String` its expectation names is not the one
    // this crate refuses"*. That is the too-strong half of the parameter rule:
    // green only while nobody has written the sentence, and nothing formats
    // `crates/nc-log/fixtures/`, so nothing would rewrite it either.
    let source: String = source
        .lines()
        .map(crate::gates::line_without_a_comment)
        .collect::<Vec<_>>()
        .join("\n");
    let source = source.as_str();
    for keyword in [
        "trait ", "struct ", "enum ", "union ", "type ", "fn ", "impl",
    ] {
        for (at, _) in source.match_indices(keyword) {
            let rest = &source[at + keyword.len()..];
            let head: String = rest
                .chars()
                .take_while(|letter| letter.is_alphanumeric() || *letter == '_')
                .collect();
            // **And the space rustc allows between them.** `fn takes <String>`
            // is the same item as `fn takes<String>`, and nothing in this tree
            // formats `crates/nc-log/fixtures/` — they are not a cargo target,
            // which is the one directory where an adversarial author writes.
            // Measured: one space added to the fixture that
            // shadows `String` with a parameter of its own, its `//! EXPECT:`
            // line byte for byte unchanged, and the wall's whole step printed
            // `12 fixture(s) refuse to compile, for the stated reason they
            // declare` with `check passed`. A newline does it too.
            let Some(angles) = rest[head.len()..].trim_start().strip_prefix('<') else {
                continue;
            };
            // To the matching `>`, because a bound carries its own: the group
            // in `<T: Into<String>>` ends at the second one.
            let mut depth = 1_usize;
            let mut end = None;
            for (index, letter) in angles.char_indices() {
                match letter {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(index);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else {
                continue;
            };
            if angles[..end]
                .split(',')
                .any(|one| one.split([':', '=']).next().unwrap_or_default().trim() == name)
            {
                return true;
            }
        }
    }
    false
}

/// [`conjures_its_own_reason`]'s two declaration scans, for the corpus.
///
/// `conjures_its_own_reason` needs a whole fixture directory and a compiler;
/// this is the clause inside it that decides whether a name in an expectation is
/// one the fixture declared, which is what a weakening would go for. The door
/// exists for the reason `caret_uses_for_tests` beside it does: `mutants.py`
/// runs `corpus::` and `limits::` for this file, and a clause reachable only
/// from `wall::tests` is a clause the meta-gate cannot weaken.
#[cfg(test)]
pub(crate) fn declared_in_for_tests(source: &str, span: &str, subject: bool) -> Option<String> {
    declared_in(
        source,
        span,
        if subject {
            &NOT_A_FIXTURE_OF_ITS_OWN
        } else {
            &[]
        },
    )
    .map(|(needle, _)| needle)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Compiled,
    FailedCorrectly,
    FailedWithout(String),
    /// It failed for the stated reason, and would have failed the same way with
    /// no `nc-log` behind it at all.
    WouldFailWithoutTheCrate,
    /// The empty-crate control could not be asked, and said so.
    ///
    /// Counted rather than absorbed: the printed sentence claims a rebuild for
    /// every fixture, and it is true only of the ones the control decided.
    ControlDidNotRun,
}

/// Builds one fixture in a crate of its own, and returns what cargo said.
///
/// Apart from `build_fixture`, which with the caret rule in it would cross
/// clippy's line limit. This half is the scaffolding — a manifest, a `main.rs`,
/// a `cargo build` — and the half that stays is the judgement.
fn compile_it_alone(
    root: &Path,
    fixture: &Path,
    scratch: &Path,
) -> Result<std::process::Output, String> {
    let quoted = |relative: &str| {
        root.join(relative)
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    };
    let path = quoted("crates/nc-log");
    // The seal fixture names `nc_catalog::Level`, because `Event` does.
    let catalog = quoted("crates/nc-catalog");
    std::fs::write(
        scratch.join("Cargo.toml"),
        format!(
            "# Written by `cargo xtask check`. Outside the workspace on purpose: a\n\
         # fixture that must not compile cannot be a member of a workspace whose\n\
         # gate is that everything compiles.\n\
         [workspace]\n\n\
         [package]\n\
         name = \"nc-wall-fixture\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\n\
         [dependencies]\n\
         nc-log = {{ path = \"{path}\" }}\n\
         nc-catalog = {{ path = \"{catalog}\" }}\n"
        ),
    )
    .map_err(|error| format!("could not write the fixture's manifest: {error}"))?;
    std::fs::copy(fixture, scratch.join("src/main.rs"))
        .map_err(|error| format!("could not copy the fixture: {error}"))?;

    let program = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    Command::new(program)
        // **JSON, so the reason is read from the diagnostic and not from the
        // build's text.** rustc echoes the offending source line into stderr,
        // so a fixture can supply its own expected reason by writing it into
        // the code that fails — with the failure being about something else
        // entirely. Measured, and this is the whole fixture:
        //
        // ```rust
        // //! EXPECT: the trait bound `String: LogValue` is not satisfied
        // fn main() {
        //     totally_undefined_helper("the trait bound `String: LogValue` is not satisfied");
        // }
        // ```
        //
        // `error[E0425]: cannot find function` — and a gate reading the text
        // counted it as proof that a `String` cannot be a log value. Changing
        // *which* string is looked for leaves the fixture able to supply it.
        // **`--offline`, because this gate must not fetch.** The scratch manifest
        // carries no lockfile, so without the flag cargo resolves it against
        // crates.io on every run — a fresh resolution per fixture per
        // `cargo xtask check`, `--offline` or not. With no network the
        // resolution fails, its error goes to *stderr* while
        // `--message-format=json` leaves stdout empty, and the gate then
        // reports every fixture as failing for the wrong reason:
        //
        // ```text
        // error crates/nc-log/fixtures/a_string_cannot_be_a_log_value.rs:1 —
        //       … did not fail for the reason it declares
        //       rustc said: nothing that looks like an error
        // error check failed: 11 finding(s) about the compile-fail fixtures
        // ```
        //
        // The word *network* appeared nowhere in the run. Every package this
        // build needs is on the machine by the time it runs: `cargo xtask check`
        // runs `cargo fetch --locked` before any step, and under `--offline`,
        // where nothing is fetched, a package missing from the machine has
        // already failed the `wiring` step, which names that fetch as the fix.
        .args(["build", "--quiet", "--offline", "--message-format=json"])
        // Its own target directory, so a fixture that must not compile never
        // leaves a half-built artifact in the workspace's.
        .env("CARGO_TARGET_DIR", scratch.join("target"))
        .current_dir(scratch)
        .output()
        .map_err(|error| format!("could not compile the fixture: {error}"))
}

/// Compiles one fixture against the real `nc-log`, outside the workspace.
fn build_fixture(root: &Path, fixture: &Path) -> Result<Outcome, String> {
    // **Removed on every path out, including the `?` ones.** A removal at the
    // end and before the early `return`s misses the `?` paths, and
    // `compile_it_alone` runs *before* `expectation(fixture)?`, so a 21 MB
    // `target/` is built and then abandoned: measured, one run of
    // `a_fixture_that_compiles_is_a_finding` left two directories, 38 MB.
    //
    // The work is a function of its own, so there is one way out and one
    // removal.
    let scratch = std::env::temp_dir().join(format!(
        "nc-wall-fixture-{}-{}",
        std::process::id(),
        fixture.file_stem().unwrap_or_default().to_string_lossy()
    ));
    let verdict = build_fixture_in(&scratch, root, fixture);
    let _ = std::fs::remove_dir_all(&scratch);
    verdict
}

fn build_fixture_in(scratch: &Path, root: &Path, fixture: &Path) -> Result<Outcome, String> {
    let _ = std::fs::remove_dir_all(scratch);
    std::fs::create_dir_all(scratch.join("src"))
        .map_err(|error| format!("could not prepare the fixture's directory: {error}"))?;

    // The path as text, escaped for TOML. `{:?}` on a `PathBuf` happens to
    // produce a quoted string and is not a serialiser — a path holding a
    // backslash or a quote would leave the manifest unparsable, and the failure
    // would read as the fixture compiling when it had never been built.
    let output = compile_it_alone(root, fixture, scratch)?;
    let rendered = String::from_utf8_lossy(&output.stdout).into_owned();
    // The fixture's own code, with the `//!` header stripped: that header is
    // where a fixture *declares* what it expects, and the rest is what it
    // compiles. A sentence found in there is a sentence the fixture wrote.
    // **Read once**, and both strings below are built from that one read.
    let whole = std::fs::read_to_string(fixture)
        .map_err(|error| format!("could not read the fixture: {error}"))?;
    let fixture_code: String = whole
        .lines()
        .filter(|line| !line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    let message = diagnostics(&rendered, &fixture_code);

    // **What the fixture says it expects, not a word that appears in its own
    // source.** A check of `message.contains("LogValue")` against stderr,
    // where rustc echoes the offending source line, is satisfied by every
    // fixture from its own `fn accepts<T: nc_log::LogValue>` line: a fixture
    // testing nothing about strings, failing because a trait it names does not
    // exist, passes — measured, with the borrowed-`&str` fixture in that
    // position.
    let expected = expectation(fixture)?;
    // **And it has to exercise the door it is named for.** Every rule above is
    // about whose words the diagnostic carries and whether the crate is behind
    // it; none asks whether the fixture is about the *thing its name claims*.
    // A four-line file proving that `Literal` has no `Default` clears all of
    // them, measured.
    let name = fixture
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if let Some(wrong) = not_about_its_own_shape(&name, &fixture_code, &expected) {
        let _ = std::fs::remove_dir_all(scratch);
        return Ok(Outcome::FailedWithout(wrong));
    }
    let whole_fixture = whole;
    // **And the reason is rustc's, not the fixture's.** `compile_error!` puts
    // an author-chosen string into the diagnostic's own `message` field, at
    // level `error`, so a fixture can still declare its expectation and then
    // print it:
    //
    // ```rust
    // //! EXPECT: the trait bound `String: LogValue` is not satisfied
    // fn main() {
    //     compile_error!("the trait bound `String: LogValue` is not satisfied");
    // }
    // ```
    //
    // Six lines, naming neither `nc_log` nor `LogValue` nor any log call, and
    // `11 fixture(s) refuse to compile, for the stated reason` — measured. A
    // rule asking "does the output hold this string" rather than "whose words
    // are these" only moves the channel, from stderr to `rendered` to
    // `message`.
    //
    // The `//!` header is not code: it is where the fixture *declares* what it
    // expects, so it is stripped before asking. What is left is what the
    // fixture compiles, and a sentence found in there is a sentence the fixture
    // wrote.
    // **The authorship rule lives in `diagnostics()`, not here.** A second copy
    // of it here cannot decide anything: `said.sentence` is
    // `format!("{code} {}", without_the_source(&message))`, and
    // `without_the_source` returns the diagnostic *JSON object* rather than a
    // sentence — so `code.contains(...)` asks whether the fixture's own source
    // holds an entire JSON blob, which no Rust file can. Instrumented over the
    // shipped fixtures: kept 15, dropped 0. `diagnostics()` compares against
    // the diagnostic's `message` field, which is the comparison that matters.
    // **The line rustc pointed at has to be a line that uses the door.** See
    // `caret_uses`: two fixtures were rewritten to fail for reasons that had
    // nothing to do with the wall, with `check passed` in both cases and the
    // wall deleted underneath one of them. The fixture writes its own code and
    // its own `EXPECT`; it does not write the caret's position.
    let at_the_door = |said: &&Said| the_refusal_is_the_doors(&name, &whole_fixture, said);
    let verdict = if output.status.success() {
        Outcome::Compiled
    } else if let Some(said) = message
        .iter()
        .find(|said| said.sentence.contains(&expected))
    {
        if !at_the_door(&said) {
            let _ = std::fs::remove_dir_all(scratch);
            return Ok(Outcome::FailedWithout(format!(
                "rustc refused it at line(s) {:?}, and none of those lines uses `{}` — \
                 so the refusal this counts as proof is not the wall's",
                said.at,
                crate::wall::SHAPES
                    .iter()
                    .find(|(shape, _, _, _)| *shape == name)
                    .map_or("", |(_, door, _, _)| door)
            )));
        }
        // **And the reason has to need the crate.** Every rule above is about
        // whose words the diagnostic carries; none asks whether `nc-log` had
        // anything to do with the refusal. `EXPECT: cannot call non-const
        // method `String::leak::<'_>` in constants` is satisfied by
        //
        // ```rust
        // const X: &str = String::new().leak();
        // fn main() { let _: Option<nc_log::Literal> = None; }
        // ```
        //
        // — two lines, naming `nc_log` in code as the rule above requires,
        // proving nothing about the wall. So the fixture is built a second time
        // against an **empty** crate of the same name: if the same sentence
        // comes back, the wall was not what refused it. A real fixture stops
        // resolving instead, and says so in some other words.
        outcome(fails_the_same_way_without_the_crate(
            scratch, fixture, &expected,
        )?)
    } else {
        Outcome::FailedWithout(format!(
            "it expects `{expected}` and rustc said: {}",
            message
                .first()
                .map_or("nothing that looks like an error", |said| said
                    .sentence
                    .as_str())
        ))
    };
    // **Removed when the work is done, not only before it starts.** The
    // directory carries the pid, so deleted only at the *top* of this function
    // every run of `check` leaves its twelve behind for ever — measured: 1246
    // directories, 139 distinct pids, 24 GB.
    let _ = std::fs::remove_dir_all(scratch);
    Ok(verdict)
}

/// Whether the refusal rustc gave is the one the fixture's door produces.
///
/// Apart from `build_fixture`, which with it inline would cross clippy's line
/// limit. `caret_uses` carries the argument for why the caret's position is
/// what answers this and the diagnostic's words are not.
fn the_refusal_is_the_doors(name: &str, whole_fixture: &str, said: &Said) -> bool {
    let Some((_, door, _, token)) = crate::wall::SHAPES
        .iter()
        .find(|(shape, _, _, _)| *shape == name)
    else {
        return false;
    };
    // **The whole file through the stripper, not each line on its own.**
    // Both halves below read the caret's line for a name, and `without_comment`
    // knows `//` and nothing else: read through it, a `/* … */` on the caret
    // line, or a string literal on it, pays for the token — measured,
    // one fixture given another's body with `accepts(typed); /* format! */`
    // left `the_wall_holds_and_the_proof_actually_ran` at `1 passed` and the
    // wall's whole section of the transcript byte-identical, while the same ten
    // characters written `// format!` were refused at once.
    //
    // `readable` blanks a comment and a string's contents character for
    // character, so every line keeps its length and its indentation and the
    // block walk below is unaffected. It is the treatment
    // `nothing_manufactures_a_static_string` already gives this file, sixty
    // lines up, for the same reason.
    let stripped = readable(whole_fixture);
    let lines: Vec<&str> = stripped.lines().collect();
    // **And the caret has to land on the thing this fixture is named for.**
    // The three columns above are the door, the sentence and the file's own
    // name, and *two pairs of rows carry identical doors and identical
    // sentences*: the two `Origin` fixtures, and the two that refuse a `String`.
    // So a fixture can be rewritten as its sibling — same header, same
    // `EXPECT:`, same door named in code, caret still inside the same block —
    // and prove what the sibling already proves while claiming to prove its own
    // thing. Measured: the `version` fixture given the `service` fixture's
    // body, then `pub version: Literal` demoted to `&'static str` with its four
    // construction sites followed — `ok 12 fixture(s) refuse to compile … each
    // was rebuilt against a crate of the same name with the guarantee removed`,
    // `check passed`, and a `OnceLock<String>` then put a tenant's address in
    // the `version` field of every line the log emits.
    //
    // *Deletion* is closed elsewhere — a fixture and its row go together and
    // both floors move. Neither floor moves when the file stays on disk and
    // stops proving what it is named for. The caret's position is the one thing
    // a fixture does not choose, which is the argument `caret_uses` already
    // makes; this asks *where* it landed rather than only whether the door is in
    // scope there.
    //
    // Two questions over the same set of lines, not one question over each: a
    // trait-bound refusal points at the *declaration* that imposes the bound and
    // at the *call* that violates it, so the door is named on one caret line and
    // the thing this fixture is about on another. Asking both of one line refuses
    // all four `LogValue` fixtures, which is the too-strong half.
    let names_the_door = said.at.iter().any(|line| caret_uses(&lines, *line, door));
    // **The subject half reads code too.** The door half above goes through
    // `caret_uses`, which spends the comment first — `CARETS` has a row named
    // *"a comment is not code"* and the meta-gate an operator over it — and a
    // subject half reading the raw line, three lines away, is one rule written
    // twice with the second copy narrower, which is the shape this file's own
    // header names. Measured: the `version` fixture given the `service`
    // fixture's body with `// version` appended to a caret line, then
    // `pub version: Literal` demoted to `&'static str` with its four
    // construction sites followed — `check passed`, every test green, the wall
    // section byte-identical, and a consumer crate outside the workspace then
    // put a tenant's email address in the `version` field of every line the
    // log emits. Deleting the ten characters of that comment and nothing else
    // is RED. The lines are already the stripper's, so the subject is looked
    // for in what is left: `without_comment` here would be a second rule about
    // the same text, and an operator planting it away is an equivalent mutant
    // once the whole file goes through `readable`.
    let names_its_subject = said.at.iter().any(|line| {
        lines
            .get(line.saturating_sub(1))
            .is_some_and(|said| said.contains(token))
    });
    names_the_door && names_its_subject
}

/// Whether a fixture is about the shape its own name claims.
///
/// Two questions, both from `SHAPES`: does its code name the **door** it is
/// named for, and does its expectation name the **subject** its filename says is
/// refused. Each was measured being walked past — a four-line file proving a
/// newtype has no `Default`, and four identical files about a locally declared
/// `struct Mine` — with the whole suite green and the wall removed underneath.
fn not_about_its_own_shape(name: &str, code: &str, expected: &str) -> Option<String> {
    let (_, door, subject, _) = crate::wall::SHAPES
        .iter()
        .find(|(shape, _, _, _)| *shape == name)?;
    // **In code, which a comment is not.** Asked as `code.contains(door)`
    // against the file minus its `//!` header, `// nc_log::LogValue` — a
    // plain comment — satisfies "it names the door in its code", while the rule
    // beside it strips comments before asking its own question. Measured: four
    // fixtures about a locally declared trait, each naming the door only in a
    // comment, every fixture green, and the type wall deleted underneath them.
    if !readable(code).contains(door) {
        return Some(format!(
            "it never names `{door}` in its code, so whatever it proves is not about the \
             door it is named for"
        ));
    }
    if expected != *subject {
        return Some(format!(
            "its expectation is `{expected}` and this list says it must be `{subject}`. \
             A fixture that changes what it expects has changed what it proves"
        ));
    }
    None
}

/// Whether the fixture's stated reason survives `nc-log` being emptied.
///
/// The same directory, the same fixture, the same expectation — and two crates
/// of the same names holding nothing. A refusal that is really the wall's stops
/// happening: the paths the fixture names no longer resolve, so rustc says
/// something else entirely. A refusal that had nothing to do with the wall says
/// exactly what it said before, and that is what this returns `Some(true)` for.
///
/// **`None` is the control that did not run**, not `false`, which is
/// indistinguishable from "the wall refused it". Every way the control can fail
/// to decide returns `None`, and the caller counts them, because a step that
/// did not run is not a step that passed.
fn fails_the_same_way_without_the_crate(
    scratch: &Path,
    fixture: &Path,
    expected: &str,
) -> Result<Option<bool>, String> {
    let hollow = scratch.join("hollow");
    for name in ["nc-log", "nc-catalog"] {
        let crate_root = hollow.join(name);
        std::fs::create_dir_all(crate_root.join("src"))
            .map_err(|error| format!("could not prepare a hollow crate: {error}"))?;
        std::fs::write(
            crate_root.join("Cargo.toml"),
            // The hollow `nc-log` depends on the hollow `nc-catalog`, exactly
            // as the real one does: `Event::level` returns `nc_catalog::Level`,
            // so a stub that declares `Event` and no dependency does not
            // compile — and a stub that does not compile puts *every* fixture
            // at name resolution, which is the failure an empty crate gives.
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
                 [lib]\npath = \"src/lib.rs\"\n{}",
                if name == "nc-log" {
                    "\n[dependencies]\nnc-catalog = { path = \"../nc-catalog\" }\n"
                } else {
                    ""
                }
            ),
        )
        .map_err(|error| format!("could not write a hollow manifest: {error}"))?;
        // **A stub, not an empty crate.** An empty `nc-log` makes every fixture
        // fail at *name resolution* — `not_about_its_own_shape` requires each to
        // name `nc_log::<door>` — so rustc aborts before trait selection, const
        // evaluation and macro expansion, which is where every `EXPECT`
        // sentence comes from, and the control can decide none of them.
        //
        // The stub declares the same paths with **the guarantee removed**: a
        // `LogValue` everything implements, public fields, no sealing. So a
        // fixture that is refused *by the wall* compiles here, and one that
        // is refused by something else — a plain const-eval error needing no
        // `nc-log` at all, measured passing the whole gate without this —
        // brings its own refusal with it and is caught.
        let named = fixture
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        std::fs::write(crate_root.join("src/lib.rs"), stub_for(name, &named))
            .map_err(|error| format!("could not write a hollow crate: {error}"))?;
    }
    let quoted = |name: &str| {
        hollow
            .join(name)
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    };
    std::fs::write(
        scratch.join("Cargo.toml"),
        format!(
            "[workspace]\n\n[package]\nname = \"nc-wall-fixture\"\nversion = \"0.0.0\"\n\
             edition = \"2024\"\n\n[dependencies]\nnc-log = {{ path = \"{}\" }}\n\
             nc-catalog = {{ path = \"{}\" }}\n",
            quoted("nc-log"),
            quoted("nc-catalog")
        ),
    )
    .map_err(|error| format!("could not write the hollow manifest: {error}"))?;

    let program = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let output = Command::new(program)
        // **`--offline`, because this gate must not fetch.** The scratch manifest
        // carries no lockfile, so without the flag cargo resolves it against
        // crates.io on every run — a fresh resolution per fixture per
        // `cargo xtask check`, `--offline` or not. With no network the
        // resolution fails, its error goes to *stderr* while
        // `--message-format=json` leaves stdout empty, and the gate then
        // reports every fixture as failing for the wrong reason:
        //
        // ```text
        // error crates/nc-log/fixtures/a_string_cannot_be_a_log_value.rs:1 —
        //       … did not fail for the reason it declares
        //       rustc said: nothing that looks like an error
        // error check failed: 11 finding(s) about the compile-fail fixtures
        // ```
        //
        // The word *network* appeared nowhere in the run. Every package this
        // build needs is on the machine by the time it runs: `cargo xtask check`
        // runs `cargo fetch --locked` before any step, and under `--offline`,
        // where nothing is fetched, a package missing from the machine has
        // already failed the `wiring` step, which names that fetch as the fix.
        .args(["build", "--quiet", "--offline", "--message-format=json"])
        .env("CARGO_TARGET_DIR", scratch.join("hollow-target"))
        .current_dir(scratch)
        .output()
        .map_err(|error| format!("could not compile the fixture without the crate: {error}"))?;
    // A fixture that *compiles* against nothing would be a strange thing, and it
    // is not this rule's business: what it asks is whether the sentence came
    // back.
    let rendered = String::from_utf8_lossy(&output.stdout).into_owned();
    let code: String = std::fs::read_to_string(fixture)
        .map_err(|error| format!("could not read the fixture: {error}"))?
        .lines()
        .filter(|line| !line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    let said = diagnostics(&rendered, &code);
    // **`cargo` failing is not the fixture compiling.** Without reading
    // `output.status`, an empty `said` is taken to mean the hollow build
    // succeeded — so every way *cargo itself* can fail scores as proof. The
    // paragraph above documents exactly this for `--offline`: the error goes to
    // stderr and `--message-format=json` leaves stdout empty. A manifest typo,
    // an unresolvable path dependency, a missing dependency under `--offline`,
    // a target directory that cannot be written: each leaves `check` printing
    // every fixture proven, with no warn, no skip and exit 0.
    //
    // Measured with one letter added to the hollow manifest's path dependency
    // on the catalog stub: the wall's whole section was byte-identical to a
    // clean run.
    let verdict = control_verdict(output.status.success(), &said, expected);
    // **One expression, because a seam between two returns is a forgery's way
    // in.** Written as `if let Some(same) = answer(&verdict) { return
    // Ok(Some(same)); }` followed by a bare `Ok(None)` under the warn, the
    // trailing literal is pinned by nothing: measured, `Ok(None)` written
    // `Ok(Some(false))` left every test passing, and with the hollow manifest
    // broken beside it `check` printed eleven `skipped: the hollow build did
    // not compile…` lines, then `ok 11 fixture(s) refuse to compile … and each
    // was rebuilt against a crate of the same name with the guarantee
    // removed`, then `check passed` — while `answer`, `outcome`,
    // `controlled_by` and `control_verdict` were each pinned.
    //
    // So there is nothing here to pin: the answer is taken once and handed
    // back, and the announcement is what happens *when there is no answer*
    // rather than a second path that produces one.
    // `a_control_that_could_not_run_answers_none` drives this function itself
    // against a hollow build cargo cannot run, and reads what comes back.
    let answered = answer(&verdict);
    if answered.is_none() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("cargo wrote nothing to stderr");
        crate::ui::warn(&announcement(&verdict, first));
    }
    Ok(answered)
}

/// What a control that did not run says, and what it hands back.
///
/// **The verdict and its *use*, both driven.** `control_verdict` has four
/// variants and a test driving every one; an arm turning them into an answer,
/// as a hand-written match nothing reads, is one token from
/// `Control::Decided(_) => Ok(Some(false))` — measured, every `wall::` test,
/// the whole workspace and the gate green, with `controlled == checked == 11`
/// still printed, and a fixture forged past every other rule in this module
/// shipped through that token.
///
/// `Decided` is the only variant that answers `Some`, and the boolean it
/// carries is passed through untouched. Every other variant is a step that did
/// not run, and says which.
fn announcement(did_not_run: &Control, cargo_said: &str) -> String {
    match did_not_run {
        Control::Decided(_) => unreachable!("a decided control is not an announcement"),
        Control::NoBuild => format!(
            "skipped: the hollow build did not compile and reported no diagnostic, so \
             the empty-crate control could not be asked of this fixture — cargo said: \
             {cargo_said}"
        ),
        Control::OnlyResolution => "skipped: the hollow build failed at name resolution, \
             so the empty-crate control could not be asked of this fixture"
            .to_owned(),
        Control::SomeOtherReason => "skipped: the hollow build failed for a reason that \
             is neither the expected one nor a name that failed to resolve, so the \
             empty-crate control could not be asked of this fixture"
            .to_owned(),
    }
}

/// The answer a verdict hands back: `Some` only when the control decided.
///
/// Separate from the announcing so it can be driven for every variant without a
/// terminal — this is the mapping a forged fixture goes through.
fn answer(verdict: &Control) -> Option<bool> {
    match verdict {
        Control::Decided(same) => Some(*same),
        _ => None,
    }
}

/// What the empty-crate control concluded, as a value rather than as control flow.
///
/// **The exit-status guard, and a test for it.** Measured: breaking the hollow
/// manifest *and* switching the guard to `if false && said.is_empty()` left
/// `the_wall_holds_and_the_proof_actually_ran` passing with
/// `controlled == checked == 11` — eleven rebuilds that never happened, reported
/// as eleven that did — and removing the guard on its own was invisible to
/// every other `wall::` test.
///
/// Split out so every branch is reachable from a test without a subprocess: the
/// thing worth holding is the decision, and the decision needs neither cargo nor
/// a filesystem.
#[derive(Debug, PartialEq, Eq)]
enum Control {
    /// The control ran. `true` is "it fails the same way with no crate behind it".
    Decided(bool),
    /// cargo itself failed: a manifest it cannot load, a dependency it cannot
    /// resolve, a target directory it cannot write. The error goes to stderr and
    /// `--message-format=json` leaves stdout empty, so an empty `said` here is
    /// *no build*, not a build that compiled.
    NoBuild,
    /// Every diagnostic was a name that failed to resolve, so rustc stopped
    /// before the guarantee could refuse anything.
    OnlyResolution,
    /// It failed, and for something else — the stub declares the door's name and
    /// not its shape, and rustc stopped at that instead.
    SomeOtherReason,
}

/// What the hollow build concluded, from its exit status and its diagnostics.
///
/// The order is the order of the questions: *did anything build at all*, then
/// *did rustc get past name resolution*, then *did the expected sentence come
/// back*. Reversing the first two makes an empty diagnostic list mean
/// "it compiled" whether or not cargo ran.
fn control_verdict(succeeded: bool, said: &[Said], expected: &str) -> Control {
    if said.is_empty() {
        return if succeeded {
            // Compiled against the stub: the refusal really is the wall's.
            Control::Decided(false)
        } else {
            Control::NoBuild
        };
    }
    if said.iter().all(|one| {
        [
            "E0405", "E0412", "E0422", "E0425", "E0426", "E0432", "E0433", "E0531", "E0603",
        ]
        .iter()
        .any(|code| one.sentence.starts_with(code))
    }) {
        return Control::OnlyResolution;
    }
    if said.iter().any(|one| one.sentence.contains(expected)) {
        return Control::Decided(true);
    }
    Control::SomeOtherReason
}

/// `Box::leak`, pre-formatting, and text made into an identifier, at a logging call site. // allow-logging: the gate naming what it refuses
///
/// The one way round the type wall: `format!` produces a `String`, and leaking
/// it produces a `&'static str` the trait accepts. §15.4 asks for exactly this
/// gate, and without it the wall is a rule rather than a guarantee.
///
/// Returns the findings **and how many logging calls it opened**, which is the
/// number `check` reports — not how many *files* it walked, because a file is
/// not a call: a count of files is non-zero from the first source file in the
/// tree whether or not a single logging call is ever read. That is `§19.3`'s
/// first failure exactly, one level in: the gate wired up, and the number
/// proving it measuring something else.
pub(crate) fn no_formatting_at_a_logging_call(
    file: &str,
    text: &str,
    compiled_into_the_log: bool,
    elsewhere_in_the_crate: &[String],
) -> (Vec<Finding>, usize) {
    // The leak scan is a rule about every source file, and this is the entry
    // point for the ones that may log — so it runs both, and `check` runs the
    // leak scan alone over everything outside this scope.
    let (mut findings, _, _) =
        nothing_manufactures_a_static_string(file, text, elsewhere_in_the_crate);
    findings.extend(no_identifier_is_minted_from_text(file, text));
    findings.extend(no_public_constructor_takes_a_value(
        file,
        text,
        compiled_into_the_log,
    ));
    let mut calls = 0_usize;
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in text.lines().enumerate() {
        // Its own marker, with a mandatory reason. Not the reference gate's:
        // the two say different things, and a line excused from one has not been
        // looked at by the other.
        //
        // **Counted before it is excused.** `calls` is the number `check`
        // prints as evidence this gate is wired up; subtracting every excused
        // call site from it makes a file whose logging calls are all excused
        // report the same number as a file with none, and using the hatch
        // lower the number that exists to prove coverage. The floor is on files
        // read, so nothing else goes red.
        //
        // An excused call is one this gate looked at and was told not to judge.
        // That is still a call it inspected.
        let excused =
            crate::references::marked_in(line, ALLOW, crate::references::Comments::Slashes);
        let opens = OPENS_A_LOGGING_CALL
            .iter()
            .any(|needle| without_comment(line).contains(needle));
        // **An excused opener is still a call to read.** A marker that skips
        // the whole window from here lets one comment excuse twenty lines. It
        // excuses its own line — the per-line test inside the
        // window below is what does that — and the call is read either way.
        if excused && !opens {
            continue;
        }
        // **The call, not the line.** Asking whether one line holds both
        // `.record(` and a `format!` misses that rustfmt breaks a call past 100
        // columns, which is what a real logging call site looks like. Measured:
        // `log.record(&E { table: format!("{user}") });` is a finding and the
        // same call wrapped over three lines is not. So the window is the call:
        // from `.record(` to the line its parentheses close on.
        let Some(call) = OPENS_A_LOGGING_CALL
            .iter()
            .find_map(|needle| call_from(&lines, index, needle))
        // allow-logging: this gate's own rule, which reads a whole call and therefore reads itself
        else {
            continue;
        };
        // **Counted only when it was read to the end.** `calls` is the number
        // `check` prints as evidence this gate is wired up, and a call the
        // window gives up on is not a call it read.
        if !call.whole {
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: "this logging call is too long for the gate to read whole".to_owned(),
                fix: "the window follows a call until its parentheses close, for at most \
                  twenty lines — past that it stops rather than reading the rest of \
                  the file, and what it did not read it cannot judge. Shorten the \
                  call, or take the value out into a `let` above it"
                    .to_owned(),
            });
            continue;
        }
        calls += 1;
        let ends = call.ends;
        let call = call.text;
        // **On the needle's own line, not on the call's first.** A marker
        // tested on the opening line, with the finding computed from a window
        // of up to twenty lines after it, lets one comment beside `.record(`
        // excuse every `format!`, `.to_string()` and `String::from` in the
        // whole call — measured, nine needles over nine lines from one marker.
        // The rule this gate states about itself is "same line only, because
        // rustfmt is free to move a comment that sits above a statement"; this
        // is that rule applied.
        let _ = &call;
        let window = &lines[index..=ends.min(lines.len().saturating_sub(1))];
        for (offset, within) in window.iter().enumerate() {
            let at = index + offset;
            let excused_here =
                crate::references::marked_in(within, ALLOW, crate::references::Comments::Slashes);
            let code = without_comment(within);
            // **And the reason names the needle.** The marker on the needle's
            // own line narrows twenty lines to one; on that one line a reason of
            // a single letter would still excuse three of the spellings at once.
            // The paragraph two rules along states the rule — *the reason has
            // to name what it excuses* — and this marker applies it like every
            // other.
            for needle in BUILDS_TEXT.iter().filter(|needle| code.contains(**needle)) {
                if excused_here
                    && crate::references::reason_after(
                        within,
                        ALLOW,
                        crate::references::Comments::Slashes,
                    )
                    .is_some_and(|reason| crate::references::names(reason, needle))
                {
                    continue;
                }
                findings.push(Finding {
                    file: file.to_owned(),
                    line: at + 1,
                    what: format!("`{needle}` builds text at a logging call site"),
                    fix: format!(
                        "a log event carries declared fields, never a sentence a caller \
                         wrote — the description a reader sees lives in \
                         `catalog/log-events.toml` beside the declaration. Pass the \
                         values themselves and let the registry say what they mean. If \
                         this one cannot, say so with `{ALLOW}` on this line and name \
                         `{needle}` in the reason, because a marker that names nothing \
                         excuses whatever else lands beside it"
                    ),
                });
            }
            // **And text made into an identifier, in the same loop and with the
            // same hatch.** Measured on a form of this rule that read the whole
            // call rather than the line, reported the call's opening line
            // whatever line the needle was on,
            // and — alone among the four — had **no `allow-logging:` hatch at
            // all**, the "too strong" half of a repair:
            // `Duration::new(0, 21_000_000)` is a std constructor
            // taking two integers, for a kind the registry declares on five of
            // its eleven events, and it was refused as *turns text into an
            // identifier* with no way to appeal. `OPENS_A_LOGGING_CALL` also
            // matches `Journal::record` and `WorkUnit::record`, which a crate
            // that reaches `nc-log` calls too — the kernel journals every
            // dispatch — so the shape of the five `.record(Record::new(` call
            // sites in `nc-journal`'s tests would be, without a hatch, an
            // unappealable finding in any crate this wall reads.
            //
            // A hatch does not weaken the rule: the reason still has to name the
            // needle, and the marker still works on the needle's own line only.
            an_identifier_minted_at_a_call(file, within, code, at, excused_here, &mut findings);
        }
        // **And text made into an identifier, which is the same move one type
        // further along.** Every declared field type is a type, and two of them
        // have public constructors that take arbitrary text: `OpaqueId::new`,
        // and `Snowflake::from_crockford`, whose reader is deliberately lenient
        // — case-insensitive, `O` for `0`, `I` and `L` for `1` — so a caller
        // need not even know the alphabet. Measured, from an ordinary consumer
        // crate with no `format!`, no `.to_string()`, no `.leak()` and no macro:
        //
        // ```text
        // let unit = nc_ids::Snowflake::from_crockford(&user_text)?;
        // log.record(&JournalAppended { sequence: 1, work_unit: Some(unit) });
        //   -> {"_msg":"journal.appended",…,"work_unit":"CPF123456789"}
        // ```
        //
        // Twelve characters of caller-chosen runtime text, verbatim, through
        // the kind the registry recommends. The refusal `load.rs` writes for
        // `type = "id"` — "a caller can mint one out of a person's name or
        // number and the type wall cannot tell" — applies word for word to the
        // kind it steers callers towards.
        //
        // **A type cannot close this and a source gate can**, for the same
        // reason it closes `format!`: the conversion is visible where it
        // happens. What neither closes is a caller who converts somewhere else
        // and logs the result, and `crates/nc-log/src/value.rs` says so
        // rather than claiming a guarantee the mechanism does not give.
    }
    (findings, calls)
}

/// The identifier-minting rule, for one line of a logging call's window.
///
/// **Its own function because `no_formatting_at_a_logging_call` would otherwise
/// cross clippy's hundred-line bound**, and in the window loop because that is
/// where it belongs: outside it the rule is made to read the whole call, report
/// the call's opening line whatever line the needle is on, and — alone among the four rules here —
/// carry **no `allow-logging:` hatch at all**.
///
/// That is the "too strong" half of a repair, and it is measured:
/// `Duration::new(0, 21_000_000)` is a std constructor taking two integers, for
/// a kind the registry declares on five of its eleven events, and it was refused
/// as *turns text into an identifier* with no way to appeal. `OPENS_A_LOGGING_CALL`
/// also matches `Journal::record` and `WorkUnit::record`, which a crate that
/// reaches `nc-log` calls too — the kernel journals every dispatch — so the
/// shape of the five `.record(Record::new(` call sites in `nc-journal`'s tests
/// would be, without a hatch, an unappealable finding in any crate this wall
/// reads.
///
/// The hatch does not weaken the rule: the reason still has to name the needle,
/// and the marker still works on the needle's own line only.
fn an_identifier_minted_at_a_call(
    file: &str,
    within: &str,
    code: &str,
    at: usize,
    excused_here: bool,
    findings: &mut Vec<Finding>,
) {
    for minted in MINTED_FROM_TEXT
        .iter()
        .filter(|needle| code.contains(**needle))
    {
        if excused_here
            && crate::references::reason_after(within, ALLOW, crate::references::Comments::Slashes)
                .is_some_and(|reason| crate::references::names(reason, minted))
        {
            continue;
        }
        findings.push(Finding {
            file: file.to_owned(),
            line: at + 1,
            what: format!("`{minted}` turns text into an identifier at a logging call site"),
            fix: format!(
                "a declared field type is not a guarantee when the value is built out of \
                 text at the call — `Snowflake::from_crockford` takes twelve caller-chosen \
                 characters and writes them to the store verbatim, which is what \
                 `type = \"id\"` is refused for, and `FileName::parse` reaches the same \
                 constructor one hop away. Log an identifier the platform minted, or \
                 declare what you mean as a field of its own. If this `{minted}` cannot \
                 carry caller text — `Duration::new` is the case — say so with `{ALLOW}` \
                 on this line and name `{minted}` in the reason"
            ),
        });
    }
}

/// The spellings that open a logging call.
///
/// **`.record(` is one of them, not all of them.** With it alone,
/// `nc_log::Log::record(&log, …)` — the associated-function form, which is
/// ordinary Rust and what a macro expands to — names no window at all, so
/// everything inside it is invisible.
/// Measured, with a `from_crockford` in the argument: `findings=0 calls=0` for
/// the associated form against `findings=1 calls=1` for the method form.
///
/// **What this still does not reach, said plainly:** a consumer's own wrapper —
/// `fn emit(&self, event: &impl Event) { self.log.record(event) }` — moves every
/// call site out of the window, because the `from_crockford` is then at a call
/// to `emit` and the `record` is somewhere else entirely. The wrapper's *own*
/// body is still read, so the door it opens is a door in that crate's source
/// where a reader of it meets it. A gate that followed method definitions
/// would be a type checker, and this is a line reader; `value.rs`'s header says
/// which half of `§15.4` the types hold and which half this holds, and this is
/// the edge of the second half.
///
/// **The name of the type is the caller's to choose.** A needle of
/// `Log::record(` is renamed away by `use nc_log::Log as Journal;` in one line
/// — measured, with an ordinary leak at the call site:
///
/// ```text
/// Journal::record(log, &nc_log::events::JournalAppended { … from_crockford(text) … })
///   -> ok 1 crate(s) outside nc-log may log … check passed
/// ```
///
/// and the gate went on to *assert* that the crate "has not logged yet", in a
/// file it had read and counted. `::record(` is the same call however the path
/// in front of it is spelled, including through an alias, a re-export or the
/// full path. `.record(` stays for the method form on a value.
///
/// **And `Log::line` is `record` without the shipping.** `record` is literally
/// `self.shipper.send(self.line(event))`, and `line` is `pub` and takes the
/// same `&impl Event` — so without `.line(` here the same argument, in the same
/// crate, one method along, opens no window:
///
/// ```text
/// .record( -> findings=1 calls=1
/// .line(   -> findings=0 calls=0     (with a `format!` in the argument)
/// ```
///
/// The value reaches the same JSON either way; what `line` skips is the queue.
/// A gate that reads one and not the other is a gate about a method name.
const OPENS_A_LOGGING_CALL: [&str; 4] = [".record(", "::record(", ".line(", "::line("];

/// Constructors that turn arbitrary text into a type the log accepts.
///
/// **This list is not complete by argument.** *"These two and no others,
/// because these two are all there are"* misses a third route:
/// `nc_ids::naming::FileName::parse(text)` reaches `Snowflake::from_crockford`
/// inside a crate this gate does not read, and hands the result back through
/// `FileName::id()`. Measured from an ordinary consumer crate outside the
/// workspace, with no `format!`, no `.leak()`, no macro and no `unsafe`, the two
/// spellings put the same twelve caller-chosen characters in the store; one was
/// refused and the other was not.
///
/// A Denote name is user data and a first-class concept here (`§16.3`), so
/// `FileName::parse(name)?.id()` is the *natural* way a later stage obtains a
/// work-unit id — and **an enumeration cannot argue itself complete.**
/// What makes this list trustworthy is not its own paragraph but
/// `every_public_route_to_an_identifier_is_named`, which reads `nc-ids`'
/// public surface and refuses a route this list does not cover.
///
/// Every other field type the registry allows — `u64`, `i64`, `bool`,
/// `Duration`, `ErrorCode`, a declared variant, a source literal — has no
/// constructor that takes caller text.
///
/// **`::parse(` and not `FileName::parse(`**, for the reason `OpaqueId::new(`
/// gives below: a path is the caller's to choose, and `use … FileName as F;`
/// then `F::parse(x).id()` is the same call. Inside a logging call's
/// parentheses the bare form means one thing; away from one it would mean every
/// parser in the tree, which is why the whole-file list spells it with the type.
///
/// There is no needle for `Snowflake::from_u64`, a function that does not
/// exist: it would be a needle matching nothing — a gate wired to nothing at
/// the granularity of one line, which is the failure `§19.3` names
/// and which is easiest to commit in the list a gate reads from. Each of these
/// is spelled with enough of its path that an unrelated `new(` cannot match it.
///
/// **`::new(`, not `OpaqueId::new(`, because a path is the caller's to
/// choose.** `from_crockford(` matches whatever precedes it; a path-spelled
/// needle does not, so `use nc_ids::OpaqueId as Who;` then `Who::new(text)?` at
/// a logging call site is invisible to it — the identical alias move
/// `OPENS_A_LOGGING_CALL` answers, applied to this list instead. Measured:
///
/// ```text
/// the spelling the gate knew       calls=1 findings=1
/// the same call through an alias   calls=1 findings=0
/// a Snowflake through an alias     calls=1 findings=1   (for contrast)
/// ```
///
/// Latent while the registry refuses `type = "id"`, and live the day `§5.1`
/// closes and that refusal goes — which is what the refusal's own comment says
/// will happen. `::new(` is the constructor however the type in front of it is
/// spelled; a bare `new(` would match every constructor in the tree, which is
/// the balance this list's own paragraph is about.
const MINTED_FROM_TEXT: [&str; 3] = ["from_crockford(", "::new(", "::parse("];

/// Every shape the wall must refuse, one fixture each — by name.
///
/// **The gate's floor, not just the test's.** A walk compared against a second
/// listing of the same directory can only disagree with it if `read_dir` itself
/// is broken — measured, deleting ten of eleven fixtures printed
/// `ok 1 fixture(s) refuse to compile` and passed.
///
/// What a written list can do, stated exactly: it makes removing a fixture a
/// **two-place edit**, one of them here, where a reader of the diff meets
/// the claim that the door was reopened on purpose. It cannot stop somebody
/// making both edits: a constant in the file the walk's author is editing does
/// not come "from somewhere the walk cannot reach". There is no git here (`§5.3`), so nothing
/// outside the tree remembers; the failure this replaces was silent, which is
/// the whole of the improvement.
///
/// **The second column is the door and the third is the subject**, because a
/// fixture can pass every rule and still be about nothing. The whole of one, measured green:
///
/// ```rust,ignore
/// //! EXPECT: the trait bound `Literal: Default` is not satisfied
/// fn main() {
///     let _: nc_log::Literal = Default::default();
/// }
/// ```
///
/// It names `nc_log` in code, it names an identifier it does not declare, it
/// fails differently against a hollow crate — and it proves only that a newtype
/// has no `Default`. So each fixture declares, here, the surface it must
/// exercise, and a fixture rewritten to be about something else stops naming
/// its door.
///
/// **And the door alone is not enough, because a bound is a use.** Four of these
/// were replaced by four identical files about a locally declared `struct Mine`,
/// each keeping `fn accepts<T: nc_log::LogValue>` so the door was named, each
/// expecting `` the trait bound `Mine: LogValue` is not satisfied `` — and with
/// `LogValue` then implemented for `String`, `&str` and `&String`, the run
/// printed `ok 11 fixture(s) refuse to compile, for the stated reason` and
/// `check passed`, with a `String` logged from a crate outside the workspace.
///
/// **And naming the subject is not enough either.** A hollow `nc-log` makes
/// `nc_log::LogValue` fail *name resolution*, and rustc stops before type
/// checking — so any unrelated type error in the same file disappears from the
/// hollow build and "fails differently" for free. All four string fixtures were
/// rewritten to expect `` `String` is not an iterator `` over
/// `Iterator::next(&mut text)`, which names the subject, names the door in a
/// bound, clears every rule — and the wall was then deleted with
/// `ok 11 fixture(s) refuse to compile, for the stated reason` and a `String`
/// logged from outside the crate.
///
/// So the third column is the **whole sentence** the fixture must expect. It is
/// written twice on purpose, here and in the file: changing what a fixture
/// proves is then a two-place edit, and the second place is a list a reader of
/// the diff reads. That is the argument the first column already makes, applied
/// to the thing that is editable.
pub(crate) const SHAPES: [(&str, &str, &str, &str); 12] = [
    (
        "a_string_cannot_be_a_log_value",
        "nc_log::LogValue",
        "the trait bound `String: LogValue` is not satisfied",
        "name",
    ),
    (
        "a_borrowed_str_cannot_be_a_log_value",
        "nc_log::LogValue",
        "the trait bound `&str: LogValue` is not satisfied",
        "borrowed",
    ),
    (
        "a_formatted_string_cannot_be_a_log_value",
        "nc_log::LogValue",
        "the trait bound `String: LogValue` is not satisfied",
        "format!",
    ),
    (
        "a_reference_to_a_string_cannot_be_a_log_value",
        "nc_log::LogValue",
        "the trait bound `&String: LogValue` is not satisfied",
        "&owned",
    ),
    (
        "a_leaked_string_cannot_be_a_literal",
        "nc_log::literal!",
        "no rules expected `leaked`",
        "leaked",
    ),
    (
        "an_outside_crate_cannot_be_an_event",
        "nc_log::Event",
        "the trait bound `Mine: nc_log::sealed::Sealed` is not satisfied",
        "Mine",
    ),
    // The three doors beside the wall, each of which can be reopened with
    // the whole suite green without a fixture of its own.
    (
        "a_leaked_string_cannot_reach_a_literal",
        "nc_log::Literal",
        "no associated function or constant named `from_macro` found for struct `Literal`",
        "from_macro",
    ),
    (
        "a_field_cannot_be_built_from_outside",
        "nc_log::Field",
        "tuple variant `Id` is private",
        "Id",
    ),
    (
        "a_variant_cannot_be_built_from_outside",
        "nc_log::Variant",
        "you might have meant to use the `of` associated function",
        "Variant",
    ),
    // The two gates *beside* the wall, each a rule `check` would not notice
    // being undone without its fixture.
    // **The path a consumer has, not the one the crate root re-imports
    // privately.** `nc_log::Shipper` goes through `use ship::Shipper;`, which is
    // private, so it is `E0603` whatever `ship.rs` says — a fixture naming it is
    // baselined where it cannot fail, and the door reopens at
    // `nc_log::ship::Shipper` with the whole tree green, measured.
    (
        "the_shipper_is_not_a_public_door",
        "nc_log::ship::Shipper",
        "struct `Shipper` is private",
        "Shipper",
    ),
    (
        "an_origin_cannot_hold_a_runtime_string",
        "nc_log::Origin",
        "expected `Literal`, found `&str`",
        "service",
    ),
    // **The `version` field, with a fixture of its own.** With `Origin`'s two
    // fields one type and one fixture named only `service`: measured,
    // `pub version: Literal` demoted to
    // `&'static str` with its four construction sites followed left
    // `the_wall_holds_and_the_proof_actually_ran` at `1 passed`, every other
    // fixture still refusing and the control deciding each. `version` is
    // on every line the log emits, and a `static OnceLock<String>` yields a
    // genuine `&'static str` out of run-time input with no leak spelling
    // anywhere.
    (
        "an_origin_version_cannot_hold_a_runtime_string",
        "nc_log::Origin",
        "expected `Literal`, found `&str`",
        "version",
    ),
];

/// How many shapes the type wall refuses, written down.
///
/// **A floor nothing else holds.** `assert!(checked >= SHAPES.len())` is a
/// count compared against itself, and `inspected_all(checked, SHAPES.len(), …)`
/// is the same comparison one file along — so without this a fixture and its
/// row can be deleted together and the gate reports
/// `ok 10 fixture(s) refuse to compile` over a wall with one guarantee fewer.
/// See the assertion that reads this for what that measured.
///
/// Changing it is the deliberate act; a table that shrinks on its own is
/// not.
pub(crate) const THE_SHAPES_THE_WALL_REFUSES: usize = 12;

/// The predicate, so `corpus.rs` can decide it case by case.
///
/// A door rather than a copy: the corpus is what `xtask/mutants.py` weakens the
/// gate against, and a corpus deciding a reimplementation would measure the
/// reimplementation.
#[cfg(test)]
pub(crate) fn caret_uses_for_tests(lines: &[&str], line: usize, door: &str) -> bool {
    caret_uses(lines, line, door)
}

#[cfg(test)]
mod tests {
    /// A span in another file is not a line of the fixture.
    ///
    /// **Without a file name, another file's span reads as the fixture's.**
    /// Most of `nc-log`'s refusals carry secondary spans in `value.rs` and
    /// `ship.rs`, and with no file name consulted their line numbers are read as
    /// the fixture's — measured, a ten-line fixture reported as
    /// *"rustc refused it at line(s) [9, 190, 191]"*, and padding its header
    /// until the door landed on line 190 made the same code pass with not one
    /// character of it changed.
    #[test]
    fn a_span_in_another_file_is_not_a_line_of_the_fixture() {
        let diagnostic = concat!(
            r#"{"spans":[{"file_name":"src/main.rs","line_start":9},"#,
            r#"{"file_name":"/w/crates/nc-log/src/value.rs","line_start":190},"#,
            r#"{"file_name":"/w/crates/nc-log/src/ship.rs","line_start":191}]}"#,
        );
        assert_eq!(
            super::pointed_at(diagnostic),
            vec![9],
            "a line from another crate's source was read as the fixture's own"
        );
        // The floor: a diagnostic naming only the fixture keeps every line.
        let ours = r#"{"spans":[{"file_name":"src/main.rs","line_start":9},{"file_name":"src/main.rs","line_start":12}]}"#;
        assert_eq!(super::pointed_at(ours), vec![9, 12]);
    }

    /// A token written in a comment is not a fixture naming its subject.
    ///
    /// **Both halves of this rule spend comments, and this drives the subject
    /// half.** `CARETS` has rows for `caret_uses` — one of them named *"a
    /// comment is not code"* — and the meta-gate an operator over it; the
    /// subject half, three lines away, needs a driver of its own, or it is one
    /// rule written twice with the second copy narrower, which is the shape
    /// this file's header names.
    ///
    /// What it costs undriven is criterion 2 itself: measured, a fixture given
    /// a sibling's body with `// version` appended to a caret line satisfied the
    /// rule, and
    /// `Origin::version` was then demoted from `Literal` to `&'static str` with
    /// `check passed` and every count in the wall's own section unmoved. A
    /// consumer outside the workspace put run-time text in the `version` field
    /// of every line the log emits.
    ///
    /// Both directions, because a rule that refuses the comment must still
    /// accept the code: the same token in code on the same line is the shape
    /// every shipped fixture has.
    #[test]
    fn a_token_in_a_comment_does_not_name_a_fixture_s_subject() {
        // The `service` fixture's shape: it names the door in code, and the
        // subject either in code or in a comment.
        for (how, line) in [
            (
                "in a comment",
                "    let held = Origin::new(name); // service",
            ),
            ("in code", "    let held = Origin { service: name };"),
        ] {
            let fixture = format!("use nc_log::Origin;\nfn main() {{\n{line}\n}}\n");
            let said = super::Said {
                sentence: "expected `Literal`, found `&str`".to_owned(),
                // Both the line above and the line itself, because a trait-bound
                // refusal points at the declaration and at the call: the door is
                // named on one caret line and the subject on another.
                at: vec![1, 3],
            };
            let decided = super::the_refusal_is_the_doors(
                "an_origin_cannot_hold_a_runtime_string",
                &fixture,
                &said,
            );
            assert_eq!(
                decided,
                how == "in code",
                "a subject written {how} was decided {decided}, and this rule is about \
                 what the fixture's code says rather than about what it claims"
            );
        }

        // **And the door half, which the rows above do not decide.** Both rows
        // above name `nc_log::Origin` on line 1 and both carets include line 1,
        // so `names_the_door` is true in each and the conjunct could be dropped
        // whole — measured, every test green, and made vacuous the transcript
        // was byte-identical to a clean run. `caret_uses` has a corpus and
        // mutation operators of its own and every one of them drives it through
        // a door written for tests; this is the only place the rule is asked as
        // the gate asks it.
        //
        // A fixture whose carets land nowhere near its door is refused: the
        // refusal is then about something else, whatever the fixture's own
        // header claims.
        // The subject is named on the caret lines and the door is not: the
        // fixture declares its own `service` and never mentions `nc_log`'s
        // `Origin` where the compiler pointed.
        let elsewhere = concat!(
            "struct Mine { service: &'static str }\n",
            "fn main() {\n",
            "    let service = String::new();\n",
            "    let _ = Mine { service: &service };\n",
            "}\n",
        );
        let said = super::Said {
            sentence: "expected `Literal`, found `&str`".to_owned(),
            at: vec![3, 4],
        };
        assert!(
            !super::the_refusal_is_the_doors(
                "an_origin_cannot_hold_a_runtime_string",
                elsewhere,
                &said,
            ),
            "a refusal whose carets name no door of `nc-log`'s was accepted as one about \
             the wall"
        );
    }

    /// The specificity floor is the number its paragraph argues for.
    ///
    /// **The shortest shipped expectation is 26 characters**,
    /// `` no rules expected `leaked` ``, and `` tuple variant `Id` is private ``
    /// is 29 — and what a wrong number costs is measured: raising the floor to
    /// 28 refuses **two shipped fixtures**.
    ///
    /// `LONGEST_PATH`, `FLUSH` and `GRACE` each carry a test of this shape;
    /// without one the constant is pinned from above by the shipped list and
    /// from below by nothing.
    #[test]
    fn the_specificity_floor_is_the_number_its_paragraph_argues_for() {
        let shortest = super::SHAPES
            .iter()
            .map(|(_, _, expected, _)| expected.chars().count())
            .min()
            .expect("the wall ships fixtures");
        assert_eq!(
            shortest, 26,
            "the shortest sentence this repository expects is {shortest} characters, and \
             the paragraph on `SPECIFIC_ENOUGH` says 26. A floor written above the \
             shortest thing it must accept refuses a fixture that is correct"
        );
        // **The constant itself, read by name.** This is what `LONGEST_PATH`,
        // `FLUSH` and `GRACE` each have: without it the floor is held from
        // above by the shipped list and from below by nothing, and every value
        // from 5 to 26 passes the whole suite, measured.
        assert_eq!(
            super::SPECIFIC_ENOUGH,
            25,
            "the floor moved and its paragraph did not. The paragraph argues for a \
             number below the shortest shipped expectation and above the forgery it \
             was written against; a floor that moves without it is a number two \
             places disagree about"
        );
        // The floor sits below it, and the paragraph's own worked example — the
        // forgery it was written against — sits below the floor.
        //
        // **Read out of the constant, not written a second time.** Comparing
        // `shortest > 25` against a literal, with `shortest` pinned to 26 two
        // assertions above and the forgery's length a compile-time 7, makes
        // both conjuncts constants, so it cannot fail for any state of this
        // tree — least of all the state its own message names. Measured by
        // moving the floor to 27: the suite went red on two other tests and
        // such an assertion passed.
        assert!(
            super::SPECIFIC_ENOUGH < shortest
                && super::SPECIFIC_ENOUGH > "`String`".chars().count(),
            "the floor is {} and no longer sits between the shortest shipped \
             expectation at {shortest} and the eight-character forgery the paragraph \
             is written about",
            super::SPECIFIC_ENOUGH
        );
    }

    /// Every way an expectation can be too weak to be about one diagnostic.
    ///
    /// **Two of these branches no other test decides.** The unclosed backtick
    /// and the length floor answer `` EXPECT: `String `` — seven characters, an
    /// unterminated backtick, and a substring of a large fraction of every
    /// diagnostic rustc emits — which kept the gate saying "for the stated
    /// reason" while the stated reason had become "something about a String",
    /// measured. Undriven, both deleted leave
    /// `ok 11 fixture(s) refuse to compile, for the stated reason` printing
    /// unchanged.
    ///
    /// The shipped expectations are the floor at the other end: each is a real
    /// one from `SHAPES`, and every one of them has to pass, or these cases are
    /// about a predicate that refuses everything.
    #[test]
    fn an_expectation_too_weak_to_be_about_one_diagnostic_is_refused() {
        // A fixture's own shape: it uses the wall and declares nothing of it, so
        // the rules below are the ones left to decide.
        const THE_SHAPE_A_FIXTURE_HAS: &str = "use nc_log::LogValue;\nfn main() {}\n";
        for (declared, why) in [
            ("`String", "leaves a backtick unclosed"),
            ("`Id`", "is 4 characters long"),
            // **The same forgery with its backtick closed**, which the
            // unclosed-backtick branch no longer sees and the length floor is
            // the only thing left to refuse. Measured with the floor lowered
            // to 8, this was accepted as an expectation with the whole suite
            // green; it is driven here so the number is
            // held by what it buys as well as by its pin.
            ("`String`", "is 8 characters long"),
            (
                "no backticks at all here, none",
                "names nothing in backticks",
            ),
        ] {
            let said = super::conjures_its_own_reason(declared, THE_SHAPE_A_FIXTURE_HAS)
                .unwrap_or_else(|| panic!("`{declared}` was accepted as an expectation"));
            assert!(
                said.contains(why),
                "`{declared}` was refused for the wrong reason: {said}"
            );
        }
        // And every expectation this repository actually ships is accepted, so
        // the three above are about weakness and not about the predicate saying
        // no to everything.
        let mut checked = 0_usize;
        for (_, _, declared, _) in super::SHAPES {
            assert!(
                super::conjures_its_own_reason(declared, THE_SHAPE_A_FIXTURE_HAS).is_none(),
                "a shipped expectation is refused: {declared}"
            );
            checked += 1;
        }
        assert!(
            checked >= 8,
            "only {checked} shipped expectation(s) were read"
        );
    }

    /// Twelve ways to hand a caller a `Literal`, and each one is refused.
    ///
    /// Every entry was measured passing against a line-matching rule. Two of
    /// them need no ingenuity at all: the brace on the next line and the
    /// wrapped parameter list are both what `cargo fmt` produces on its own
    /// once a signature is long, and the gate requires `cargo fmt --check` to
    /// pass — so such a rule is one formatter run from blind.
    #[test]
    fn a_public_constructor_is_refused_however_it_is_written() {
        for (way, code) in [
            (
                "a free function",
                "pub fn make(text: &'static str) -> Literal { Literal(text) }\n",
            ),
            (
                "a `From` impl",
                "impl From<&'static str> for Literal {\n    fn from(text: &'static str) -> Self {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "a trait method",
                "impl Mint for u8 {\n    fn mint(&self, text: &'static str) -> Literal {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "a builder on another type",
                "impl Builder {\n    pub fn build(self, text: &'static str) -> Literal {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "a `where` clause",
                "impl Literal {\n    pub fn made<T>(text: T) -> Self\n    where\n        T: Into<&'static str>,\n    {\n        Literal(text.into())\n    }\n}\n",
            ),
            (
                "the brace on the next line",
                "impl Literal {\n    pub fn built(text: &'static str) -> Self\n    {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "an `Option<Self>`",
                "impl Literal {\n    pub fn maybe(text: &'static str) -> Option<Self> {\n        Some(Literal(text))\n    }\n}\n",
            ),
            (
                "a `Result<Self, E>`",
                "impl Literal {\n    pub fn tried(text: &'static str) -> Result<Self, ()> {\n        Ok(Literal(text))\n    }\n}\n",
            ),
            (
                "a signature rustfmt wrapped",
                "impl Literal {\n    pub fn a_name_long_enough_that_rustfmt_wraps_the_parameter_list(\n        text: &'static str,\n    ) -> Self {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "the name split across lines",
                "impl Literal {\n    pub\n    fn split(text: &'static str) -> Self {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "the same thing for `Variant`",
                "impl Variant {\n    pub fn made(text: &'static str) -> Self {\n        Variant(text)\n    }\n}\n",
            ),
            (
                "a public field, which declares no function at all",
                "pub struct Literal(pub &'static str);\n",
            ),
            (
                "a square bracket in the signature",
                "impl Literal {\n    #[must_use]\n    pub fn first_of(texts: &[&'static str]) -> Self {\n        Literal(texts.first().copied().unwrap_or(\"\"))\n    }\n}\n",
            ),
            (
                "a square bracket and no attribute",
                "impl Literal {\n    pub fn first_of(texts: &[&'static str]) -> Self {\n        Literal(texts.first().copied().unwrap_or(\"\"))\n    }\n}\n",
            ),
            (
                "an attribute holding a bracket in a string",
                "impl Literal {\n    #[doc = \"a]b\"]\n    pub fn made(text: &'static str) -> Self {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "an attribute above it",
                "impl Literal {\n    #[must_use]\n    pub fn made(text: &'static str) -> Self {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "the door this crate records having removed, put back",
                "impl Literal {\n    #[doc(hidden)]\n    #[must_use]\n    pub const fn from_macro(text: &'static str) -> Self {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "a public static holding the constructor",
                "pub static MINT: fn(&'static str) -> Literal = Literal::of_text;\n",
            ),
            (
                "a public const holding the constructor",
                "pub const MINT: fn(&'static str) -> Literal = Literal::of_text;\n",
            ),
            (
                "a type alias with an inherent impl behind it",
                "pub type Text = Literal;\nimpl Text {\n    pub fn made(text: &'static str) -> Self {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "a `Deref` whose target is the sealed type",
                "impl core::ops::Deref for Wrapper {\n    type Target = Literal;\n    fn deref(&self) -> &Literal {\n        &self.0\n    }\n}\n",
            ),
        ] {
            let findings = super::no_public_constructor_takes_a_value(
                "crates/nc-log/src/value.rs",
                code,
                true,
            );
            assert_eq!(findings.len(), 1, "{way} was not refused: {findings:#?}");
        }
    }

    /// A trait declaration's default body is a public constructor too.
    ///
    /// **The row for "a trait method" is a trait `impl`.** A method written in
    /// the *declaration* carries no `pub` — its visibility is the trait's — and
    /// its enclosing header is not an `impl … for …`, so a rule asking only for
    /// that walks past it, and any type outside `nc-log` reaches the default
    /// body by writing `impl Named for Mine {}`. Measured: a consumer crate
    /// printed a run-time string out of a `Literal` and out of `Origin.service`
    /// — the field `NOCTUA.md §15.4` puts on every line the log emits — with
    /// both of this step's lines byte-identical and the whole gate green.
    ///
    /// Apart from [`a_public_constructor_is_refused_however_it_is_written`],
    /// because the two together are more than one body may hold, and separate
    /// on its own merits: every row there is an `impl` and the rows here are
    /// declarations.
    #[test]
    fn a_trait_declarations_default_body_is_a_public_constructor() {
        for (way, code) in [
            (
                "a trait declaration's default body",
                "pub trait Named {\n    fn named(&self, text: &'static str) -> Literal {\n        Literal(text)\n    }\n}\n",
            ),
            (
                "an unsafe trait declaration's default body",
                "pub unsafe trait Minted {\n    fn minted(&self, text: &'static str) -> Literal {\n        Literal(text)\n    }\n}\n",
            ),
            // **And one wearing an attribute.** `#[doc(hidden)]` is what a door
            // written to be invisible carries, and a rule reading the raw header
            // misses it.
            (
                "a trait declaration behind an attribute",
                "#[doc(hidden)]\npub trait Hidden {\n    fn hidden(&self, text: &'static str) -> Literal {\n        Literal(text)\n    }\n}\n",
            ),
        ] {
            let findings = super::no_public_constructor_takes_a_value(
                "crates/nc-log/src/value.rs",
                code,
                true,
            );
            assert_eq!(findings.len(), 1, "{way} was not refused: {findings:#?}");
        }
    }

    /// And the crate as it stands is not a finding.
    ///
    /// A type whose name begins with a sealed one is a different type.
    ///
    /// **A raw substring is not a name**, which is why the rule asks `names`,
    /// the word-boundary helper this file
    /// has. Read as a substring, `pub struct LiteralSpan { pub at: usize }` — an
    /// ordinary documented type holding a number, which can hand a caller no
    /// text at all — is reported as the sealed newtype having a public field, in
    /// a message that names a tuple struct and a `Literal::of::<S>()` constructor
    /// neither of which it has. The rule carries no logging hatch of its own, so
    /// the only way past it would be to rename the type.
    ///
    /// Both directions, because narrowing a rule is the other way to break it:
    /// the sealed newtypes themselves must still be refused when their field is
    /// public.
    #[test]
    fn a_type_whose_name_begins_with_a_sealed_one_is_a_different_type() {
        for allowed in [
            "pub struct LiteralSpan {\n    pub at: usize,\n}\n",
            "pub struct VariantRendering {\n    pub quoted: bool,\n}\n",
            "pub struct Literals {\n    pub held: usize,\n}\n",
            // A name that merely *ends* with one is not the question either.
            "pub struct RawLiteral {\n    pub at: usize,\n}\n",
        ] {
            let findings = super::no_public_field(
                "crates/nc-log/src/value.rs",
                allowed,
                &["Literal", "Variant"],
            );
            assert!(
                findings.is_empty(),
                "this is a different type and was refused as the sealed one: \
                 {allowed} {findings:#?}"
            );
        }
        for refused in [
            "pub struct Literal(pub &'static str);\n",
            "pub struct Variant {\n    pub name: &'static str,\n}\n",
        ] {
            assert_eq!(
                super::no_public_field(
                    "crates/nc-log/src/value.rs",
                    refused,
                    &["Literal", "Variant"]
                )
                .len(),
                1,
                "the sealed newtype's own public field is what this rule is for: {refused}"
            );
        }
    }

    /// The floor for `a_public_constructor_is_refused_however_it_is_written`: a
    /// rule that answered "constructor" to every public function would satisfy
    /// every one of its evasions and make `nc-log` unwritable.
    ///
    /// These are the shapes it must leave alone — `get`, `of`, a `Clone` on the
    /// type itself, and a `-> Self` in an impl of something else.
    #[test]
    fn the_crate_as_written_hands_a_caller_nothing() {
        for allowed in [
            "impl Literal {\n    pub const fn of<S: Source>() -> Self {\n        Literal(S::TEXT)\n    }\n}\n",
            "impl Literal {\n    pub const fn get(self) -> &'static str {\n        self.0\n    }\n}\n",
            "impl Clone for Literal {\n    fn clone(&self) -> Self {\n        *self\n    }\n}\n",
            "impl Value {\n    pub fn of(text: &str) -> Self {\n        Value::Text(text)\n    }\n}\n",
            "impl Literal {\n    fn from_macro(text: &'static str) -> Self {\n        Literal(text)\n    }\n}\n",
            "pub struct Literal(&'static str);\n",
            "pub fn describe(one: &Literal) -> &'static str {\n    one.get()\n}\n",
        ] {
            let findings = super::no_public_constructor_takes_a_value(
                "crates/nc-log/src/value.rs",
                allowed,
                true,
            );
            assert!(
                findings.is_empty(),
                "this hands a caller nothing and was refused: {allowed} {findings:#?}"
            );
        }
        // And the real file, which is the only one that has to keep passing.
        let real = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/nc-log/src/value.rs"),
        )
        .expect("the crate this rule is about is there");
        let left_over =
            super::no_public_constructor_takes_a_value("crates/nc-log/src/value.rs", &real, true);
        assert!(
            left_over.is_empty(),
            "the crate as committed does not pass its own rule: {left_over:?}"
        );
        // The floor for *that*: a file this rule never looked at would pass it.
        assert!(
            real.contains("pub struct Literal"),
            "the file read is not the one this rule is about"
        );
    }

    /// A wrapped signature is read as one signature, up to the written number.
    ///
    /// **`SIGNATURE_LINES` decides, and this holds it.** Without a test of
    /// `signatures_joined` or `promotes_to_static`, the constant that joins a
    /// rustfmt-wrapped signature can be set to `0` with the whole gate green
    /// and the leak the rule exists for walking straight through, measured:
    ///
    /// ```text
    /// static REMEMBERED: OnceLock<String> = OnceLock::new();
    /// pub fn remembered_for_a_very_long_parameter_name_indeed(
    ///     text_supplied_by_a_caller_at_run_time: String,
    /// ) -> &'static str { … }
    ///
    ///   = 24  ->  error crates/nc-canon/src/lib.rs:549 — this function hands
    ///             back a `&'static str` it was not given
    ///   =  0  ->  ok    88 source file(s) manufacture no static string
    /// ```
    ///
    /// The number is restated here, so moving it means moving it twice — the
    /// treatment `PLAUSIBLE_DRIFT_MS` and `MAX_DEPTH` already have.
    #[test]
    fn the_number_of_lines_a_signature_may_wrap_over_is_the_written_one() {
        use std::fmt::Write as _;

        assert_eq!(
            super::SIGNATURE_LINES,
            24,
            "the number of lines a wrapped signature may run to moved. rustfmt writes \
             one parameter per line, so this is a bound on parameters; a signature \
             longer than it is read as its first line alone, and a `-> &'static str` \
             below that line is not seen"
        );

        // A signature on one line is itself.
        let one = super::signatures_joined("pub fn f(text: String) -> &'static str {\n");
        assert_eq!(one.len(), 1);
        assert!(super::promotes_to_static(&one[0], &[]), "{one:?}");

        // Wrapped the way rustfmt wraps it, which is the ordinary case for this
        // rule: a leak takes an owned value and returns a borrow of a static.
        let wrapped = super::signatures_joined(
            "pub fn remembered(\n    text: String,\n) -> &'static str {\n    REMEMBERED\n}\n",
        );
        assert!(
            super::promotes_to_static(&wrapped[0], &[]),
            "a wrapped signature was not joined: {wrapped:?}"
        );

        // And one wrapped over more lines than the bound is not joined, which is
        // what the bound is: the rule stops rather than reading the rest of the
        // file as one signature.
        let mut parameters = String::new();
        for at in 0..super::SIGNATURE_LINES + 2 {
            let _ = writeln!(parameters, "    p{at}: String,");
        }
        let past =
            super::signatures_joined(&format!("pub fn f(\n{parameters}) -> &'static str {{\n"));
        assert!(
            !super::promotes_to_static(&past[0], &[]),
            "a signature wrapped over more than SIGNATURE_LINES lines was joined whole, \
             so the bound is not a bound: {:?}",
            past[0]
        );

        // The floor in the other direction: this predicate is about the shape of
        // a signature, and a signature that does not hand back a `&'static str`
        // is not one — wrapped or not. Whether the borrow was *supplied* is the
        // caller's question, not this one's.
        for honest in [
            "pub fn f(text: String) -> String {\n",
            "pub fn f(\n    text: String,\n) -> String {\n",
            "let text: &'static str = other();\n",
            "pub const fn f() -> u32 {\n",
        ] {
            let joined = super::signatures_joined(honest);
            assert!(
                !super::promotes_to_static(&joined[0], &[]),
                "`{honest}` hands back no `&'static str` at all: {joined:?}"
            );
        }
        // And the qualifiers a signature may carry do not hide it.
        for dressed in [
            "pub(crate) async fn f(text: String) -> &'static str {\n",
            "pub async fn f(\n    text: String,\n) -> &'static str {\n",
            "unsafe fn f(text: String) -> Option<&'static str> {\n",
            "pub(in crate::wall) fn f(text: String) -> impl Iterator<Item = &'static str> {\n",
            // **The binding half, behind every visibility too.** Every row
            // above is a `fn`, and a branch that reads a `static` or `const`
            // holding one by four literal prefixes passes the control
            // immediately below and walks the two under it past a rule this
            // file's own prose says strips exactly those visibilities —
            // measured.
            "pub static EIGHT: fn(String) -> &'static str = |text| other(text);\n",
            "pub(crate) static EIGHT: fn(String) -> &'static str = |text| other(text);\n",
            "pub(super) const EIGHT: fn(String) -> &'static str = |text| other(text);\n",
            "pub(in crate::wall) static EIGHT: fn(String) -> &'static str = |text| other(text);\n",
        ] {
            let joined = super::signatures_joined(dressed);
            assert!(
                super::promotes_to_static(&joined[0], &[]),
                "`{dressed}` hands back a `&'static str` and was not seen: {joined:?}"
            );
        }
    }

    /// A name this file gives a `&'static str` is a `&'static str`.
    ///
    /// **A `pub type` would overturn criterion 2.** Deciding on
    /// `returns.contains("&'static str")` — the return type as written — lets a
    /// file that names the type walk past the shape rule.
    /// Measured, probe and control in one file and one run in
    /// `crates/nc-prune/src/lib.rs`:
    ///
    /// ```text
    /// pub type Forever = &'static str;
    /// static KEPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    /// pub fn kept_aliased(text: String) -> Forever  { … }   ->  reported by nothing
    /// pub fn kept_plain(text: String) -> &'static str { … } ->  reported
    /// ```
    ///
    /// With the control taken out the gate printed `88 source file(s)
    /// manufacture no static string` over a function that manufactures one from
    /// its caller's `String`, and the `LEAKS` needles cannot help —
    /// `OnceLock::get_or_init(…).as_str()` names none of them, which is the
    /// shape `signatures_joined`'s own doc records as needle-proof. Its
    /// argument is this one: "no list of paths holds a name the caller
    /// invented", and a return type is a spelling too.
    ///
    /// A case per alias spelling, because what a rule reads is a spelling, and a
    /// spelling without a case decides nothing — as `SIGNATURE_LINES` shows. Each case
    /// also asserts the same signature is *not* seen with the aliases withheld,
    /// so a case that stopped depending on the resolution says so.
    #[test]
    fn an_alias_for_a_static_string_is_read_as_the_type_it_names() {
        for (declared, signature) in [
            (
                "pub type Forever = &'static str;",
                "pub fn kept(text: String) -> Forever {",
            ),
            (
                "type Forever = &'static str;",
                "pub fn kept(text: String) -> Forever {",
            ),
            (
                "pub(crate) type Forever = &'static str;",
                "fn kept(text: String) -> Forever {",
            ),
            (
                "pub(in crate::wall) type Forever = &'static str;",
                "async fn kept(text: String) -> Forever {",
            ),
            // Wrapped by rustfmt, which is what a long declaration looks like.
            (
                "pub type Forever =\n    &'static str;",
                "pub fn kept(text: String) -> Forever {",
            ),
            // Holding one rather than being one, which is the width the
            // written-out rule already has: `-> Vec<&'static str>` is judged.
            (
                "pub type Names = Vec<&'static str>;",
                "pub fn kept(text: String) -> Names {",
            ),
            (
                "pub type Edit = (&'static str, fn(&mut u8));",
                "pub fn kept(text: String) -> Vec<Edit> {",
            ),
            // The alias inside another type, as the written-out spelling is
            // already read anywhere in the return type.
            (
                "pub type Forever = &'static str;",
                "pub fn kept(text: String) -> Option<Forever> {",
            ),
            (
                "pub type Forever = &'static str;",
                "pub fn kept(t: String) -> impl Iterator<Item = Forever> {",
            ),
            // And both halves wrapped, which is the pair that hid this: the
            // alias hides the needle and the wrap hides the signature.
            (
                "pub type Forever = &'static str;",
                "pub fn kept(\n    text: String,\n) -> Forever {",
            ),
            // A binding whose type is a function returning one, which the
            // `static`/`const` arm above judges by the same return type.
            (
                "pub type Forever = &'static str;",
                "pub static EIGHT: fn(String) -> Forever = |text| kept(text);",
            ),
        ] {
            let text = format!("{declared}\n{signature}\n");
            let aliases = super::static_aliases(&text);
            assert!(
                !aliases.is_empty(),
                "`{declared}` declares a name for a `&'static str` and this rule \
                 collected none: {aliases:?}"
            );
            let joined = super::signatures_joined(&text);
            let at = text
                .lines()
                .position(|line| line.contains("kept"))
                .expect("the signature is in the text");
            assert!(
                super::promotes_to_static(&joined[at], &aliases),
                "`{signature}` hands back `{declared}` and was not seen: {:?}",
                joined[at]
            );
            // The floor under the case: it is the alias that decides, and not
            // something the signature says on its own.
            assert!(
                !super::promotes_to_static(&joined[at], &[]),
                "`{signature}` was read as handing back a `&'static str` with no alias \
                 resolved at all, so this case proves nothing about `{declared}`"
            );
        }
    }

    /// A name is not the type, and a type that is not one is not named into one.
    ///
    /// The floor under [`an_alias_for_a_static_string_is_read_as_the_type_it_names`]:
    /// a resolution that answered "yes" to every name would satisfy every case
    /// there and report every function in the tree.
    #[test]
    fn a_name_that_is_not_an_alias_for_a_static_string_is_not_read_as_one() {
        for (declared, signature) in [
            (
                "pub type Owned = String;",
                "pub fn kept(text: String) -> Owned {",
            ),
            // A longer name that merely opens with the alias.
            (
                "pub type Forever = &'static str;",
                "pub fn kept(text: String) -> Forevermore {",
            ),
            // The alias taken rather than handed back.
            (
                "pub type Forever = &'static str;",
                "pub fn kept(text: Forever) -> String {",
            ),
            // Written in a comment, which declares nothing.
            (
                "// pub type Forever = &'static str;",
                "pub fn kept(text: String) -> Forever {",
            ),
        ] {
            let text = format!("{declared}\n{signature}\n");
            let aliases = super::static_aliases(&text);
            let joined = super::signatures_joined(&text);
            let at = text
                .lines()
                .position(|line| line.contains("fn kept"))
                .expect("the signature is in the text");
            assert!(
                !super::promotes_to_static(&joined[at], &aliases),
                "`{signature}` under `{declared}` hands back no `&'static str` and was \
                 reported: {aliases:?}"
            );
        }
    }

    /// The gate reports the aliased door beside the written-out one.
    ///
    /// The two above are about the predicate; this is the gate, over the probe
    /// that would overturn the criterion — control beside it in one file and one
    /// run, which is what says the file was judged at all rather than skipped.
    #[test]
    fn the_probe_that_overturned_the_criterion_is_reported() {
        let probe = concat!(
            "pub type Forever = &'static str;\n",
            "static KEPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();\n",
            "pub fn kept_aliased(text: String) -> Forever { KEPT.get_or_init(|| text).as_str() }\n",
            "pub fn kept_plain(text: String) -> &'static str { KEPT.get_or_init(|| text).as_str() }\n",
        );
        let (findings, read, _) =
            super::nothing_manufactures_a_static_string("crates/nc-prune/src/lib.rs", probe, &[]);
        assert_eq!(read, 4, "the gate did not read the whole probe");
        let at: Vec<usize> = findings.iter().map(|finding| finding.line).collect();
        assert_eq!(
            at,
            vec![3, 4],
            "the aliased door and the written-out one are the same door, and the gate \
             reported {at:?}: {findings:#?}"
        );
    }

    /// A turbofish the walk loses its place in hides the call after it.
    ///
    /// **`without_type_arguments` is the first thing the leak scan runs**, so a
    /// walk that runs off the end of the line makes all six needles blind for
    /// it. Counting `<` and `>` without the shift guard and the `->` guard the
    /// other walks in this file carry, a shift or an arrow inside the type
    /// arguments swallows the rest of the line. Each row below is the control
    /// one character apart.
    ///
    /// **A character walk can tell a right shift from two closing angles.** A
    /// guard symmetric in `>>` would trade the last row — an ordinary nested
    /// generic inside an array length — for the right shift, measured; brackets
    /// nest, and the second set of rows below holds that both are read.
    #[test]
    fn a_shift_or_an_arrow_in_a_turbofish_does_not_hide_the_leak() {
        let judge = |probe: &str| {
            let (found, read, _) = super::nothing_manufactures_a_static_string(
                "crates/nc-prune/src/lib.rs",
                probe,
                &[],
            );
            assert!(read > 0, "the gate read nothing of `{probe}`");
            found
        };
        for (what, probe) in [
            (
                "the control, a plain array length",
                "Vec::<[u8; 8]>::leak(v)",
            ),
            (
                "a left shift in the array length",
                "Vec::<[u8; 1 << 3]>::leak(v)",
            ),
            (
                "a const argument holding a shift",
                "Vec::<{ 1 << 3 }>::leak(v)",
            ),
            ("an arrow in a function type", "Box::<fn() -> u8>::leak(v)"),
            ("a qualified path", "Box::<<u8 as A>::B>::leak(v)"),
            (
                "a nested generic in an array",
                "Box::<[Box<Box<str>>; 2]>::leak(v)",
            ),
        ] {
            let found = judge(&format!("fn a(v: Vec<u8>) {{ let _x = {probe}; }}\n"));
            assert!(
                !found.is_empty(),
                "{what}: the turbofish walk lost its place and the leak after it was \
                 invisible — `{probe}`"
            );
        }

        // **The claimed limit, closed.** A right shift can be told from two
        // closing angles without trading the nested generic above for it:
        // brackets nest, and remembering the angle depth at each one's opening
        // closes all four of these at no cost. Pinned as a hole, this assertion
        // would be a lock on the repair rather than a floor under the rule,
        // with a real `Vec::leak` unreported.
        for (what, probe) in [
            ("a right shift", "Vec::<[u8; 8 >> 1]>::leak(v)"),
            (
                "a comparison in a const expression",
                "Vec::<[u8; (1 >= 0) as usize]>::leak(v)",
            ),
            ("a comparison in a const block", "Vec::<{ 1 > 0 }>::leak(v)"),
            (
                "a shift of an associated const",
                "Vec::<[u8; A::<u8>::N >> 1]>::leak(v)",
            ),
            // The mirror of the comparison row above, one character apart,
            // which a `<` arm forgiving a `<` only beside another `<` answers
            // the opposite way, while the `>` arm forgives any `>` a bracket
            // holds. Three spellings, measured.
            (
                "a less-than in a const expression",
                "Vec::<[u8; (1 <= 0) as usize]>::leak(v)",
            ),
            ("a less-than in a const block", "Vec::<{ 1 < 2 }>::leak(v)"),
            (
                "a comparison in an if-expression length",
                "Vec::<[u8; if 1 < 2 { 3 } else { 4 }]>::leak(v)",
            ),
        ] {
            let found = judge(&format!("fn a(v: Vec<u8>) {{ let _x = {probe}; }}\n"));
            assert!(
                !found.is_empty(),
                "{what}: the turbofish walk lost its place and the leak after it was \
                 invisible — `{probe}`"
            );
        }
    }

    /// The alias rule reaches one level and one crate, and says where it stops.
    ///
    /// **The limit, tested.** `hands_back_a_static` resolves an alias one level
    /// deep, and with the crate-wide union one declared anywhere in the member
    /// being walked. Unpinned, neither boundary moves a number: a two-line
    /// `type A = &'static str; type B = A;` walks under
    /// `88 source file(s) manufacture no static string`, and so does an alias
    /// imported from another crate — and a future widening *or* narrowing of
    /// either would move no number in this repository.
    ///
    /// So both are cases here. The two in scope are reported and the two out of
    /// scope are not, which makes each of the four a decision a reader can see
    /// rather than a property nobody measured. If a later change resolves an
    /// alias of an alias, this test fails and the paragraph on
    /// [`super::hands_back_a_static`] is corrected in the same edit.
    #[test]
    fn an_alias_two_levels_deep_or_a_crate_away_is_a_stated_hole() {
        let held = "static KEPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();\n";
        let body = "{ KEPT.get_or_init(|| text).as_str() }";
        let judge = |declared: &str, signature: &str, from_the_crate: &[String]| {
            let text = format!("{declared}{held}{signature} {body}\n");
            let (findings, read, _) = super::nothing_manufactures_a_static_string(
                "crates/nc-prune/src/probe.rs",
                &text,
                from_the_crate,
            );
            assert!(read > 0, "the gate read nothing of `{signature}`");
            !findings.is_empty()
        };

        // In scope: the first is the probe that would overturn criterion 2;
        // the second is the one that shows the limit is a crate rather than a
        // file.
        assert!(
            judge(
                "pub type Forever = &'static str;\n",
                "pub fn kept(text: String) -> Forever",
                &[],
            ),
            "an alias declared in the file being read is not resolved"
        );
        assert!(
            judge(
                "",
                "pub fn kept(text: String) -> crate::Forever",
                &["Forever".to_owned()],
            ),
            "an alias declared in a sibling module of the same crate is not resolved"
        );

        // **Out of scope, on purpose.** An alias of an alias needs the
        // right-hand side of one declaration resolved against another, which is
        // a resolver and not a scan; and an alias imported from a *different*
        // crate needs the `use` graph, which is cargo's answer and not this
        // file's. Both are named here so that the day either is worth having,
        // the change is a two-place edit rather than a silent widening.
        assert!(
            !judge(
                "type Once = &'static str;\ntype Twice = Once;\n",
                "pub fn kept(text: String) -> Twice",
                &[],
            ),
            "an alias of an alias is now resolved, which is a widening this rule's own \
             paragraph says it does not do — correct the paragraph on \
             `hands_back_a_static` in the same edit"
        );
        assert!(
            !judge(
                "pub use nc_canon::Forever;\n",
                "pub fn kept(text: String) -> Forever",
                &[],
            ),
            "an alias imported from another crate is now resolved, which needs the \
             `use` graph — correct the paragraph on `hands_back_a_static` in the same \
             edit"
        );

        // The control that says the four above are about the *alias* and not
        // about the shape: written out, the same function is reported.
        assert!(
            judge("", "pub fn kept(text: String) -> &'static str", &[]),
            "the written-out signature is not reported, so nothing above compares \
             anything"
        );
    }

    /// An attribute in front of a door does not hide the door.
    ///
    /// **One `#[doc(hidden)]` can switch this rule off.** `is_public` strips the
    /// run of attributes in front of a declaration — it exists for that, and its
    /// own doc carries the measurement — and testing the *raw* text with
    /// `starts_with("pub type ")` in `no_other_public_door`, three lines below
    /// the call to it, makes the item public to one half and not a door to the
    /// other, measured:
    ///
    /// ```text
    /// #[doc(hidden)]
    /// pub type Text = Literal;
    /// impl Text { pub fn made(text: &'static str) -> Self { Literal(text) } }
    ///
    ///   ok    0 crate(s) outside nc-log may log …
    ///   check passed
    /// ```
    ///
    /// with the attribute deleted and nothing else changed:
    ///
    /// ```text
    ///   error crates/nc-log/src/value.rs:471 — this hands a caller a `Literal`
    ///         without declaring a function
    /// ```
    ///
    /// The alias is what makes it worth closing: the constructor rule resolves
    /// an `impl` header by name, and `Text` is not a sealed name, so nothing
    /// else in the wall sees the `impl` underneath.
    #[test]
    fn an_attribute_in_front_of_a_door_does_not_hide_it() {
        for door in [
            "pub type Text = Literal;\n",
            "pub static ONE: Literal = Literal(\"x\");\n",
            "pub const ONE: Literal = Literal(\"x\");\n",
        ] {
            let bare =
                super::no_other_public_door("crates/nc-log/src/value.rs", door, &["Literal"]);
            assert_eq!(
                bare.len(),
                1,
                "`{door}` is a door and was not reported: {bare:#?}"
            );
            for attribute in [
                "#[doc(hidden)]\n",
                "#[allow(dead_code)]\n",
                "#[doc(hidden)]\n#[allow(dead_code)]\n",
                "#[doc = \"a]b\"]\n",
            ] {
                let dressed = format!("{attribute}{door}");
                let found = super::no_other_public_door(
                    "crates/nc-log/src/value.rs",
                    &dressed,
                    &["Literal"],
                );
                assert_eq!(
                    found.len(),
                    1,
                    "`{attribute}` in front of `{door}` hid it: {found:#?}"
                );
            }
        }
        // The floor in the other direction: a private alias is not a door, with
        // or without an attribute in front of it.
        for shut in [
            "pub(crate) type Text = Literal;\n",
            "#[doc(hidden)]\npub(crate) type Text = Literal;\n",
            "pub type Text = String;\n",
        ] {
            let found =
                super::no_other_public_door("crates/nc-log/src/value.rs", shut, &["Literal"]);
            assert!(found.is_empty(), "`{shut}` is not a door: {found:#?}");
        }
    }

    /// A fixture cannot hand the gate the reason it is judged against.
    ///
    /// **The channel is `rendered`.** rustc echoes the offending source line
    /// into its output, so a fixture that fails for an unrelated reason and
    /// writes its expected reason into the failing line is counted as proof by
    /// a gate reading it:
    ///
    /// ```text
    /// //! EXPECT: the trait bound `String: LogValue` is not satisfied
    /// fn main() {
    ///     undefined_helper("the trait bound `String: LogValue` is not satisfied");
    /// }
    ///
    /// error[E0425]: cannot find function `undefined_helper` in this scope
    ///   -> counted as proof that a String cannot be a log value
    /// ```
    ///
    /// Changing which string is looked for leaves the fixture able to supply
    /// it. What is read is the diagnostic minus its
    /// `rendered` block and minus each span's `text` — rustc's own words, which
    /// a fixture cannot write for it.
    #[test]
    fn a_diagnostic_is_read_without_the_source_it_echoes() {
        // One line of `cargo build --message-format=json`, trimmed to the
        // fields that matter, with the fixture's own sentence in `rendered` and
        // in the span's `text` — and nothing rustc wrote saying it.
        let echoed = r#"{"reason":"compiler-message","message":{"rendered":"error[E0425]: cannot find function\n  |\n5 |     undefined_helper(\"the trait bound `String: LogValue` is not satisfied\");\n","children":[{"level":"help","message":"a function with a similar name exists"}],"code":{"code":"E0425"},"level":"error","message":"cannot find function `undefined_helper` in this scope","spans":[{"text":[{"text":"    undefined_helper(\"the trait bound `String: LogValue` is not satisfied\");"}],"label":null}]}}"#;
        let said = super::diagnostics(echoed, "");
        assert_eq!(said.len(), 1, "the error was not read at all: {said:?}");
        assert!(
            !said[0].sentence.contains("String: LogValue"),
            "the fixture's own sentence reached the reason it is judged against: {said:?}"
        );
        assert!(
            said[0].sentence.contains("cannot find function"),
            "rustc's own words are gone too: {said:?}"
        );
        assert!(
            said[0].sentence.contains("E0425"),
            "the code is not carried: {said:?}"
        );

        // **And a child's level is not the diagnostic's.** `children` is written
        // before the outer `level`, so a `find` for the first `"level":"`
        // answers `help` and every real error is skipped — measured as the
        // whole gate reporting every fixture as failing for a reason it does
        // not declare.
        let noted = r#"{"message":{"children":[{"level":"note","message":"required by a bound"}],"level":"error","message":"the trait bound `String: LogValue` is not satisfied","spans":[]}}"#;
        let said = super::diagnostics(noted, "");
        assert_eq!(
            said.len(),
            1,
            "a diagnostic with children was skipped: {said:?}"
        );
        assert!(said[0].sentence.contains("String: LogValue"), "{said:?}");
        // The child's note is rustc's own words and is kept: `cannot be
        // constructed` and `expected `Literal`, found `&str`` are a note and a
        // span label, and two shipped fixtures are judged by them.
        assert!(said[0].sentence.contains("required by a bound"), "{said:?}");

        // **A sentence the fixture itself wrote is not a reason.**
        // `compile_error!` puts an author-chosen string into the diagnostic's
        // own `message` field, at level `error` — so a fixture that fails on a
        // string it chose can declare that string as its expectation and be
        // counted as proof. Measured: six lines, naming neither `nc_log` nor `LogValue`
        // nor any log call, with `11 fixture(s) refuse to compile, for the

        // stated reason` unchanged.
        let authored = r#"{"message":{"children":[],"code":null,"level":"error","message":"the trait bound `String: LogValue` is not satisfied","spans":[]}}"#;
        let left_over = super::diagnostics(
            authored,
            "compile_error!(\"the trait bound `String: LogValue` is not satisfied\");",
        );
        assert!(
            left_over.is_empty(),
            "a sentence out of the fixture's own code was read as rustc's: {left_over:?}"
        );
        // And the same diagnostic from a fixture that did not write it is kept,
        // or the rule would be "no diagnostic counts".
        assert_eq!(
            super::diagnostics(authored, "fn main() { let x: String = one(); }").len(),
            1
        );
        // **A macro's refusal carries no code and is still rustc's.**
        // `literal!` answers a non-literal with `no rules expected \`leaked\``,
        // which is the wall working — requiring an error code would refuse
        // it.
        let macro_refusal = r#"{"message":{"children":[],"code":null,"level":"error","message":"no rules expected `leaked`","spans":[]}}"#;
        assert_eq!(
            super::diagnostics(macro_refusal, "log.record(&E { name: literal!(leaked) });").len(),
            1,
            "a macro's own refusal was read as the fixture's words"
        );

        // A warning is not why a fixture failed to compile.
        let warned = r#"{"message":{"children":[],"level":"warning","message":"unused variable","spans":[]}}"#;
        let left_over = super::diagnostics(warned, "");
        assert!(left_over.is_empty(), "{left_over:?}");
    }

    use super::{FIXTURES, SHAPES, compile_fail};

    /// The gate over one file, naming no alias declared elsewhere in its crate.
    ///
    /// The union over a member's modules is the *caller's* half of the rule —
    /// `check.rs` collects it in a pass of its own — and every case below is
    /// about one file's text. Written here so those cases say so, rather than
    /// each repeating an empty slice.
    fn no_formatting_at_a_logging_call(
        file: &str,
        text: &str,
        compiled_into_the_log: bool,
    ) -> (Vec<super::Finding>, usize) {
        super::no_formatting_at_a_logging_call(file, text, compiled_into_the_log, &[])
    }
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// A fixture may not define what it claims the wall refuses.
    ///
    /// **Every fixture is forgeable past the authorship filter alone.**
    /// Measured: every fixture replaced with a file failing for an unrelated
    /// reason, each `EXPECT` kept unchanged, gave
    /// `ok 11 fixture(s) refuse to compile, for the stated reason` with
    /// `check passed`. The authorship filter cannot see it: it asks whether the
    /// fixture's *source* contains the diagnostic's sentence, and such fixtures
    /// never write the sentence — they make rustc produce it.
    ///
    /// Three shapes, one per rule, each measured against the real gate.
    #[test]
    fn a_fixture_cannot_conjure_the_sentence_it_declares() {
        for (why, declared, code) in [
            (
                "a local trait of the same name",
                "the trait bound `String: LogValue` is not satisfied",
                "use nc_log as _;\ntrait LogValue {}\nfn accepts<T: LogValue>(_: T) {}\n",
            ),
            (
                "a local struct of the same name",
                "struct `Shipper` is private",
                "use nc_log as _;\nmod inner {\n    pub struct Shipper;\n}\n",
            ),
            (
                "a local module standing in for the crate",
                "the trait bound `Mine: nc_log::sealed::Sealed` is not satisfied",
                "use nc_log as _;\nmod nc_log {\n    pub mod sealed {}\n}\nstruct Mine;\n",
            ),
            (
                "an attribute that writes the sentence",
                "cannot be constructed",
                "use nc_log as _;\n#[diagnostic::on_unimplemented(label = \"cannot be constructed\")]\ntrait T {}\n",
            ),
            (
                "a macro of its own",
                "no rules expected `leaked`",
                "use nc_log as _;\nmacro_rules! only_a_literal {\n    ($t:literal) => {};\n}\n",
            ),
            (
                "a sentence with no identifier, from a file about nothing",
                "attempt to use a non-constant value in a constant",
                "use nc_log as _;\nfn main() {\n    let n = 1_usize;\n    const M: usize = n;\n}\n",
            ),
            (
                "a visibility the prefix strip does not know",
                "the trait bound `String: LogValue` is not satisfied",
                "use nc_log as _;\npub(crate) trait LogValue {}\nfn accepts<T: LogValue>(_: T) {}\n",
            ),
            (
                "a declaration inside a one-line module",
                "the trait bound `String: LogValue` is not satisfied",
                "use nc_log as _;\nmod inner { pub trait LogValue {} }\n",
            ),
            (
                "naming the crate only in a comment",
                "the trait bound `String: LogValue` is not satisfied",
                "// nc_log is named nowhere but here.\ntrait Whatever {}\n",
            ),
            // **And a local *subject* of the same name**, which is the half the
            // three cases above do not reach: all of them shadow the *bound*.
            // The four `LogValue` fixtures name `String`, `&str` and `&String`
            // as their subject, and a file that writes `struct String;` has not
            // built a subject — it has shadowed one, so rustc's sentence is
            // about a different type from the one the fixture's own header says
            // it is about.
            (
                "a local struct standing in for a prelude name",
                "the trait bound `String: LogValue` is not satisfied",
                "use nc_log as _;\nstruct String;\nfn accepts<T: nc_log::LogValue>(_: T) {}\n",
            ),
            (
                "a local struct standing in for a primitive",
                "the trait bound `&str: LogValue` is not satisfied",
                "use nc_log as _;\nstruct str;\nfn accepts<T: nc_log::LogValue>(_: T) {}\n",
            ),
        ] {
            assert!(
                super::conjures_its_own_reason(declared, code).is_some(),
                "{why} was accepted"
            );
        }

        // And the shapes a real fixture takes are not refused, or the six above
        // would hold against a rule that rejects everything. The subject of a
        // trait bound is the fixture's own to declare — that is what a fixture
        // *is*.
        for (why, declared, code) in [
            (
                "declaring the subject, not the bound",
                "the trait bound `Mine: nc_log::sealed::Sealed` is not satisfied",
                "struct Mine;\nfn main() {\n    nc_log::Log::record(&log, &Mine);\n}\n",
            ),
            (
                "naming a type it does not declare",
                "the trait bound `String: LogValue` is not satisfied",
                "fn main() {\n    let _ = nc_log::literal!(String::new());\n}\n",
            ),
            // The other side of the subject rule, so it is not one that refuses
            // everything: a fixture's own name as its own subject is what a
            // fixture *is*, and `Mine` is not a name the language owns.
            (
                "declaring a subject the language does not own",
                "the trait bound `Mine: nc_log::LogValue` is not satisfied",
                "struct Mine;\nfn accepts<T: nc_log::LogValue>(_: T) {}\n",
            ),
            (
                // **The other direction, which is a live trap.** Looked for in
                // the whole file, the three banned spellings in a fixture's own
                // doc line — "the refusal below is rustc's own, not a
                // `compile_error!` we wrote" — refuse a file that is
                // byte-identical in behaviour, with a message that is actively
                // false. This repository writes exactly that kind of header.
                "naming a banned spelling in its own prose",
                "the trait bound `String: LogValue` is not satisfied",
                "//! The refusal is rustc's own, not a `compile_error!` we wrote.\n\
                 //! It is not a `macro_rules!` either, nor `#[diagnostic::…]`.\n\
                 fn main() {\n    let _ = nc_log::literal!(String::new());\n}\n",
            ),
        ] {
            assert_eq!(
                super::conjures_its_own_reason(declared, code),
                None,
                "{why} was refused"
            );
        }
    }
    /// Every way the control can fail to decide is a `None`, not a pass.
    ///
    /// **The exit-status guard, driven.** Measured: breaking the hollow manifest
    /// *and* neutering the guard left
    /// `the_wall_holds_and_the_proof_actually_ran` green with
    /// `controlled == checked == 11`, so `check` printed "each was rebuilt
    /// against a crate of the same name with the guarantee removed" over eleven
    /// rebuilds that never happened — and removing the guard alone was invisible
    /// to every other test in this module.
    ///
    /// The distinction that matters is the one no subprocess is needed to make:
    /// an empty diagnostic list means *the fixture compiled* when cargo
    /// succeeded and *nothing was built at all* when it did not, and without the
    /// guard those two are the same value.
    #[test]
    fn a_control_that_did_not_run_is_not_a_control_that_passed() {
        let sentence = |text: &str| super::Said {
            sentence: text.to_owned(),
            at: Vec::new(),
        };
        // cargo succeeded and rustc said nothing: it compiled against the stub,
        // so the wall is what refused the real build.
        assert_eq!(
            super::control_verdict(true, &[], "anything"),
            super::Control::Decided(false)
        );
        // cargo failed and rustc said nothing: the error went to stderr and the
        // JSON stream is empty. **Without the exit status, this is
        // `Decided(false)` too.**
        assert_eq!(
            super::control_verdict(false, &[], "anything"),
            super::Control::NoBuild
        );
        // The expected sentence came back without the crate, so it proves nothing.
        assert_eq!(
            super::control_verdict(
                false,
                &[sentence("E0277 the trait bound")],
                "the trait bound"
            ),
            super::Control::Decided(true)
        );
        // Names that failed to resolve: rustc stopped before the guarantee.
        assert_eq!(
            super::control_verdict(false, &[sentence("E0433 cannot find")], "the trait bound"),
            super::Control::OnlyResolution
        );
        // A door declared by name and not by shape.
        assert_eq!(
            super::control_verdict(false, &[sentence("E0599 no associated function")], "bound"),
            super::Control::SomeOtherReason
        );
        // **And what the caller does with each.** As a hand-written match
        // nothing reads, `Control::Decided(_) => Ok(Some(false))` is one token
        // away and, measured, left every other test in this module green with
        // `controlled == checked == 11`, and a fixture forged past every other
        // rule here shipped through exactly that.
        assert_eq!(super::answer(&super::Control::Decided(true)), Some(true));
        assert_eq!(super::answer(&super::Control::Decided(false)), Some(false));
        for did_not_run in [
            super::Control::NoBuild,
            super::Control::OnlyResolution,
            super::Control::SomeOtherReason,
        ] {
            assert_eq!(
                super::answer(&did_not_run),
                None,
                "{did_not_run:?} is a control that did not run, and it answered"
            );
        }
        // **And the two hand-written matches beside it**, where the same
        // forgery goes once `answer` is closed. `outcome` turns an answer into a
        // verdict; `controlled_by` counts the verdicts that came from a control
        // that ran. Merging `FailedCorrectly` with `ControlDidNotRun` — the
        // natural way to write two arms that do nothing — makes eleven rebuilds
        // that never happened count as eleven that did, measured.
        assert_eq!(
            super::outcome(Some(true)),
            super::Outcome::WouldFailWithoutTheCrate
        );
        assert_eq!(super::outcome(Some(false)), super::Outcome::FailedCorrectly);
        assert_eq!(super::outcome(None), super::Outcome::ControlDidNotRun);
        assert_eq!(
            super::controlled_by(&[
                super::Outcome::FailedCorrectly,
                super::Outcome::ControlDidNotRun,
                super::Outcome::FailedCorrectly,
            ]),
            2,
            "a control that did not run counted as one that did"
        );
        assert_eq!(
            super::controlled_by(&vec![super::Outcome::ControlDidNotRun; 11]),
            0,
            "eleven controls that never ran counted as controls"
        );
        // And one resolution failure among others is not "only resolution".
        assert_eq!(
            super::control_verdict(
                false,
                &[sentence("E0433 cannot find"), sentence("E0599 no function")],
                "bound"
            ),
            super::Control::SomeOtherReason
        );
    }

    /// A control that could not run answers `None`, through the function itself.
    ///
    /// **The one call frame `a_control_that_did_not_run_is_not_a_control_that_
    /// passed` cannot reach.** `answer`, `outcome`, `controlled_by` and
    /// `control_verdict` are each pinned by that test; what
    /// [`super::fails_the_same_way_without_the_crate`] *does* with an answer of
    /// `None` is pinned here. With the function ending in a bare `Ok(None)` under
    /// the warn, one token — `Ok(Some(false))` — leaves every other test passing,
    /// measured, and with the hollow manifest's path dependency misspelt beside
    /// it `check` prints eleven `skipped: the hollow build did not compile…`
    /// lines and then `ok 11 fixture(s) refuse to compile … and each was rebuilt
    /// against a crate of the same name with the guarantee removed`, over eleven
    /// rebuilds that never happen.
    ///
    /// Driven against a scratch with **no source file in it**, so cargo cannot
    /// parse a target out of the manifest the function writes: it fails before
    /// rustc runs, its message goes to stderr, and `--message-format=json`
    /// leaves stdout empty. That is `Control::NoBuild`, the variant a forged
    /// fixture goes through, and it costs no compilation to reach.
    #[test]
    fn a_control_that_could_not_run_answers_none() {
        let held = crate::check::LeavesNothingBehind::new("scratch");
        let scratch = held.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        // A fixture that exists, because the function reads one after cargo
        // returns — what is under test is the answer, not the reading.
        let fixture = root().join(FIXTURES).join(format!("{}.rs", SHAPES[0].0));
        assert!(
            fixture.is_file(),
            "{} is not a fixture, so this test proves nothing",
            fixture.display()
        );

        let answered =
            super::fails_the_same_way_without_the_crate(&scratch, &fixture, "the trait bound");
        let _ = std::fs::remove_dir_all(&scratch);
        assert_eq!(
            answered,
            Ok(None),
            "a hollow build cargo could not run answered as though it had run. A step \
             that did not run is not a step that passed, and `controlled_by` counts \
             every `Some` as a control that decided — so this is the eleven-skip run \
             reported as eleven proofs"
        );
    }

    /// Each announcement names its own step, and no other's.
    ///
    /// **Not only that each starts with `skipped: `.** Asserted that way alone,
    /// collapsing `Control::NoBuild`'s to `format!("skipped: {cargo_said}")`
    /// leaves every other `wall::` test green, measured — a warn line saying a
    /// fixture was skipped and not why, in the one gate whose entire subject is
    /// a control that did not run. A reader deciding whether
    /// `controlled == checked` can be trusted has nothing else to read.
    ///
    /// Apart from its neighbour because the two together pass a hundred
    /// lines: that one holds what a verdict *answers*, this one holds what it
    /// *says*.
    #[test]
    fn every_announcement_names_the_step_that_did_not_run() {
        // Each variant names its own cause **and no other variant's**: three
        // arms becoming one by copy is the failure this shape has, and mere
        // distinctness does not see it — `format!("skipped: {cargo_said}")` is
        // distinct from both of its neighbours.
        let causes = [
            (
                super::Control::NoBuild,
                "did not compile and reported no diagnostic",
            ),
            (super::Control::OnlyResolution, "failed at name resolution"),
            (
                super::Control::SomeOtherReason,
                "neither the expected one nor a name that failed to resolve",
            ),
        ];
        let mut sentences = Vec::new();
        for (did_not_run, cause) in &causes {
            let said = super::announcement(did_not_run, "cargo said something");
            assert!(
                said.starts_with("skipped: "),
                "a step that did not run has to announce it: {said}"
            );
            assert!(
                said.contains(cause),
                "{did_not_run:?} announces a skip and not its cause, so the warn line \
                 does not say which step did not run: {said}"
            );
            assert!(
                said.contains("empty-crate control could not be asked"),
                "{did_not_run:?} does not say which control was not asked, so a reader \
                 cannot tell what the count it is missing from is: {said}"
            );
            for (other, elsewhere) in &causes {
                assert!(
                    other == did_not_run || !said.contains(elsewhere),
                    "{did_not_run:?} carries {other:?}'s cause, so two arms say the \
                     same thing and at least one of them is wrong: {said}"
                );
            }
            // **And what it answers, beside what it says.** The two travel
            // together or the warn line and the count contradict each other:
            // a variant that announces a skip and hands back `Some` is the
            // forgery that ships through `fails_the_same_way_without_the_crate`,
            // printed as eleven skips under a line claiming eleven rebuilds.
            assert_eq!(
                super::answer(did_not_run),
                None,
                "{did_not_run:?} announces `{said}` and answers with a verdict anyway, \
                 so `controlled_by` counts a control that did not run"
            );
            sentences.push(said);
        }
        // **And cargo's own error goes to exactly one of them.** `NoBuild` is
        // the variant where cargo itself failed and its message is on stderr
        // with nothing on stdout, so it is the only one with an error to pass
        // on — and the only one that may blame cargo for what rustc decided.
        assert!(
            sentences[0].contains("cargo said something"),
            "the one variant where cargo itself failed does not hand on what it said, \
             so the stderr line that explains the skip is dropped: {}",
            sentences[0]
        );
        for said in &sentences[1..] {
            assert!(
                !said.contains("cargo said something"),
                "a variant that read rustc's diagnostics blames cargo's stderr for \
                 them: {said}"
            );
        }
        sentences.sort();
        sentences.dedup();
        assert_eq!(
            sentences.len(),
            causes.len(),
            "two of the three announcements are one sentence, so the warn line cannot \
             say which step did not run"
        );
    }

    #[test]
    fn the_stub_declares_every_door_the_shapes_name() {
        // **The control, held by a test.** With `stub_for`,
        // `fails_the_same_way_without_the_crate` and
        // `Outcome::WouldFailWithoutTheCrate` named nowhere but their own
        // definitions, `"nc-log"` → `"nc-logx"`, one token, makes the stub
        // empty: measured, eleven `skipped:` lines and the `ok` line still
        // saying every fixture "was rebuilt against a crate of the same name
        // with the guarantee removed", directly beneath eleven lines saying it
        // was asked of none. That sentence is the whole substance of criterion
        // 1's clean run.
        let stub = super::stub_for("nc-log", "a_string_cannot_be_a_log_value");
        assert!(
            !stub.is_empty(),
            "the hollow build gets an empty `nc-log`, so every fixture fails at name \
             resolution and the control decides none of them"
        );
        assert!(
            super::stub_for("nc-catalog", "a_string_cannot_be_a_log_value").contains("Level"),
            "the seal fixture's `Event::level` returns `nc_catalog::Level`, so an empty \
             `nc-catalog` puts that fixture back at name resolution"
        );
        for (fixture, door, _, _) in super::SHAPES {
            // A macro door is written `nc_log::literal!`; the stub declares it
            // as `macro_rules! literal`, so the `!` is not part of the name.
            let bare = door
                .rsplit("::")
                .next()
                .expect("a door names an item")
                .trim_end_matches('!');
            assert!(
                stub.contains(bare),
                "`{fixture}` is refused at `{door}` and the stub declares no `{bare}`, so \
                 the hollow build cannot get past name resolution for it"
            );
        }
    }

    #[test]
    fn a_fixture_that_compiles_is_a_finding() {
        // **The compile-fail gate, watched firing.** It is acceptance
        // criterion 2's whole proof — "a dynamic string cannot compile into a
        // log event, proven by a compile-fail test rather than by a comment" —
        // and every other test around it asserts that the fixtures *do* fail.
        // Without a run that reports one that does not, the reporting half is
        // `§19.3`'s hazard in the gate that answers the criterion: a gate nobody
        // has watched fire is a gate nobody knows is wired up.
        //
        // A whole tree is copied because `build_fixture` resolves `crates/nc-log`
        // and `crates/nc-catalog` relative to the root it is handed, and the
        // fixture has to compile against the real crates to be a real answer.
        // Only the manifests and sources those two paths need are copied, plus
        // one fixture, replaced with a file that compiles.
        let held = crate::check::LeavesNothingBehind::new("wall-fires");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(FIXTURES)).expect("writable");
        // **Removed however this test ends.** A removal as the last line after
        // five assertions leaks a copied `crates/` tree on a panic in any of
        // them — measured, 1.2 MB a failing run. `build_fixture` in this same
        // file records the same defect for itself —
        // *"1246 directories, 139 distinct pids, 24 GB"* — and its neighbour
        // `a_control_that_could_not_run_answers_none` removes before it
        // asserts.
        // Every crate, because `nc-log` reaches `nc-canon`, `nc-ids` and
        // `nc-catalog`, and a fixture that fails to build for a missing
        // dependency exercises a different branch than one that compiles — which
        // is the branch this test is about.
        copy_into(&root().join("crates"), &scratch.join("crates"));
        // The workspace manifest, so the path dependencies resolve the way they
        // do in the real tree.
        std::fs::copy(root().join("Cargo.toml"), scratch.join("Cargo.toml"))
            .expect("the workspace manifest is there");

        // One fixture, and it compiles.
        std::fs::write(
            scratch.join(FIXTURES).join("a_probe_that_compiles.rs"),
            // It names `nc_log` because every fixture must: a file that does
            // not touch the crate under test is not about it. What is being
            // probed here is that a fixture *compiling* is a finding.
            // It names `nc_log` in code and an identifier in backticks because
            // every fixture must: a file that does not touch the crate under
            // test is not about it, and a declaration naming nothing is one no
            // rule can hold to a subject. What is being probed here is that a
            // fixture *compiling* is a finding.
            "//! EXPECT: nothing, because this `Literal` fixture compiles\n\
             fn main() {\n    let _ = nc_log::literal!(\"a literal\");\n}\n",
        )
        .expect("writable");

        let (findings, checked, controlled) = compile_fail(&scratch).expect("the gate runs");
        assert_eq!(checked, 1, "one fixture, and it was the one written");
        assert_eq!(findings.len(), 1, "{findings:#?}");
        // **And the control decided none of them**, which is the third number.
        // `answer`, `outcome` and `controlled_by` are each pinned by a test of
        // their own, and all three still agree while the value pushed into
        // `decided` is forged upstream of them: one token
        // — `let output_kind = output.clone()` written
        // `let output_kind = Outcome::FailedCorrectly` — made every fixture score
        // as a rebuild that had happened, measured, and `check` printed
        // `ok 11 fixture(s) refuse to compile … and each was rebuilt against a
        // crate of the same name with the guarantee removed` with `check passed`
        // and every test in this crate green. This fixture *compiles*, so no
        // control was asked about it and the honest answer is zero.
        assert_eq!(
            controlled, 0,
            "the fixture compiled, so the empty-crate control decided nothing about it \
             — and the gate counted {controlled} rebuild(s) that never happened"
        );
        assert!(
            findings[0].what.contains("compiled"),
            "the finding does not say what happened: {}",
            findings[0].what
        );

        // **And the three refusal verdicts, each watched firing.** Undriven,
        // `not_about_its_own_shape`'s two clauses and the caret rule each
        // weaken green: measured, the door-in-a-comment clause reverted to
        // `code.contains(door)` — the hole its own doc records — left every
        // test passing, as did deleting it outright and comparing the expected
        // sentence by length. `compile_fail`'s `Outcome::Compiled` was driven
        // above; these are the other half of what it can say.
        std::fs::remove_file(scratch.join(FIXTURES).join("a_probe_that_compiles.rs"))
            .expect("this test wrote it");
        for (named, body, said) in [
            // **Named after a real `SHAPES` row**, because the door clause is
            // reached only for a fixture the table knows: an unknown name makes
            // the lookup return early and the clause is never asked. The door is
            // in a comment and nowhere else, which is the hole its own doc
            // records — four fixtures about a locally declared trait, every
            // fixture green, and the type wall deleted underneath them.
            (
                "a_string_cannot_be_a_log_value.rs",
                "//! EXPECT: the trait bound `String: LogValue` is not satisfied\n\
                 // nc_log::LogValue, named where no rule can reach it\n\
                 fn main() {\n    let _: u8 = \"a string\";\n}\n",
                // The earlier check refuses the walk before
                // `not_about_its_own_shape`'s door clause is reached, which is
                // why reverting that clause to `code.contains(door)` leaves the
                // suite green: for this shape the two overlap, and the stronger
                // one is above. What is watched here is that *something* refuses
                // a fixture naming its door only in a comment.
                "never names `nc_log` outside its comments",
            ),
            (
                "a_probe_expecting_another_shape.rs",
                "//! EXPECT: a sentence no SHAPES row declares\n\
                 fn main() {\n    let _ = nc_log::literal!(1_u8);\n}\n",
                "names nothing in backticks",
            ),
        ] {
            std::fs::write(scratch.join(FIXTURES).join(named), body).expect("writable");
            // A refusal verdict may be a finding or a refusal of the whole walk
            // — `not_about_its_own_shape` stops the gate, because a fixture that
            // defines its own reason makes every later verdict meaningless.
            // Either is the rule firing; silence is not.
            let said_here = match compile_fail(&scratch) {
                Ok((found, ran, _)) => {
                    assert_eq!(ran, 1, "one fixture, and it was the one written");
                    found
                        .iter()
                        .map(|finding| finding.what.clone())
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                Err(refusal) => refusal,
            };
            assert!(
                said_here.contains(said),
                "`{named}` produced nothing saying `{said}`: {said_here}"
            );
            std::fs::remove_file(scratch.join(FIXTURES).join(named)).expect("this test wrote it");
        }

        // **And the verdict for a refusal the wall had nothing to do with**, end
        // to end: elsewhere `Outcome::WouldFailWithoutTheCrate` appears at its
        // own push site, in one unit assertion about `outcome`, and in a
        // comment, so the hollow rebuild, the answer it gives and what the
        // caller does with that answer are three call frames nothing else
        // drives. Measured: the answer taken as `…?.or(Some(false))` in
        // `build_fixture_in`, beside a hollow manifest whose path dependency on
        // the catalog stub is misspelt, left every test passing while `check`
        // printed eleven `skipped: the hollow build did
        // not compile…` lines, then `ok 11 fixture(s) … each was rebuilt against
        // a crate of the same name with the guarantee removed`, then
        // `check passed`.
        //
        // The probe is named after a real `SHAPES` row and declares that row's
        // sentence, names `nc_log::Origin` in its code, and rustc's caret sits
        // inside the `nc_log::Origin { … }` block — so it clears
        // `conjures_its_own_reason`, `not_about_its_own_shape` and the caret
        // rule, which are the three that catch the other forgeries. What it
        // does not clear is the rebuild.
        //
        // What actually fails the rebuild is the line below: the forgery binds
        // its runtime string to a `nc_log::Literal` by name, and a type variable
        // accepts it while the real crate's `Literal` does not. The same
        // sentence comes back from the control, the fixture proves nothing the
        // stub does not also prove, and that is the finding.
        //
        // **And it puts the string in `service`, which is the field this row is
        // named for.** Put in `version`, the caret-token column catches it
        // three rules earlier, and the probe tests that rule instead of the
        // rebuild. A forgery has to clear every text rule to be a test of the
        // control.
        let forged = "an_origin_cannot_hold_a_runtime_string.rs";
        std::fs::write(
            scratch.join(FIXTURES).join(forged),
            "//! EXPECT: expected `Literal`, found `&str`\n\
             fn main() {\n\
             \x20   let given = std::env::args().nth(1).unwrap_or_default();\n\
             \x20   let held: &'static str = given.leak();\n\
             \x20   let _ = nc_log::Origin {\n\
             \x20       service: { let forged: nc_log::Literal = held; forged },\n\
             \x20       version: nc_log::literal!(\"9.9.9\"),\n\
             \x20   };\n\
             }\n",
        )
        .expect("writable");
        let (findings, checked, controlled) = compile_fail(&scratch).expect("the gate runs");
        assert_eq!(checked, 1, "one fixture, and it was the one written");
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings[0]
                .what
                .contains("fails the same way with no `nc-log` behind it"),
            "a fixture whose refusal does not need `nc-log` was taken as the wall's \
             proof, and the gate said: {}. (This message used to blame \
             `macro_rules!`, which is `conjures_its_own_reason`'s vocabulary three \
             rules away — and this assertion is the only thing standing between the \
             tree and a fixture the hollow rebuild also refuses, so its wording is \
             load-bearing.)",
            findings[0].what
        );
        assert_eq!(
            controlled, 0,
            "the control found this fixture is not the wall's, and the gate still counted \
             {controlled} rebuild(s) as having proved one. `controlled` is the number the \
             `ok` line turns into `each was rebuilt against a crate of the same name with \
             the guarantee removed`"
        );
        std::fs::remove_file(scratch.join(FIXTURES).join(forged)).expect("this test wrote it");
        drop(held);
    }

    /// Copies a directory tree, for the fixture test above.
    fn copy_into(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("writable");
        for entry in std::fs::read_dir(from).expect("readable").flatten() {
            let source = entry.path();
            let target = to.join(entry.file_name());
            if source.is_dir() {
                if source.file_name().is_some_and(|name| name == "fixtures") {
                    continue;
                }
                copy_into(&source, &target);
            } else {
                let _ = std::fs::copy(&source, &target);
            }
        }
    }

    #[test]
    fn every_text_building_spelling_is_a_finding() {
        // **`BUILDS_TEXT`'s floor.** `LEAKS` has the test below — a
        // written-out mirror and a length — and `MINTED_FROM_TEXT` and
        // `AWAY_FROM_A_CALL` have `every_public_route_to_an_identifier_is_named`,
        // which derives `nc-ids`' surface and compares both ways. Without this
        // the list is referenced at its definition and its single use and
        // nowhere else. Measured: deleting `.push_str(` left `check passed` with
        // the whole run byte-identical, and cutting the list back to the three
        // spellings its own doc names left `check passed` with every test green
        // and a real `.join(` inside a `log.record(` window in
        // `crates/nc-log/src/ship.rs` — restored, the same tree reports it.
        //
        // Written out here rather than looped over `BUILDS_TEXT`, for the reason
        // the test below gives: a loop over the list proves each entry reaches
        // the matcher and says nothing about an entry that was removed. Spelled
        // with `concat!` for the same reason the list is — this file is walked
        // by its own gate.
        const MUST_BE_REFUSED: [&str; 15] = [
            concat!("format", "!("),
            concat!(".to_str", "ing()"),
            concat!(".to_ow", "ned()"),
            concat!("String", "::from("),
            concat!(".push_", "str("),
            concat!("write", "!("),
            concat!(".jo", "in("),
            concat!(".rep", "lace("),
            concat!(".conc", "at()"),
            concat!("write", "ln!("),
            concat!("String", "::from_utf8_", "lossy("),
            concat!(".repl", "acen("),
            concat!(".to_string_", "lossy("),
            concat!(".to_lower", "case()"),
            concat!(".to_upper", "case()"),
        ];
        for needle in MUST_BE_REFUSED {
            assert!(
                super::BUILDS_TEXT.contains(&needle),
                "`{needle}` builds text and BUILDS_TEXT does not hold it"
            );
            // Inside a logging window, because that is the only place this rule
            // looks — and built from the needle, so the spelling never sits in
            // this source as code.
            // A needle that opens a bracket needs it closed, or the call runs
            // past the end of the text and the gate says so instead.
            let closes = if needle.ends_with('(') { ")" } else { "" };
            let source = format!("log.record(&Refused {{\n    at: x{needle}{closes},\n}});\n");
            let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &source, false);
            assert_eq!(findings.len(), 1, "{needle}: {findings:#?}");
            assert!(
                findings[0].what.contains(needle),
                "the finding does not name what it refused: {}",
                findings[0].what
            );
        }
        assert_eq!(
            super::BUILDS_TEXT.len(),
            MUST_BE_REFUSED.len(),
            "a needle was added to BUILDS_TEXT and this list was not"
        );
    }

    #[test]
    fn every_leak_spelling_is_a_finding() {
        // **One needle per line.** A line holding two needles under
        // `findings.len() == 1` masks `into_boxed_str` behind whichever matches
        // first, and measured that way, deleting `String::leak`, `Vec::leak`,
        // `into_boxed_str` or `Box::into_raw` left the suite green — and
        // `String::leak` is the specific call the type wall is designed against.
        //
        // **Written out here, not read from `LEAKS`.** A loop over the list
        // proves each entry reaches the matcher and says nothing about an entry
        // that was removed. Spelled with `concat!` for the same reason `LEAKS`
        // is: this file is walked by its own gate.
        const MUST_BE_REFUSED: [&str; 6] = [
            concat!("Box", "::leak"),
            concat!("String", "::leak"),
            concat!("Vec", "::leak"),
            concat!(".lea", "k()"),
            concat!("into_boxed", "_str"),
            concat!("Box", "::into_raw"),
        ];
        for needle in MUST_BE_REFUSED {
            assert!(
                super::LEAKS.contains(&needle),
                "`{needle}` manufactures a static string and LEAKS does not hold it"
            );
            // Built from the needle, so the spelling never sits in this source.
            let line = format!("let n = x{needle};\n");
            let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &line, false);
            assert_eq!(findings.len(), 1, "{needle}: {findings:#?}");
            assert!(
                findings[0].what.contains(needle),
                "the finding does not name what it refused: {}",
                findings[0].what
            );
        }
        assert_eq!(
            super::LEAKS.len(),
            MUST_BE_REFUSED.len(),
            "a needle was added to LEAKS and this list was not"
        );
    }

    #[test]
    fn a_call_rustfmt_wrapped_is_still_one_call() {
        // **rustfmt breaks a call past 100 columns**, and a rule reading a line
        // finds the one-line form and not the wrapped form, which is what a
        // real logging call site looks like. Measured through the gate: 1
        // finding and 0.
        // Written a line at a time, because the marker is same-line only and the
        // finding points at a line — which, in a string continued with `\`, is
        // not a Rust line of its own.
        let wrapped = [
            "        log.record(&events::PruneWithinLimit {", // allow-logging: this gate's own fixture
            "            table: format!(\"{user}\"),",
            "            rows: 1,",
            "        });",
        ]
        .join("\n")
            + "\n";
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &wrapped, false);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(
            findings[0].line, 2,
            "the finding points at the value, not at the call's first line — which is \
             also what makes the marker mean one line: it used to point here at line 1, \
             and a marker there excused every needle in the twenty lines below it"
        );

        // **A value built on the line before is not this gate's to catch**, and
        // that is deliberate rather than a gap: `format!` produces a `String`,
        // no `LogValue` accepts one, and the field of a generated struct is
        // typed — so the shape below does not compile at all. The source gate
        // exists for what the types *cannot* refuse, which is a leaked static
        // string, and the `LEAKS` needles are not window-scoped for exactly that
        // reason.
        let before = "        let table = format!(\"{user}\");\n\
                      \x20       log.record(&events::PruneWithinLimit { table });\n"; // allow-logging: this gate's own fixture
        let left_over = no_formatting_at_a_logging_call("src/x.rs", before, false).0;
        assert!(
            left_over.is_empty(),
            "the window grew past the call it is about: {left_over:?}"
        );
        // The leaked form of the same shape, which *would* compile, is caught
        // wherever it sits.
        let leaked = [
            "        let table: &'static str = x.leak();", // allow-logging: this gate's own fixture
            "        log.record(&events::PruneWithinLimit { table });",
        ]
        .join("\n")
            + "\n";
        assert_eq!(
            no_formatting_at_a_logging_call("src/x.rs", &leaked, false)
                .0
                .len(),
            1
        );

        // **And a `format!` that is not part of the call is not a finding**, so
        // the window is the call rather than "anywhere near it".
        let elsewhere = "        log.record(&events::PruneWithinLimit { rows: 1 });\n\
                         \x20       let message = format!(\"{user}\");\n"; // allow-logging: this gate's own fixture
        let left_over = no_formatting_at_a_logging_call("src/x.rs", elsewhere, false).0;
        assert!(
            left_over.is_empty(),
            "a format! after the call closed was read as part of it: {left_over:?}"
        );
    }

    #[test]
    fn formatting_at_a_logging_call_is_a_finding() {
        let (findings, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "log.record(&Whatever { table: name.to_string() });\n", // allow-logging: this gate's own fixture
            false,
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn an_ordinary_logging_call_is_not_a_finding() {
        // The other half. Without it, a gate that flagged everything would pass
        // the two tests above.
        let (findings, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "log.record(&PruneWithinLimit { table: table.name, rows, max_rows });\n",
            false,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn text_made_into_an_identifier_at_a_logging_call_is_a_finding() {
        // **A way through the wall, and the one the registry recommends.**
        // `type = "id"` is refused by `nc-catalog-compiler` with
        // the reason that a caller can mint one out of a person's name or
        // number and the type wall cannot tell — and `type = "snowflake"`, the
        // kind that refusal steers callers to, has the same property:
        // `Snowflake::from_crockford` takes twelve caller-chosen characters and
        // `to_crockford` writes them back verbatim. Measured from an ordinary
        // consumer crate with no `format!`, no `.to_string()`, no `.leak()` and
        // no macro:
        //
        //     from_crockford("CPF123456789") -> Ok(…) -> "CPF123456789"
        //
        // An eleven-digit CPF fits in twelve characters, which is the example
        // the `id` refusal itself uses.
        let smuggled = [
            "        let unit = nc_ids::Snowflake::from_crockford(&user_text)?;",
            "        log.record(&events::JournalAppended {", // allow-logging: this gate's own fixture
            "            sequence: 1,",
            "            work_unit: Some(nc_ids::Snowflake::from_crockford(&user_text)?),",
            "        });",
        ]
        .join("\n")
            + "\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", &smuggled, false);
        assert_eq!(calls, 1, "the call was not opened");
        // **Three, and the extra two are the point.** The window rule reports
        // the value built *at* the call, on line 2. The file rule reports both
        // conversions wherever they are — including the `let` on line 1, which
        // is the cheapest way past the window: no helper, no second file, one
        // line above the call.
        assert_eq!(findings.len(), 3, "{findings:#?}");
        assert!(
            findings.iter().all(|f| f.what.contains("from_crockford")),
            "a finding does not name what it refused: {findings:#?}"
        );
        // **On the needle's own line, not on the call's opening line** — line
        // 4, not line 2 where `log.record(` sits. A rule scanning the whole call
        // as one string reports where the call began; the window loop, which
        // gives this rule the hatch its three siblings have, the
        // `allow-logging:` one, also makes it say where the conversion is. A
        // hatch works on the needle's own line, so a rule that reports a
        // different line is a rule whose hatch cannot be written where it
        // belongs.
        assert_eq!(
            findings.iter().filter(|f| f.line == 4).count(),
            2,
            "the call-site finding and the file-rule finding both belong on the line \
             the conversion is written on: {findings:#?}"
        );
        assert!(
            findings.iter().any(|f| f.line == 1),
            "the `let` above the call was not reported: {findings:#?}"
        );

        // The other constructor that takes caller text, in the wrapped shape.
        let opaque = [
            "        log.record(&events::PruneWithinLimit {", // allow-logging: this gate's own fixture
            "            table: nc_ids::OpaqueId::new(name)?,",
            "        });",
        ]
        .join("\n")
            + "\n";
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &opaque, false);
        assert_eq!(findings.len(), 2, "{findings:#?}");

        // **And the same construction away from a call is this gate's too.**
        // Turning text into an identifier is ordinary, and a gate refusing
        // every one would be switched off within a day — both true, and still
        // not a reason to leave it out: measured, the conversion one line above
        // the call put twelve caller-chosen characters into `work_unit` with the
        // step reporting `build no text`. The rule is scoped to the crates that
        // may log, which is what keeps it from refusing every ordinary
        // conversion in `nc-ids` and `nc-journal` — those crates cannot log.
        let elsewhere = "        let unit = Snowflake::from_crockford(&text)?;\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", elsewhere, false);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(
            calls, 0,
            "no call was opened, and the finding is not about one"
        );

        // Every needle is a function that exists, or it is a needle matching
        // nothing — which is `§19.3` at the granularity of one line.
        // **An alias is the caller's to choose, on this list too.** A
        // path-spelled `OpaqueId::new(` beside an unspelled `from_crockford(`
        // lets the evasion the call-site needles answer work here: `use
        // nc_ids::OpaqueId as Who;` then `Who::new(text)?`.
        for spelling in [
            "Who::new(",              // allow-logging: this gate's own fixture
            "OpaqueId::new(",         // allow-logging: this gate's own fixture
            "nc_ids::OpaqueId::new(", // allow-logging: this gate's own fixture
        ] {
            let minted = format!(
                "        log.record(&E {{ who: {spelling}&text)? }});\n" // allow-logging: this gate's own fixture
            );
            let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", &minted, false);
            assert_eq!(calls, 1, "`{spelling}` opened no window");
            // The window rule reads every spelling; the file rule reads the two
            // it can name without an alias, so the path forms are seen twice and
            // `Who::new(` once. Both numbers are the rules agreeing, not
            // disagreeing.
            let expected = usize::from(spelling.ends_with("OpaqueId::new(")) + 1;
            assert_eq!(
                findings.len(),
                expected,
                "`{spelling}` was not read: {findings:#?}"
            );
        }

        for needle in super::MINTED_FROM_TEXT {
            assert!(
                needle.ends_with('('),
                "`{needle}` is not spelled as a call and would match a mention"
            );
        }
    }

    /// A constructor is a value, and every way of naming one is a mint.
    ///
    /// A needle carrying a `(` is the *call* form: four spellings
    /// reach `Snowflake::from_crockford` without ever writing `from_crockford(`,
    /// each one measured passing such a needle.
    #[test]
    fn naming_the_constructor_is_enough_however_it_is_spelled() {
        for spelling in [
            "        let mint = nc_ids::Snowflake::from_crockford;",
            "        let unit = Snowflake::from_crockford::<Realm>(&user_text)?;",
            "use nc_ids::Snowflake::from_crockford as mint;",
            "        const MINT: fn(&str) -> Option<Snowflake> = Snowflake::from_crockford;",
            "        let id = OpaqueId::new(user_text);",
        ] {
            let text = format!("{spelling}\n");
            let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &text, false);
            assert_eq!(
                findings.len(),
                1,
                "`{spelling}` minted from text: {findings:#?}"
            );
        }
    }

    /// A literal that ends where the scanner thought it did.
    ///
    /// **Every line below hides a real mint from a scanner wrong about
    /// literals**, and the first two are what `cargo fmt` writes by itself:
    /// `r"\"` is a raw string holding one backslash, and an escape-tracking
    /// scanner treats the `\` as escaping the closing quote, so it stays inside
    /// a literal and blanks the rest of the line — measured, `check passed` with
    /// `Snowflake::from_crockford(&user_text)` two tokens later. A scanner that
    /// is wrong about where a literal *ends*
    /// blinds every rule that reads the line after it, not just the one about
    /// the literal.
    #[test]
    fn a_literal_ends_where_the_scanner_says_it_does() {
        for hiding in [
            "        let sep = r\"\\\"; let u = Snowflake::from_crockford(&t)?;",
            "        let q = '\"'; let u = Snowflake::from_crockford(&t)?;",
            "        let s = r#\"a \" b\"#; let u = Snowflake::from_crockford(&t)?;",
            "        let s = \"\\\\\"; let u = Snowflake::from_crockford(&t)?;",
            "        let a: &'static str = \"x\"; let u = OpaqueId::new(t);",
        ] {
            let text = format!("{hiding}\n");
            let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &text, false);
            assert!(
                findings.iter().any(|f| f.what.contains("identifier")),
                "a mint after a literal was not seen: {hiding}"
            );
        }
        // And the other direction: a needle *inside* a raw string is still prose.
        let (prose, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "        let s = r\"from_crockford\";\n",
            false,
        );
        // Empty, not merely free of one word: `all(…)` over an empty list is
        // true whatever the scanner did, and this closure was measured never
        // seeing an element.
        assert!(
            prose.is_empty(),
            "a needle inside a raw string was read as a use: {prose:#?}"
        );
        // The blanking keeps every line, so a finding still knows where it is.
        for source in [
            "let a = \"x\\\"y\";\n",
            "let b = r#\"a \" b\"#;\n",
            "let c = '\\'';\n",
            "let d: &'static str = \"\";\n",
            "/* nested /* like this */ still a comment */ let e = 1;\n",
            "let f = r\"\n a raw string\n over lines\n\";\n",
        ] {
            assert_eq!(
                super::readable(source).lines().count(),
                source.lines().count(),
                "the stripper changed the line count of: {source}"
            );
        }
    }

    /// One marker excuses one line of a call, not the whole call.
    ///
    /// **Not up to twenty.** Tested on the line that *opens* the call, with the
    /// finding computed from the window after it, a single comment beside
    /// `.record(` silences every `format!`, `.to_string()` and `String::from` in
    /// the call below it — nine needles from one comment, measured. The gate's
    /// own paragraph says "same line only, because rustfmt is free to move a
    /// comment that sits above a statement", and this holds it to that.
    #[test]
    fn a_marker_on_the_call_does_not_excuse_the_lines_below_it() {
        let call = [
            "        log.record(&events::PruneWithinLimit { // allow-logging: the opener only",
            "            table: format!(\"{user}\"),",
            "            other: user.to_string(),",
            "            rows: 1,",
            "        });",
        ]
        .join("\n")
            + "\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", &call, false);
        assert_eq!(calls, 1, "the call was not opened");
        assert_eq!(
            findings.len(),
            2,
            "one marker on the opener excused the values below it: {findings:#?}"
        );
        assert_eq!(findings[0].line, 2);
        assert_eq!(findings[1].line, 3);

        // And a marker on the value's own line does excuse that value, and only
        // it — the floor for the rule above, which would otherwise be satisfied
        // by a gate that ignored the marker entirely.
        let one_excused = [
            "        log.record(&events::PruneWithinLimit { // allow-logging: the opener only",
            "            table: format!(\"{user}\"), // allow-logging: this `format!(` is measured",
            "            other: user.to_string(),",
            "        });",
        ]
        .join("\n")
            + "\n";
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &one_excused, false);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].line, 3);
    }

    /// One reason excuses one spelling, not every spelling on the line.
    ///
    /// **A per-line hatch is still a blanket on its own line.** A reason of a
    /// single letter would excuse three `BUILDS_TEXT` needles at once on one
    /// line — measured — and the paragraph beside the minting rule states the
    /// rule this applies: the reason has to name what it excuses.
    #[test]
    fn one_reason_excuses_one_spelling() {
        let three = "        log.record(&E { a: format!(\"{x}\"), b: y.to_string(), \
                     c: String::from(z) }); // allow-logging: a\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", three, false);
        assert_eq!(calls, 1, "the call was not opened");
        assert_eq!(
            findings.len(),
            3,
            "one letter excused three spellings: {findings:#?}"
        );

        // Naming one excuses that one and leaves the others.
        let named = "        log.record(&E { a: format!(\"{x}\"), b: y.to_string() }); \
                     // allow-logging: the `format!(` is a fixture\n";
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", named, false);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings[0].what.contains(".to_string()"),
            "the wrong one survived: {findings:#?}"
        );

        // And a reason naming it as part of a longer word is not naming it: the
        // shape that lets `democrates` excuse a claim about `crates` and
        // `chronology` excuse `chrono`.
        let substring = "        log.record(&E { a: y.to_string() }); \
                         // allow-logging: about to_stringify\n";
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", substring, false);
        assert_eq!(
            findings.len(),
            1,
            "a substring of a longer word excused it: {findings:#?}"
        );
    }

    /// And prose about the constructor is not a use of it.
    ///
    /// The floor for the test above: a needle that matched anywhere on the line
    /// would report every doc string and error message that names the thing it
    /// refuses — including this gate's own `fix` text — and an excuse added to
    /// quiet prose is an excuse that stays behind on real code.
    #[test]
    fn the_constructor_named_inside_a_string_is_prose() {
        for prose in [
            "        return Err(\"call from_crockford to read one back\".to_owned());",
            "        let fix = \"OpaqueId::new takes caller text\";",
        ] {
            let text = format!("{prose}\n");
            let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &text, false);
            // Empty, for the reason above.
            assert!(
                findings.is_empty(),
                "prose was read as a mint: {findings:#?}"
            );
        }
        // And the stripper keeps the line, so a column still lines up.
        let line = "let a = \"x\\\"y\";\n";
        assert_eq!(super::readable(line).lines().count(), 1);
    }

    #[test]
    fn the_associated_function_form_opens_the_window_too() {
        // **`nc_log::Log::record(&log, …)` opens a window too.** It is
        // ordinary Rust, it is what a macro expands to, and with `.record(`
        // alone everything inside it is invisible — measured, with a
        // `from_crockford` in the argument:
        // `findings=0 calls=0` against `findings=1 calls=1` for the method form.
        let associated = [
            "        nc_log::Log::record(", // allow-logging: this gate's own fixture
            "            &log,",
            "            &events::JournalAppended {",
            "                sequence: 1,",
            "                work_unit: Some(Snowflake::from_crockford(&user_text)?),",
            "            },",
            "        );",
        ]
        .join("\n")
            + "\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", &associated, false);
        assert_eq!(calls, 1, "the associated-function form opened no window");
        // Once from the window and once from the file scan, both naming it.
        assert_eq!(findings.len(), 2, "{findings:#?}");
        assert!(
            findings.iter().all(|f| f.what.contains("from_crockford")),
            "{findings:#?}"
        );

        // **A wrapper of the consumer's own opens no window, and is read
        // anyway.** The conversion is at a call to the wrapper and the `record`
        // is in another function entirely, so the window rule sees neither — and
        // "no finding" here is the helper one function up, measured with the
        // same twelve characters reaching `work_unit` and the step reporting
        // `build no text`. The file rule sees the
        // wrapper's own body, which is where the door is.
        let wrapper = "        self.emit(&E { u: Snowflake::from_crockford(&t)? });\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", wrapper, false);
        assert_eq!(calls, 0, "a wrapper is not a logging call");
        assert_eq!(findings.len(), 1, "the wrapper's own body was not read");
        assert_eq!(findings[0].line, 1);

        // **And the name of the type is the caller's to choose.** A needle of
        // `Log::record(` is renamed away by `use nc_log::Log as Journal;` in one
        // line — measured in a consumer's own source, in a file the gate had
        // read and counted, after which the step went on to assert that the
        // crate "has not logged yet":
        //
        // ```text
        // Journal::record(log, &events::JournalAppended { … from_crockford(t) … })
        //   -> ok 1 crate(s) outside nc-log may log … check passed
        // ```
        for spelling in [
            "Journal::record(",  // allow-logging: this gate's own fixture
            "log::Log::record(", // allow-logging: this gate's own fixture
            "Wrapper::record(",  // allow-logging: this gate's own fixture
            // **`line` is `record` without the shipping**, and the value reaches
            // the same JSON: `record` is `self.shipper.send(self.line(event))`.
            "log.line(",  // allow-logging: this gate's own fixture
            "Log::line(", // allow-logging: this gate's own fixture
        ] {
            let renamed = format!(
                "        {spelling}
            &log,
            &E {{ u:                  Snowflake::from_crockford(&t)? }},
        );
"
            );
            let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", &renamed, false);
            assert_eq!(calls, 1, "`{spelling}` opened no window");
            // Twice: once because it is at a call, once because it is in a
            // crate that may log at all.
            assert_eq!(findings.len(), 2, "`{spelling}`: {findings:#?}");
        }

        // Every needle is spelled as a call, or it matches a mention of one.
        for needle in super::OPENS_A_LOGGING_CALL {
            assert!(needle.ends_with('('), "`{needle}` is not spelled as a call");
        }
    }

    #[test]
    fn the_gate_counts_calls_and_not_lines() {
        // **A count of source files is non-zero from the first source file in
        // the tree** whether or not one logging call is ever read — a gate whose
        // evidence of being wired up is evidence of something else. The count is
        // of calls opened.
        let two = [
            "        log.record(&A { rows: 1 });", // allow-logging: this gate's own fixture
            "        let x = 1;",
            "        log.record(&B {", // allow-logging: this gate's own fixture
            "            rows: 2,",
            "        });",
        ]
        .join("\n")
            + "\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", &two, false);
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(calls, 2, "a wrapped call and a one-line call are two calls");

        // A file with no logging call in it counts none, which is what makes the
        // number mean something when it is zero.
        let (_, calls) = no_formatting_at_a_logging_call("src/x.rs", "fn main() {}\n", false);
        assert_eq!(calls, 0);
    }

    #[test]
    fn formatting_away_from_a_logging_call_is_not_a_finding() {
        // `format!` is ordinary Rust. The gate is about logging call sites, and
        // one that refused every `format!` in the repository would be switched
        // off within a day.
        let (findings, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "let message = format!(\"{count}\");\n",
            false,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn the_one_exemption_is_scoped_and_not_a_hole() {
        // A fixture's whole job is to be the attack.
        let leak = "let x = format!(\"{y}\").leak();\n"; // allow-logging: this gate's own fixture
        assert_eq!(
            no_formatting_at_a_logging_call("crates/nc-log/src/ship.rs", leak, false)
                .0
                .len(),
            1,
            "the method spelling was the one the needles missed"
        );
        let left_over =
            no_formatting_at_a_logging_call("crates/nc-log/fixtures/a.rs", leak, false).0;
        assert!(
            left_over.is_empty(),
            "the gate reported its own proof: {left_over:?}"
        );
    }

    #[test]
    fn a_url_in_a_string_does_not_blind_the_rest_of_the_line() {
        // `split("//")` cuts the line at the first slash pair, so anything after a
        // URL in a string literal is invisible — and this crate's own default
        // endpoint is a URL, so it is the natural carrier.
        let (findings, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "let e = \"http://127.0.0.1:9428\"; Box::leak(x)\n", // allow-logging: this gate's own fixture
            false,
        );
        assert_eq!(findings.len(), 1, "a URL hid the rest of the line");
        // And a genuine trailing comment still ends the code.
        let (commented, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "let e = 1; // this mentions Box::leak in prose\n", // allow-logging: this gate's own fixture
            false,
        );
        assert!(commented.is_empty(), "{commented:?}");
        // An escaped quote inside a string must not flip the state back.
        let (escaped, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "let e = \"a\\\" // b\"; Box::leak(x)\n", // allow-logging: this gate's own fixture
            false,
        );
        assert_eq!(escaped.len(), 1, "an escaped quote confused the scan");
    }

    #[test]
    fn a_character_literal_does_not_close_the_window_early() {
        // **`balance_from` honours `'` as well as `"`.** Otherwise a `)` inside
        // a character literal moves the depth, so the window closes at that line
        // and anything after it in the same call is never read — with the call
        // still counted as inspected:
        //
        // ```text
        // log.record(&E {
        //     delim: ')',
        //     table: format!("{user}"),
        // });                              -> findings=0 calls=1
        // ```
        let (findings, calls) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "log.record(&E {\n    delim: ')',\n    table: format!(\"{user}\"),\n});\n",
            false,
        );
        assert_eq!(calls, 1, "the call was not counted");
        assert_eq!(
            findings.len(),
            1,
            "a `)` in a character literal closed the window early: {findings:#?}"
        );

        // And a lifetime is an apostrophe that opens nothing. `&'static str` is
        // ordinary at these call sites, and reading it as a literal would hold
        // the window open to the end of the file.
        let (quiet, calls) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "log.record(&E::<'a> { name: literal!(\"x\"), n: 1 });\nlet y = 2;\n",
            false,
        );
        assert_eq!(calls, 1);
        assert!(
            quiet.is_empty(),
            "a lifetime was read as a literal: {quiet:#?}"
        );

        // An escaped apostrophe inside a character literal closes on the second.
        let (escaped, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "log.record(&E {\n    q: '\\'',\n    table: format!(\"{user}\"),\n});\n",
            false,
        );
        assert_eq!(escaped.len(), 1, "an escaped apostrophe confused the scan");
    }

    #[test]
    fn a_string_that_spans_lines_does_not_close_the_window() {
        // **The scan's state carries across lines.** `call_from` calls
        // `balance_from` once per line, and with each call beginning fresh the
        // second half of a multi-line string reads as code — a `)` in it closes
        // the window, the `format!` after it is never read, and the call is
        // counted as inspected anyway, measured:
        //
        // ```text
        // multiline-string  findings=0 calls=1
        // control           findings=1 calls=1
        // raw-string        findings=0 calls=1
        // ```
        //
        // All three are the same call.
        let control = "log.record(&E {\n    note: \"he said (stop now)\",\n    \
                       table: format!(\"{user}\"),\n});\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", control, false);
        assert_eq!(
            (findings.len(), calls),
            (1, 1),
            "the control: {findings:#?}"
        );

        let across = "log.record(&E {\n    note: \"he said (stop\n          now)\",\n    \
                      table: format!(\"{user}\"),\n});\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", across, false);
        assert_eq!(
            (findings.len(), calls),
            (1, 1),
            "a string across two lines closed the window: {findings:#?}"
        );

        // A raw string, whose quotes do not end it and whose backslashes do not
        // escape: a scan counting every `"` as a flip comes out at -1 on
        // `r#"let r = r#"a "b )" c"#;"#`.
        let raw = "log.record(&E {\n    note: r#\"he said \"stop) now\"#,\n    \
                   table: format!(\"{user}\"),\n});\n";
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", raw, false);
        assert_eq!(
            (findings.len(), calls),
            (1, 1),
            "a raw string closed the window: {findings:#?}"
        );

        // And a `//` *inside* a string is not a comment, so the window does not
        // lose the rest of the line to the comment stripper.
        let url = "log.record(&E {\n    note: \"see http://example.invalid/(x)\",\n    \
                   table: format!(\"{user}\"),\n});\n";
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", url, false);
        assert_eq!(findings.len(), 1, "a URL in a string: {findings:#?}");
    }

    #[test]
    fn a_call_the_window_could_not_finish_is_not_counted_as_read() {
        // **`calls` is what `check` prints as evidence this gate is wired up.**
        // A call longer than the window, or one whose parentheses never close,
        // returned as though it had been read whole is counted the same as one
        // that was — so the number says "inspected" about a call the gate gave
        // up on, which is `§19.3` one report along.
        let padding = "    a: 1,\n".repeat(super::LONGEST_CALL + 2);
        let long = format!("log.record(&E {{\n{padding}    table: format!(\"{{user}}\"),\n}});\n");
        let (findings, calls) = no_formatting_at_a_logging_call("src/x.rs", &long, false);
        assert_eq!(calls, 0, "a call the window gave up on was counted as read");
        assert!(
            findings.iter().any(|f| f.what.contains("too long")),
            "the gate said nothing about a call it could not read: {findings:#?}"
        );

        // And an ordinary call is counted, so the rule is about the window and
        // not about counting nothing.
        let (quiet, calls) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "log.record(&E { table: literal!(\"x\"), rows: 1 });\n",
            false,
        );
        assert_eq!(calls, 1);
        assert!(quiet.is_empty(), "{quiet:#?}");
    }

    #[test]
    fn excusing_a_call_does_not_lower_the_number_that_proves_coverage() {
        // **The hatch does not subtract from the count.** `calls` is what
        // `check` prints as evidence this gate is wired up; leaving an excused
        // call site uncounted makes a file whose logging calls are all excused
        // report the same number as a file with none, and using the hatch lower
        // the number that exists to prove coverage. The floor is on files read,
        // so nothing else goes red.
        //
        // An excused call is one this gate looked at and was told not to judge.
        // That is still a call it inspected, and the two numbers say different
        // things: `findings` is what it judged, `calls` is what it read.
        let judged = "log.record(&E { table: literal!(\"x\"), rows: 1 });\n";
        let (findings, judged_calls) = no_formatting_at_a_logging_call("src/x.rs", judged, false);
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(judged_calls, 1);

        let excused = "log.record(&E { table: format!(\"{u}\") }); \
                       // allow-logging: this `format!(` is this gate's own fixture\n";
        let (findings, excused_calls) = no_formatting_at_a_logging_call("src/x.rs", excused, false);
        assert!(
            findings.is_empty(),
            "the marker did not excuse it: {findings:#?}"
        );
        assert_eq!(
            excused_calls, 1,
            "an excused call site was not counted as inspected, so using the hatch \
             lowers the number that proves this gate reads anything"
        );

        // And a file with no logging call in it counts none, or the assertion
        // above would be satisfied by counting every line.
        let (_, none) = no_formatting_at_a_logging_call("src/x.rs", "let x = 1;\n", false);
        assert_eq!(none, 0);
    }

    #[test]
    fn a_bare_marker_excuses_nothing() {
        // The hole, tested rather than the rule. An escape hatch that costs
        // nothing to use is one that gets used, and then the gate applies to
        // whatever nobody bothered to mark.
        let (findings, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "Box::leak(x) // allow-logging:\n", // allow-logging: this gate's own fixture
            false,
        );
        assert_eq!(findings.len(), 1, "a bare marker excused it");
    }

    #[test]
    fn the_escape_hatch_works_on_the_same_line_and_needs_a_reason() {
        let (findings, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            // The reason names `Box::leak`, which is what it excuses.
            "Box::leak(x) // allow-logging: `Box::leak` here is this gate's own fixture\n", // allow-logging: `Box::leak` is this gate's own fixture
            false,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// The scan reaches a leak wherever in a file it sits.
    ///
    /// **A line floor cannot ask this**, which is the whole reason this test
    /// exists. Measured without it: three lines making the scan judge only
    /// each file's first line leave every count exactly where it was —
    /// `86 source file(s) manufacture no static string`, `check passed`,
    /// byte-identical to a clean run, with a `Box::leak` planted in `nc-ids`.
    /// Adding a floor over the line count does not close it either: a plant
    /// that skips the judgement and increments the counter anyway keeps the
    /// number honest — measured too, still green.
    ///
    /// What answers "did the walk reach the line" is a defect at a line nothing
    /// can predict. The depth and the line come from this process's own id, so a
    /// filter tuned to any fixed shape misses it.
    #[test]
    fn the_scan_reaches_a_leak_wherever_it_sits() {
        let mut body = String::new();
        // **Past the longest file this suite reads, not within a window of
        // forty.** A depth of `pid % 40 + 3` puts the defect between lines 3
        // and 42, so `if index >= 64 { read += 1;
        // continue; }` is invisible to it while the scan judges the first 64
        // lines of every file — measured, with a real `Box::leak` in `nc-ids`
        // and `check passed`. A fixture longer than the longest real file has no such
        // window: any filter that lets a real file's tail through lets this
        // through too.
        let longest = crate::gates::longest_file_read(root());
        let depth = longest + 1 + std::process::id() as usize % 100;
        // **A real comparison, not `longest + 1 > longest`.** An assertion
        // reading the value it has just derived passes for every `longest`
        // including zero — it would pass with `longest_file_read` replaced
        // by a literal. What has to be true is that the fixture is longer than
        // anything the gate really reads, so the number is fetched again, from
        // the tree.
        let measured = crate::gates::longest_file_read(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.."
        )));
        assert!(
            depth > measured && measured > 100,
            "the fixture is {depth} line(s) and the longest file this suite reads is \
             {measured}: a filter tuned above the fixture would be invisible to it"
        );
        for _ in 1..depth {
            body.push_str("// nothing here\n");
        }
        body.push_str("let forever = Box::leak(text.into_boxed_str());\n");
        let (findings, read, _) =
            super::nothing_manufactures_a_static_string("src/planted.rs", &body, &[]);
        assert!(
            findings.iter().any(|finding| finding.line == depth),
            "the scan did not reach a leak on line {depth} of {read} — a filter between \
             the counter and the judgement leaves every count where it was: {findings:#?}"
        );
        assert!(
            read >= depth,
            "read {read} line(s) and the leak is on {depth}"
        );
    }

    #[test]
    /// A reason that names something else excuses nothing.
    ///
    /// **The hole, not the rule.** A leak-scan hatch accepting any non-empty
    /// reason excuses whatever lands beside it. Measured, in a crate that has
    /// nothing to do with logging —
    ///
    /// ```text
    /// Box::leak(text.into_boxed_str()) // allow-logging: this module is about JSON
    ///   ->  ok … 86 source file(s) manufacture no static string
    ///       check passed
    /// ```
    ///
    /// — and removing only the marker reported the leak. `cargo fmt` joins lines,
    /// so a marker written about the statement above ends up beside this one; the
    /// three sibling hatches require the reason to name the needle, for that
    /// measured reason.
    fn a_reason_that_names_something_else_excuses_nothing() {
        let (findings, _) = no_formatting_at_a_logging_call(
            "src/x.rs",
            "Box::leak(x) // allow-logging: this module is about JSON, not logging\n", // allow-logging: `Box::leak` is this gate's own fixture
            false,
        );
        assert_eq!(
            findings.len(),
            1,
            "a reason naming nothing this line does excused it: {findings:?}"
        );
        // **And a reason naming a *different* one of the six.** Asking whether
        // the reason names a needle `applies` lets this *file* hold — which is
        // all six in every crate but the fixtures — lets one marker excuse any
        // leak on its line, and `cargo fmt` puts a marker written about one
        // statement beside the next: five of the six reasons a line can carry
        // are the wrong one, and all five pass, measured.
        let another = format!(
            "{}(x) // {} {}, the arena below owns it\n",
            concat!("String", "::leak"),
            super::ALLOW,
            concat!("Box", "::leak"),
        );
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &another, false);
        assert_eq!(
            findings.len(),
            1,
            "a reason naming another of the six needles excused this one: {findings:?}"
        );
        // **And a line carrying two of the six needs both named.**
        // `Box::leak(text.into_boxed_str())` is the canonical spelling and
        // carries two, so a reason about the harmless one — *into_boxed_str
        // only moves the allocation, it leaks nothing*, which is **true** —
        // excuses the leak beside it under a rule asking for any one. That is
        // the reason somebody would actually write, which is what makes the
        // hole worth closing rather than arguing about.
        let both = format!(
            "{}({}(x)) // {} {} only moves the allocation, it leaks nothing\n",
            concat!("Box", "::leak"),
            concat!("into_boxed", "_str"),
            super::ALLOW,
            concat!("into_boxed", "_str"),
        );
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &both, false);
        assert_eq!(
            findings.len(),
            1,
            "a reason about the harmless half of a two-needle line excused the leak: \
             {findings:?}"
        );
        // And naming both still excuses it.
        let named_both = format!(
            "{leak}({boxed}(x)) // {marker} `{leak}` here is the arena's, and `{boxed}` \
             only moves the allocation\n",
            leak = concat!("Box", "::leak"),
            boxed = concat!("into_boxed", "_str"),
            marker = super::ALLOW,
        );
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &named_both, false);
        assert!(
            findings.is_empty(),
            "a reason naming both needles on the line excused nothing: {findings:?}"
        );
        // **And the other direction, because too strong is the other way to be
        // wrong**: the reason that does name the spelling on the line still
        // excuses it, so this is a hatch that has to be earned and not a hatch
        // that was removed.
        let named = format!(
            "{leak}(x) // {marker} `{leak}`, and the arena below owns it\n",
            leak = concat!("String", "::leak"),
            marker = super::ALLOW,
        );
        let (findings, _) = no_formatting_at_a_logging_call("src/x.rs", &named, false);
        assert!(
            findings.is_empty(),
            "a reason that names the spelling on the line excused nothing: {findings:?}"
        );
    }

    #[test]
    fn the_wall_holds_and_the_proof_actually_ran() {
        // Acceptance criterion 2, and its own coverage floor: a fixtures
        // directory that lost its files would otherwise pass with nothing run.
        let (findings, checked, controlled) =
            compile_fail(root()).expect("the fixtures are readable");
        assert!(findings.is_empty(), "{findings:#?}");
        // **The control runs for every fixture, not merely for some.**
        // `checked` counts fixtures and is unmoved by a control that could not
        // be asked — measured, the gate printed "each was rebuilt against a
        // crate of the same name with the guarantee removed" over eleven while
        // the control had decided seven, and no number in the run moved; one
        // letter in the hollow manifest's path dependency on the catalog stub
        // killed the control for all eleven, with the whole section
        // byte-identical.
        assert_eq!(
            controlled, checked,
            "{checked} fixture(s) were judged and the empty-crate control decided \
             {controlled} of them, so the rest are refused by something nobody measured"
        );
        // **The shapes, not the count.** With a floor of `checked >= 4`,
        // deleting the borrowed-`&str` fixture — the one this module calls the
        // shape the wall most depends on — while copying another to a second
        // name keeps the count at four and the gate green, measured.
        assert!(checked >= SHAPES.len(), "only {checked} fixture(s) ran");
        // **And the number itself.** Both floors on "how many fixtures ran"
        // read `SHAPES.len()` — the count on disk against the count in the
        // table — so deleting a fixture *and* its row moves both sides
        // together. Measured: the borrowed `&str` fixture removed with its row,
        // and then `impl LogValue for &'static str` added to `value.rs` — what
        // this module's header rules out, because `String::leak()`
        // manufactures a real `&'static str` out of anything — gave
        // `ok 10 fixture(s) refuse to compile … and each was rebuilt` and
        // `check passed`. The only thing that objected was a *string* census in
        // another file, whose remedy text tells a reader how to finish the
        // removal.
        //
        // Every other defence around `SHAPES` is two-directional. This is the
        // absolute number, written down, so shrinking the table is a two-place
        // edit and the second place is a line a reader of the diff meets.
        assert_eq!(
            SHAPES.len(),
            super::THE_SHAPES_THE_WALL_REFUSES,
            "the wall names {} shape(s) and this repository has {} \
             written down. A shape deleted with its fixture moves every count in this gate \
             together, so the number is here on its own",
            SHAPES.len(),
            super::THE_SHAPES_THE_WALL_REFUSES
        );
        let present: std::collections::BTreeSet<String> = std::fs::read_dir(root().join(FIXTURES))
            .expect("the fixtures are there")
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension()? == "rs").then(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
            })
            .collect();
        for (shape, _, _, _) in SHAPES {
            assert!(
                present.contains(shape),
                "`{shape}` is gone, and it is one of the shapes the wall has to refuse"
            );
        }
        // **And the other direction**: `SHAPES ⊆ disk` says every named fixture
        // exists, and says nothing about a fixture on disk that the list does
        // not name. Such a fixture runs — the walk finds it — but its deletion is
        // invisible, because the floor is `SHAPES.len()` and the list never
        // mentioned it. `SUITES` and `CORE_POLICIES` are both compared in both
        // directions, and so is this.
        let mut on_disk: Vec<String> = present.iter().cloned().collect();
        on_disk.sort();
        let mut declared: Vec<String> = SHAPES
            .iter()
            .map(|(shape, _, _, _)| (*shape).to_owned())
            .collect();
        declared.sort();
        assert_eq!(
            declared, on_disk,
            "SHAPES and crates/nc-log/fixtures disagree about which shapes the wall refuses"
        );
    }
}

#[cfg(test)]
mod the_exemption_is_one_directory {
    use super::{EXEMPT, LEAKS, applies};

    /// `applies` exempts the fixtures directory and nothing else.
    ///
    /// **Widening it moves no printed number.** `read` and the file count both
    /// sit outside the filter, so one extra `starts_with` and a real `Box::leak`
    /// in `nc-journal` leave every number — 15, 4, 86, 9, 29 — byte identical to
    /// a clean run, measured, and a test probing two paths cannot see it. A
    /// count at the call site does not fix that: it compares one
    /// constant with another and moves for the same reason the others do not.
    ///
    /// What decides an exclusion list is a case per thing it could grow to
    /// include. One per member crate, so widening to any of them fails here.
    #[test]
    fn every_crate_but_the_fixtures_is_judged() {
        let members = [
            "crates/nc-authz/src/lib.rs",
            "crates/nc-canon/src/lib.rs",
            "crates/nc-catalog/src/lib.rs",
            "crates/nc-catalog-compiler/src/lib.rs",
            "crates/nc-guest/src/lib.rs",
            "crates/nc-ids/src/lib.rs",
            "crates/nc-journal/src/lib.rs",
            "crates/nc-kernel/src/lib.rs",
            "crates/nc-local/src/lib.rs",
            "crates/nc-log/src/lib.rs",
            "crates/nc-platform/src/lib.rs",
            "crates/nc-policy/src/lib.rs",
            "crates/nc-prune/src/lib.rs",
            "crates/nc-schema/src/lib.rs",
            "plugins/reference/src/lib.rs",
            "xtask/src/main.rs",
            // The fixtures' own crate, outside the fixtures directory.
            "crates/nc-log/src/ship.rs",
            "crates/nc-log/tests/anything.rs",
        ];
        for needle in LEAKS {
            for file in members {
                assert!(
                    applies(needle, file),
                    "`{needle}` is not judged in `{file}`, so the one exemption has grown \
                     past `{EXEMPT}` — and widening it moves no printed number, which is \
                     why this list exists"
                );
            }
            // And the directory the exemption is actually for.
            assert!(
                !applies(needle, "crates/nc-log/fixtures/a_borrowed_str.rs"),
                "the fixtures are judged, and they are files whose whole purpose is to \
                 hold the defect rustc must refuse"
            );
        }

        // **And the signature rule, whose exclusions `applies` does not know.**
        // The loop above drives `applies`, which knows only `EXEMPT` and
        // discards its needle, so those six iterations are one assertion six
        // times. Measured: one `&& !file.starts_with("crates/nc-journal/")`
        // among the signature rule's exclusions and a real
        // `pub fn remembered(text: String) -> &'static str` in the journal left
        // `read=1911` unchanged and that loop green.
        //
        // A case per member against the predicate the rule actually applies —
        // **the members cargo builds, not a hand list of ten paths.** A crate
        // added tomorrow would get no case in either direction from a hand
        // list, which is the shape
        // this test exists to prevent one level down. Two members are excused by
        // name and are judged below instead: `nc-log` builds the type the wall
        // is for, and `crates/nc-catalog/src/` is emitted from the registry.
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let built = crate::members::members(root).expect("cargo lists this workspace");
        assert!(
            built.len() >= 9,
            "cargo lists {} member(s), which is not this workspace",
            built.len()
        );
        for member in &built {
            let file = format!("{}/src/lib.rs", member.path);
            if file.starts_with("crates/nc-log/") || file.starts_with("crates/nc-catalog/src/") {
                continue;
            }
            assert!(
                super::the_signature_rule_judges(&file),
                "`{file}` is outside the signature rule. Its exclusions are a list, and a \
                 list is a claim: every member of this workspace is judged, or the \
                 exclusion is written here with the reason"
            );
        }
        // And the hand list above is still every member, so the two agree about
        // the ground rather than one of them quietly shrinking.
        for member in &built {
            assert!(
                members
                    .iter()
                    .any(|file| file.starts_with(&format!("{}/", member.path))),
                "cargo builds `{}` and the list in this test does not name it",
                member.path
            );
        }
        // The three directories the exclusions are for, and nothing else.
        for excused in [
            "crates/nc-log/fixtures/a_borrowed_str.rs",
            "crates/nc-log/src/value.rs",
            "crates/nc-log/src/ship.rs",
            "crates/nc-catalog/src/i18n.rs",
            "crates/nc-catalog/src/lib.rs",
        ] {
            assert!(
                !super::the_signature_rule_judges(excused),
                "`{excused}` is judged by the signature rule, and it is one of the three \
                 the exclusion names"
            );
        }
        // A file merely *near* an excused directory is judged: the exclusion is
        // a prefix, and a prefix that swallowed a sibling would be a hole.
        for near in [
            "crates/nc-log-extra/src/lib.rs",
            "crates/nc-catalog/tests/anything.rs",
            "crates/nc-catalog-compiler/src/emit.rs",
        ] {
            assert!(
                super::the_signature_rule_judges(near),
                "`{near}` is next to an excused directory and is not in one"
            );
        }
    }

    /// The hatch can name the item behind any visibility Rust writes.
    ///
    /// **One rule, not two copies with the second narrower.** Cut at its first
    /// `(`, the head of `pub(crate) fn only_for(` ends at the paren inside the
    /// *visibility* — so the head is `pub`, no keyword splits it,
    /// [`super::the_item_named_on`] answers `None`, and the reason can name
    /// nothing: every `pub(crate)`, `pub(super)` and `pub(in path)` item in the
    /// tree falls outside the escape hatch this file documents, while
    /// [`super::promotes_to_static`] strips exactly those visibilities and goes
    /// on finding them.
    ///
    /// Measured, in `crates/nc-prune/src/postgres.rs`, one token changed and
    /// its own existing correct marker left alone: `pub(crate) async fn only_for(`
    /// is `error … this function hands back a &'static str it was not given`,
    /// `check failed: 1 finding(s) about logging call sites`.
    ///
    /// The rule is loud in that direction, which costs a developer rather than
    /// hiding a leak — and a gate that is wrong in the loud direction teaches a
    /// reader to reach for the suppression, which is the failure this whole
    /// file is against.
    ///
    /// Both ways round on every spelling: a reason that names the item excuses
    /// it, and a reason that names a *neighbour* still does not.
    #[test]
    fn the_signature_hatch_reads_an_item_behind_any_visibility() {
        for visibility in [
            "",
            "pub ",
            "pub(crate) ",
            "pub(super) ",
            "pub(in crate::wall) ",
        ] {
            for form in [
                format!("{visibility}fn only_for(which: bool) -> &'static str {{"),
                format!("{visibility}async fn only_for(which: bool) -> &'static str {{"),
                format!("{visibility}const ONLY_FOR: fn(bool) -> &'static str = |it| \"a\";"),
                format!("{visibility}static ONLY_FOR: fn(bool) -> &'static str = |it| \"a\";"),
            ] {
                let named = super::the_item_named_on(&form);
                assert!(
                    named.as_deref() == Some("only_for") || named.as_deref() == Some("ONLY_FOR"),
                    "`{form}` declares an item this cannot name ({named:?}), so no reason \
                     can be written about it and the hatch is shut for every item behind \
                     `{visibility}`"
                );
                // And the hatch itself, which is what the name is for.
                let about_it = format!(
                    "    // {} `only_for` and `ONLY_FOR` hand back a column name from \
                     the catalog constant",
                    super::ALLOW
                );
                let lines = [about_it.as_str(), form.as_str()];
                assert!(
                    super::excuses_a_signature(&lines, 1),
                    "a reason naming `{form}` does not excuse it"
                );
                // A reason about the neighbour is still not about this item.
                let about_another = format!(
                    "    // {} `somewhere_else` is a catalog constant",
                    super::ALLOW
                );
                let lines = [about_another.as_str(), form.as_str()];
                assert!(
                    !super::excuses_a_signature(&lines, 1),
                    "a reason naming a different item excused `{form}`, so the repair \
                     above has widened the hatch instead of opening it"
                );
                // **And on the signature line itself, which is what the fix
                // text tells the reader to do.** Every fixture above writes the
                // marker on the line *above*, and no marker in the tree sits on
                // a signature line, so without this the arm serving the sentence
                // `mark the line` is driven by nothing: handed `None` in place of
                // the signature line it reads, the suite stays green, measured.
                let on_the_line = format!(
                    "{form} // {} `only_for` and `ONLY_FOR` hand back a column name \
                     from the catalog constant",
                    super::ALLOW
                );
                let lines = ["    let unrelated = 1;", on_the_line.as_str()];
                assert!(
                    super::excuses_a_signature(&lines, 1),
                    "a reason written on the signature line itself did not excuse \
                     `{form}`, though that is what the remedy tells the reader to write"
                );
            }
        }

        // **A `const` whose type is written without a paren.** The head is cut
        // before the type for this shape alone: past the cut lies `&'static
        // str` itself, and the keyword walk reads `str` out of it as the name
        // of the binding — so the reason could only excuse an item called
        // `str`, and every real reason would be refused. Nothing the wall hands
        // this function today lacks a paren, so deleting the clause is otherwise
        // invisible; the contract is the function's own, and the next caller is
        // the one this is written for.
        assert_eq!(
            super::the_item_named_on("const ONLY_FOR: &'static str = \"a\";").as_deref(),
            Some("ONLY_FOR"),
            "a `const` binding is named by the type it holds rather than by itself"
        );
    }
}

/// The surface `nc-ids` offers, held against the list this gate refuses.
///
/// Its own module because what it reads is a *different crate's* public API
/// rather than this file's behaviour, and because the scan it needs is worth
/// reading on its own.
#[cfg(test)]
mod the_routes_into_an_identifier {
    /// What follows the `fn` keyword, in any spelling a public function wears.
    ///
    /// `pub … const? async? unsafe? extern "abi"? fn` — the words between `pub`
    /// and `fn` are what a prefix list keeps missing, and a trait method has no
    /// `pub` at all because its visibility is the trait's.
    fn after_the_fn_keyword(line: &str, through_a_trait: bool) -> Option<&str> {
        let rest = match line.strip_prefix("pub ") {
            Some(rest) => rest,
            None if through_a_trait => line,
            None => return None,
        };
        let mut at = rest;
        loop {
            let (word, tail) = at.split_once(' ')?;
            match word {
                "fn" => return Some(tail),
                "const" | "async" | "unsafe" => at = tail,
                "extern" => {
                    at = tail.strip_prefix('"').map_or(tail, |abi| {
                        abi.split_once('"')
                            .map_or(tail, |(_, after)| after.trim_start())
                    });
                }
                _ => return None,
            }
        }
    }

    /// The type an `impl` line is for, and whether it is a trait impl.
    ///
    /// `impl X {` and `impl Trait for X {` — the type a `Self` in a signature
    /// below refers to. `impl<C: Clock> Minter<C>` has no space after the
    /// keyword, and reading only `impl ` leaves every generic impl invisible,
    /// which is a route the census would not see at all.
    ///
    /// **The generic parameters are stepped over first.** `impl<C: Clock>
    /// Minter<C> {` reads as an impl of nothing otherwise, and the walk then
    /// keeps the *previous* impl's name — measured, `Minter::mint` was recorded
    /// as `SystemClock::mint`, a route this list would have had to accept under
    /// a name no reader could resolve.
    fn the_type_an_impl_is_for(line: &str) -> Option<(String, bool)> {
        let rest = line
            .strip_prefix("impl")
            .filter(|rest| rest.starts_with([' ', '<']))
            .map(str::trim_start)?;
        let rest = rest.strip_prefix('<').map_or(rest, |generics| {
            let mut depth = 1_usize;
            let mut at = 0;
            for (index, character) in generics.char_indices() {
                match character {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            at = index + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            generics[at..].trim_start()
        });
        let named = rest.rsplit_once(" for ").map_or(rest, |(_, after)| after);
        Some((
            named
                .trim_end_matches(" {")
                .split(['<', ' '])
                .next()
                .unwrap_or_default()
                .to_owned(),
            rest.contains(" for "),
        ))
    }

    /// Every `Type::name` in `nc-ids` whose return type carries an identifier.
    ///
    /// Its own function so the test below stays inside clippy's line bound, and
    /// so the scan can be read on its own: it is a text walk, and what it can
    /// and cannot see is the whole question. An `impl` whose type it fails to
    /// read keeps the previous one — measured, `Minter::mint` recorded as
    /// `SystemClock::mint` with the generic list not stepped over — so a route
    /// under a name no reader can resolve is the shape to watch for here.
    fn the_routes_nc_ids_offers() -> Vec<String> {
        let root =
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/nc-ids/src"));
        let mut found: Vec<String> = Vec::new();
        // **Every `.rs` under the crate, not one directory.** A single
        // `read_dir` leaves a route in a `src/<module>/mod.rs` — the shape every
        // crate reaches for as a module grows — outside the census entirely.
        let mut sources: Vec<std::path::PathBuf> = Vec::new();
        crate::gates::walk(root, &mut |path| {
            if path.extension().is_some_and(|kind| kind == "rs") {
                sources.push(path.to_path_buf());
            }
        })
        .expect("nc-ids is a member of this workspace");
        sources.sort();
        // **And the names an identifier answers to.** Asking whether the
        // *spelling* of a return type holds `Snowflake` or `OpaqueId` misses
        // `pub type WorkUnitId = Snowflake;` and a constructor returning
        // `Option<WorkUnitId>` — a new public route from caller text into a
        // logged identifier with `check passed` and `COVERED` unmoved, measured.
        // An alias is the same type, so the census reads the aliases first.
        let mut answers_to: Vec<String> = vec![
            "Snowflake".to_owned(),
            "OpaqueId".to_owned(),
            "Self".to_owned(),
        ];
        for path in &sources {
            let text = std::fs::read_to_string(path).expect("a readable source file");
            // **A declaration joined to its `;`.** Reading one line and asking
            // for `=` on it misses that `cargo fmt` breaks a type alias after
            // the `=` as soon as the line passes a hundred columns — so the
            // wrapped form registers no alias, and a `pub const fn` handing the
            // aliased type back is not a route. Measured: the same alias one
            // character shorter is caught, one character longer is not, and
            // `cargo fmt --check` is clean on both. `static_aliases`, the
            // sibling of this pass in this same file, joins to the `;` too;
            // anything less is one rule written twice with the second copy
            // narrower.
            aliases_in(&text, &mut answers_to);
        }
        for path in &sources {
            let text = std::fs::read_to_string(path).expect("a readable source file");
            routes_in(&text, &answers_to, &mut found);
        }
        found
    }

    /// Every route one source file offers, given the names an identifier
    /// answers to.
    ///
    /// Its own function so a fixture can be fed to it. What this walk can and
    /// cannot see is the whole question, and two ways for it to be wrong only a
    /// fixture shows: a `pub trait` read or not read depending on which `impl`
    /// happens to precede it in the file, and a private module-level `fn` after
    /// a trait impl reported as a public route.
    fn routes_in(text: &str, answers_to: &[String], found: &mut Vec<String>) {
        {
            let mut carrying = String::new();
            // Whether the block being read is `impl Trait for X`. A trait
            // method carries no `pub` and is as public as the trait: `impl
            // FromStr for Snowflake` — the counterpart of the `Display` this
            // type already has, and the one the API guidelines ask for — is a
            // new route from caller text to a logged identifier, and a census
            // without this cannot see it.
            let mut through_a_trait = false;
            // **A signature joined across lines.** Reading one line and asking
            // for `pub fn NAME(` and `->` on it misses `cargo fmt`'s own wrapped
            // form — which it produces as soon as a signature is long, and
            // which `crates/nc-ids/src/naming.rs` contains. `segments`, earlier
            // in this file, joins signatures for the same reason.
            let mut joined = String::new();
            for raw in text.lines() {
                let line = raw.trim();
                if !joined.is_empty() {
                    joined.push(' ');
                    joined.push_str(line);
                    if !(line.ends_with('{') || line.ends_with(';') || line.contains("-> ")) {
                        continue;
                    }
                }
                // **The block ends where the item does.** Set by an `impl` line
                // and never cleared, these two make what the walk does with a
                // trait depend on which `impl` happens to sit above it in the
                // file: measured, the same five-line `pub trait` was invisible
                // after an inherent `impl` and, a hundred lines lower, was read as
                // `MintError::identifier` — a route under a name no reader can
                // resolve, which is the exact shape the paragraph on
                // `the_type_an_impl_is_for` says it watches for. Worse in the
                // other direction: a *private* module-level `fn` after any trait
                // impl was reported as a public route, and the only way to
                // silence it is to write a route that does not exist into
                // `COVERED`.
                //
                // A top-level item closes with a `}` in column zero, which is
                // what `cargo fmt --check` guarantees of every file this walks.
                if raw == "}" {
                    through_a_trait = false;
                    carrying = String::new();
                }
                // The impl this line opens, if it opens one.
                if let Some((named, through)) = the_type_an_impl_is_for(line) {
                    through_a_trait = through;
                    carrying = named;
                }
                // **And a trait *declaration*, whose methods are as public as it
                // is.** `pub trait Identified { fn identifier(&self) -> Snowflake; }`
                // is a route into a logged identifier for every type that
                // implements it, and a walk reading only `impl` lines misses it.
                // **And `unsafe` is a word between `pub` and `trait`.** The
                // sealed-type rule reads both spellings and its comment cites
                // *this* walk for the lesson, so this walk reads both too: a
                // rule copied in one direction and not the other is the same
                // defect as a rule written twice.
                if let Some(named) = line
                    .strip_prefix("pub trait ")
                    .or_else(|| line.strip_prefix("pub unsafe trait "))
                    .or_else(|| line.strip_prefix("pub(crate) trait "))
                    .or_else(|| line.strip_prefix("pub(crate) unsafe trait "))
                {
                    through_a_trait =
                        line.starts_with("pub trait ") || line.starts_with("pub unsafe trait ");
                    carrying = named
                        .split(['<', ' ', ':', '{'])
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                }
                let reading = if joined.is_empty() {
                    line.to_owned()
                } else {
                    joined.clone()
                };
                let reading = reading.as_str();
                // **Every spelling `pub fn` can wear**, which a two-prefix list
                // is not: `pub async fn` is the shape every door into an async
                // store takes, and it is the exact narrowness the `_for_tests`
                // door gate guards against too. A trait method's
                // visibility is the trait's, so it carries no `pub` of its own
                // and is public surface all the same.
                let Some(after) = after_the_fn_keyword(reading, through_a_trait) else {
                    joined.clear();
                    continue;
                };
                let Some((name, signature)) = after.split_once('(') else {
                    joined.clear();
                    continue;
                };
                let returns = signature
                    .rsplit_once("->")
                    .map(|(_, kind)| kind.trim().to_owned());
                let Some(returns) = returns else {
                    // The signature wraps: keep it and read the rest of it.
                    joined = if joined.is_empty() {
                        reading.to_owned()
                    } else {
                        String::new()
                    };
                    continue;
                };
                let name = name.to_owned();
                joined.clear();
                let hands_back = answers_to.iter().any(|name| {
                    returns.contains(name.as_str())
                        && (name != "Self" || answers_to.iter().any(|known| known == &carrying))
                });
                if hands_back {
                    // **Named before it is pushed**, because
                    // `the_verdicts_the_source_can_push` reads `.push(format!(`
                    // as a gate verdict wherever it is written — and this is a
                    // route in another crate, not something the gate says. The
                    // scanner is right to be broad; this is the spelling that
                    // keeps a census out of a list of sentences.
                    let route = format!("{carrying}::{name}");
                    found.push(route);
                }
            }
        }
    }

    /// The names an identifier answers to, read off one source file.
    fn aliases_in(text: &str, answers_to: &mut Vec<String>) {
        let mut declaring = String::new();
        for line in text.lines() {
            let line = line.trim();
            if declaring.is_empty() {
                let Some(rest) = line.strip_prefix("pub type ") else {
                    continue;
                };
                declaring = rest.to_owned();
            } else {
                declaring.push(' ');
                declaring.push_str(line);
            }
            if !declaring.contains(';') {
                continue;
            }
            let whole = std::mem::take(&mut declaring);
            let Some((name, of)) = whole.split_once('=') else {
                continue;
            };
            if answers_to.iter().any(|known| of.contains(known.as_str())) {
                answers_to.push(name.split(['<', ' ']).next().unwrap_or_default().to_owned());
            }
        }
    }

    /// A route is read from the item it sits on, whatever sits above it.
    ///
    /// **Both directions, measured.** A walk keeping the last `impl`'s name and
    /// its trait-ness makes a `pub trait` after an inherent `impl` invisible
    /// and records the same five lines a hundred lines lower as
    /// `MintError::identifier` — a route under a name no reader can resolve.
    /// A private module-level `fn` after any trait impl is then reported as
    /// public surface, which a developer can only silence by writing a route
    /// that does not exist into `COVERED`.
    #[test]
    fn a_route_is_read_from_the_item_it_is_on() {
        let answers_to = vec![
            "Snowflake".to_owned(),
            "OpaqueId".to_owned(),
            "Self".to_owned(),
        ];
        for (label, source, wanted) in [
            (
                "a public trait after an inherent impl",
                "impl Snowflake {\n    pub const fn bits(self) -> u64 {\n        0\n    }\n}\n\n\
                 pub trait Identified {\n    fn identifier(&self) -> Snowflake;\n}\n",
                vec!["Identified::identifier".to_owned()],
            ),
            (
                "the same trait after a trait impl",
                "impl core::fmt::Display for Snowflake {\n    fn fmt(&self) -> Result {\n        \
                 Ok(())\n    }\n}\n\npub trait Identified {\n    fn identifier(&self) -> \
                 Snowflake;\n}\n",
                vec!["Identified::identifier".to_owned()],
            ),
            // **And `unsafe` between `pub` and `trait`**, the spelling the
            // sealed-type rule one file-section away reads too, citing this walk
            // as its source.
            (
                "an unsafe public trait",
                "pub unsafe trait Identified {\n    fn identifier(&self) -> Snowflake;\n}\n",
                vec!["Identified::identifier".to_owned()],
            ),
            (
                "a private helper after a trait impl",
                "impl core::fmt::Display for Snowflake {\n    fn fmt(&self) -> Result {\n        \
                 Ok(())\n    }\n}\n\nfn rebuilt(bits: u64) -> Snowflake {\n    Snowflake(bits)\n\
                 }\n",
                Vec::new(),
            ),
        ] {
            let mut found = Vec::new();
            routes_in(source, &answers_to, &mut found);
            assert_eq!(found, wanted, "{label}");
        }

        // **And an alias `cargo fmt` has wrapped.** rustfmt breaks a type alias
        // after the `=` as soon as the line passes a hundred columns, and a
        // pass reading one line registers no alias for the wrapped form, so a
        // function handing the aliased type back is not a route. Both forms
        // here, because the rule is about the declaration and not about its
        // width.
        for (label, source) in [
            ("on one line", "pub type WorkUnitId = Snowflake;\n"),
            (
                "wrapped after the `=`",
                "pub type WorkUnitId =\n    Snowflake;\n",
            ),
        ] {
            let mut answers_to = vec!["Snowflake".to_owned()];
            aliases_in(source, &mut answers_to);
            assert!(
                answers_to.iter().any(|name| name == "WorkUnitId"),
                "{label}: the alias was not registered, so a route returning it is not one"
            );
        }
    }

    /// Every public route in `nc-ids` from anything to a logged identifier is
    /// one this gate covers.
    ///
    /// **A list cannot argue itself complete.** *"These two and no others,
    /// because these two are all there are"* misses
    /// `nc_ids::naming::FileName::parse`, a third route: it reaches
    /// `from_crockford` inside a crate this gate does not read, and hands the
    /// result back through
    /// `FileName::id()`. An enumeration cannot argue itself complete — so the
    /// enumeration is checked instead.
    ///
    /// **What it reads is the *surface*, not the call sites.** Every public
    /// function in `nc-ids` whose return type carries a `Snowflake` or an
    /// `OpaqueId` is a way for a caller to obtain one, whatever it takes as an
    /// argument; whether the value came from caller text is a question about
    /// the whole path, and the honest answer is that any of them can be at the
    /// end of one. Each is listed below with what covers it, and a route that
    /// arrives with no line here fails this test rather than passing quietly.
    ///
    /// It reads `nc-ids` and not the whole tree because `nc-ids` is where the
    /// two types are defined: nothing outside it can add a constructor for
    /// either, and a wrapper elsewhere still has to call one of these.
    #[test]
    fn every_public_route_to_an_identifier_is_named() {
        /// Each route, and the needle list that covers it.
        ///
        /// `Minter::mint` takes no argument at all, so there is no text for it
        /// to carry; it is named here so that the list is the whole surface
        /// rather than the part somebody thought was dangerous.
        /// The third column is what `AWAY_FROM_A_CALL` has to hold for this
        /// route — which is not always the route's own spelling. `FileName::id`
        /// is the second hop of two, and the hop that takes the text is
        /// `FileName::parse`, so that is the needle a whole-file scan has to
        /// carry. Written beside the route because the pair is what a reader
        /// needs to check, and because the assertion below is exact: an entry
        /// mis-spelled is an entry that catches nothing.
        const COVERED: [(&str, &str, &str); 4] = [
            (
                "Snowflake::from_crockford",
                "from_crockford(",
                "from_crockford",
            ),
            ("OpaqueId::new", "::new(", "OpaqueId::new"),
            ("FileName::id", "::parse(", "FileName::parse"),
            ("Minter::mint", "takes no argument", "takes no argument"),
        ];

        let mut found = the_routes_nc_ids_offers();
        found.sort_unstable();
        found.dedup();
        assert!(
            found.len() >= 2,
            "the scan read {found:?} from nc-ids, which is fewer routes than the two \
             this repository has always had: it stopped matching rather than finding a \
             smaller surface"
        );
        let mut named: Vec<String> = COVERED
            .iter()
            .map(|(route, _, _)| (*route).to_owned())
            .collect();
        named.sort_unstable();
        assert_eq!(
            found, named,
            "nc-ids' public surface offers routes to an identifier that this gate does \
             not name. Each one is a way a caller obtains a `Snowflake` or an `OpaqueId` \
             — add it to `MINTED_FROM_TEXT` and `AWAY_FROM_A_CALL`, or write down here \
             why it cannot carry caller text"
        );
        for (route, needle, away) in COVERED {
            if needle == "takes no argument" {
                continue;
            }
            assert!(
                super::MINTED_FROM_TEXT.contains(&needle),
                "`{route}` is written down as covered by `{needle}`, and \
                 `MINTED_FROM_TEXT` does not hold it. The whole-file list inside \
                 `no_identifier_is_minted_from_text` spells the same routes with their \
                 types, and this is what holds that one"
            );
            // **And the whole-file list.** Checked in one direction only, the
            // `FileName::parse` entry of `AWAY_FROM_A_CALL` can be deleted with
            // every test still passing, measured — and that list is what closes
            // the eight evasions its own doc enumerates, so half the rule would
            // be deletable in silence.
            // **The spelling, not the type it hangs off.** Comparing only the
            // half before the `::` lets `FileName::parse` — the hop that takes
            // the text, and the reason that entry names a type at all — be
            // changed to `FileName::id` with every test green while the
            // whole-file scan goes blind to a real mint, measured. A rule that
            // catches deletion and not mis-spelling is exactly as off.
            assert!(
                super::AWAY_FROM_A_CALL.contains(&away),
                "`{route}` is a public route into an identifier and \
                 `AWAY_FROM_A_CALL` does not hold `{away}`, so the whole-file half of \
                 the rule — the half that closes a `let` on the line above, a helper in \
                 the same file, and six more — does not cover it"
            );
        }
    }
}
