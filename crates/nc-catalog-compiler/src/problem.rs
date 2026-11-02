//! What a bad catalog looks like when a developer reads it.
//!
//! `NOCTUA.md §15.1` asks for diagnostics that **point at the exact location and
//! end with an actionable fix, not a restatement of the failure**. That is three
//! fields and a `Display`, so it is three fields and a `Display` rather than a
//! dependency: `miette` renders beautifully with its `fancy` feature and prints
//! a struct dump without it, and nothing here needs a source snippet.
//!
//! The third field is [`Problem::check`], and it exists because of how the tests
//! that prove these gates fire were defeated. They matched a substring of the
//! message, so a test naming a key passed when a *different* check happened to
//! mention the same key — one of them was satisfied entirely by the ledger's
//! diagnostic and stayed green with the check it named switched off. A stable
//! identity per check is what makes "this test failed for the reason it claims"
//! an assertion rather than a coincidence.

use std::fmt;

/// One thing wrong with the catalog, located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Repository-relative, forward slashes.
    pub file: String,
    /// 1-indexed. `None` when the problem is about the file as a whole.
    pub line: Option<usize>,
    /// What is wrong. One sentence, and it names the offending thing.
    pub what: String,
    /// What to do about it. Never a restatement of `what`.
    pub fix: String,
    /// Which check produced this, as a stable kebab-case name.
    ///
    /// Not shown to a developer — it is what a test asserts on, so that a test
    /// cannot be satisfied by a different check that happens to mention the same
    /// word.
    pub check: &'static str,
}

impl Problem {
    /// A problem about one line of one file.
    pub fn at(
        file: impl Into<String>,
        line: usize,
        what: impl Into<String>,
        fix: impl Into<String>,
        check: &'static str,
    ) -> Self {
        Self {
            file: file.into(),
            line: Some(line),
            what: what.into(),
            fix: fix.into(),
            check,
        }
    }

    /// A problem about a whole file.
    pub fn in_file(
        file: impl Into<String>,
        what: impl Into<String>,
        fix: impl Into<String>,
        check: &'static str,
    ) -> Self {
        Self {
            file: file.into(),
            line: None,
            what: what.into(),
            fix: fix.into(),
            check,
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "{}:{} — {}", self.file, line, self.what),
            None => write!(formatter, "{} — {}", self.file, self.what),
        }
    }
}

/// The line the `nth` occurrence of a needle appears on, 1-indexed, ignoring
/// comments.
///
/// Semantic problems — a duplicate code, an area nothing declares — are found
/// after parsing, when the parser's spans are gone. Searching the source for the
/// offending token is how the location survives that.
///
/// **Two refinements a reviewer paid for.** A comment mentioning a code was
/// reported as the code's declaration, because comments in these registries
/// routinely name the entry below them — so a line whose first non-space
/// character is `#` is skipped. And for a duplicate, the *first* occurrence is
/// the innocent one: `nth` is what lets the caller ask for the second.
///
/// A needle that is not found yields `None` and the problem is reported against
/// the file, which is still better than reporting it against nothing.
#[must_use]
pub fn line_of(text: &str, needle: &str, nth: usize) -> Option<usize> {
    let mut seen = 0;
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if line.contains(needle) {
            seen += 1;
            if seen == nth {
                return Some(index + 1);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Problem, line_of};

    #[test]
    fn a_located_problem_reads_as_file_line_sentence() {
        let problem = Problem::at(
            "catalog/errors.toml",
            12,
            "NC-DB-0001 is declared twice",
            "delete one",
            "duplicate-code",
        );
        assert_eq!(
            problem.to_string(),
            "catalog/errors.toml:12 — NC-DB-0001 is declared twice"
        );
    }

    #[test]
    fn a_problem_about_a_whole_file_omits_the_line() {
        let problem = Problem::in_file(
            "catalog/errors.toml",
            "the file is empty",
            "declare an error",
            "empty-registry",
        );
        assert_eq!(
            problem.to_string(),
            "catalog/errors.toml — the file is empty"
        );
    }

    #[test]
    fn a_comment_naming_the_entry_below_it_is_not_the_entry() {
        // Measured: a plausible comment — "NC-LOG-0001 used to be called
        // NC-LOG-0000; see the ledger." — was reported as the declaration, for a
        // problem sixty lines further down.
        let text = "# NC-DB-0001 used to be NC-DB-0000\ncode = \"NC-DB-0001\"\n";
        assert_eq!(line_of(text, "NC-DB-0001", 1), Some(2));
    }

    #[test]
    fn the_nth_occurrence_is_reachable_because_a_duplicate_is_never_the_first() {
        let text = "code = \"NC-DB-0001\"\nother\ncode = \"NC-DB-0001\"\n";
        assert_eq!(line_of(text, "NC-DB-0001", 1), Some(1));
        assert_eq!(line_of(text, "NC-DB-0001", 2), Some(3));
        assert_eq!(line_of(text, "NC-DB-0001", 3), None);
    }

    #[test]
    fn a_needle_nothing_holds_is_located_nowhere() {
        assert_eq!(line_of("one\ntwo\n", "NC-DB-9999", 1), None);
    }
}
