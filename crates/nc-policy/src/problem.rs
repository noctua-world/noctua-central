//! What the policy pack objected to.

/// One thing wrong with the pack, its cases, or the relationship between them.
///
/// The same two-field shape the gates use: `what` is the defect and `fix` is the
/// remedy, separate so that "there is a remedy" is a structural property rather
/// than a word count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Repository-relative, forward slashes.
    pub file: String,
    /// 1-indexed. Zero when the problem is about the file as a whole.
    pub line: usize,
    /// What is wrong, naming the offending policy or case.
    pub what: String,
    /// What to do about it.
    pub fix: String,
    /// Which check produced this, as a stable identity.
    ///
    /// A test that asserted on words in `what` could be satisfied by a different
    /// check that happened to mention the same policy. This is the name of the
    /// check itself, and it does not change when the prose does.
    pub check: &'static str,
}

/// The 1-indexed line `needle` is declared on, or 0.
///
/// **A comment is not a declaration.** Comments in these case files routinely
/// quote the case below them — the pack's own files open with a paragraph
/// explaining what each case is for — so a bare `contains` filed every
/// diagnostic against the prose above the case rather than the case. The
/// compiler's twin of this reader has skipped comment lines since a reviewer
/// paid for it; this one, written later, did not, and nothing noticed because
/// nothing in this crate ever read a `Problem`'s line back. Both halves reported
/// in round sixty-two.
///
/// **In both syntaxes this crate reads.** The skip knew `#`, which is the case
/// files' TOML; every *policy* is Cedar, whose comment is `//`, and the two
/// callers in `lib.rs` search Cedar text for a policy id. Every core policy in
/// this repository opens with a paragraph of `//` that names the rule below it
/// — `chain_depth.cedar`'s first line quotes its own id — so a diagnostic about
/// a policy was filed against the prose at the top of its file rather than
/// against the policy, which is exactly the defect the paragraph above records
/// being paid for once already, in the other syntax. Reported in round
/// sixty-four.
///
/// Both openers are refused unconditionally rather than chosen by the caller: a
/// `#` opens no line of Cedar and `//` opens no line of TOML, so neither can
/// refuse a declaration in the other's file, and a reader who adds a third kind
/// of file has one place to say so.
pub(crate) fn line_of(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|line| !a_comment(line) && line.contains(needle))
        .map_or(0, |index| index + 1)
}

/// Whether a line is a comment in either syntax the pack is written in.
///
/// TOML's `#` for the case files, Cedar's `//` for the policies.
fn a_comment(line: &str) -> bool {
    let opens = line.trim_start();
    opens.starts_with('#') || opens.starts_with("//")
}

impl core::fmt::Display for Problem {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Line zero means the problem is about the file as a whole — a schema
        // that will not parse, a policy with no denial test — and printing `:0`
        // sends a reader to a line that is not there.
        if self.line == 0 {
            write!(formatter, "{} — {}", self.file, self.what)
        } else {
            write!(formatter, "{}:{} — {}", self.file, self.line, self.what)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Problem, line_of};

    fn problem(line: usize) -> Problem {
        Problem {
            file: "policy/core/quorum.cedar".to_owned(),
            line,
            what: "something".to_owned(),
            fix: "do something else".to_owned(),
            check: "test",
        }
    }

    #[test]
    fn a_problem_about_a_whole_file_does_not_claim_a_line() {
        assert_eq!(
            problem(0).to_string(),
            "policy/core/quorum.cedar — something"
        );
        assert_eq!(
            problem(12).to_string(),
            "policy/core/quorum.cedar:12 — something"
        );
    }

    /// A policy's own file quotes its id in the paragraph above it.
    ///
    /// The skip knew TOML's `#` and not Cedar's `//`, so every diagnostic about
    /// a core policy was filed against the header paragraph of its file — and
    /// the paragraph is where the rule is *explained*, which is the line a
    /// reader is least helped by. Both real spellings, because
    /// `policy/core/chain_depth.cedar` opens with `//` prose and
    /// `policy/core/tests/*.toml` with `#`.
    #[test]
    fn a_comment_quoting_a_policy_is_not_where_it_is_declared() {
        let cedar = concat!(
            "// `NOCTUA.md §14.6`: chain.depth is the rule below.\n",
            "//\n",
            "  // and indented prose is prose too, naming chain.depth again\n",
            "@id(\"chain.depth\")\n",
            "forbid (principal, action, resource);\n"
        );
        assert_eq!(
            line_of(cedar, "chain.depth"),
            4,
            "a Cedar comment quoting the policy was read as the declaration"
        );
        let toml = concat!(
            "# the quorum case, name = \"quorum.two\"\n",
            "name = \"quorum.two\"\n"
        );
        assert_eq!(line_of(toml, "name = \"quorum.two\""), 2);
        // And a line that merely *contains* `//` is not a comment: a Cedar
        // policy may hold one in a string, and refusing it would send a reader
        // to nowhere at all.
        let held = "@id(\"a.rule\")\nwhen { resource.at == \"https://x/y\" };\n";
        assert_eq!(line_of(held, "https://x/y"), 2);
    }

    #[test]
    fn a_needle_that_is_not_there_is_line_zero_and_not_line_one() {
        // Line one is a real line, and pointing at it for something absent sends
        // a reader to whatever happens to be written there.
        assert_eq!(line_of("a\nb\nc\n", "d"), 0);
        assert_eq!(line_of("a\nb\nc\n", "c"), 3);
    }
}
