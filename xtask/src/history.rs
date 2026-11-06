//! The repository's history: what every commit says, and what every clone
//! downloads.
//!
//! `NOCTUA.md §22.5` holds every commit to three rules, and this module judges
//! them over the whole history — every commit reachable from `HEAD`, merges
//! included — because a clone downloads all of it and a release reads all of
//! it:
//!
//! * **a conventional subject** ([`subject`]): the type decides the version
//!   (`NOCTUA.md §22.3`) and the changelog is built from the subjects
//!   (`NOCTUA.md §22.4`), so a subject the release cannot read is a change it
//!   cannot place;
//! * **no agent credit** ([`credits`]), in the subject or the body
//!   (`NOCTUA.md §2.6`);
//! * **no blob above [`LIMIT`]** unless [`ABOVE_THE_LIMIT`] names its path
//!   (`NOCTUA.md §17.8`). Every blob ever reachable, not only the tree at
//!   `HEAD`: a file added and then removed is still in what every clone
//!   downloads.
//!
//! A merge commit is judged like any other. GitHub titles one with the pull
//! request's title and appends ` (#N)`, so a merged history is conventional
//! when every title was, and [`subject`] reads that suffix off. A commit
//! GitHub or the release App authored is judged by the same rules as one a
//! person did: what is judged is what a message says, never who wrote it.
//!
//! # A pull request is judged as its event names it
//!
//! Under `pull_request`, CI checks out a merge commit GitHub makes for the run
//! and puts on no branch, titled `Merge <sha> into <sha>`. So there the step
//! judges what the event names — the history of the pull request's head and of
//! its base, and the pull request's title and body — and never the commit the
//! checkout is at ([`the_pull_request`]). Anywhere else, a push or a run on a
//! developer's machine, it judges everything reachable from `HEAD`.
//!
//! # Git is asked, and nothing it inherits chooses what it reads
//!
//! The history is read through `git` and nothing else: `rev-list` for what is
//! reachable, `cat-file` for each object as git stores it, and `diff-tree -z`
//! for every path a commit puts a blob above the limit at. Plumbing rather than
//! `git log`, because a porcelain's output follows the configuration — a
//! signature check printed into it, a message re-encoded — and plumbing's does
//! not. What [`a_git`] closes before git starts, and why each, is written there.

use std::ffi::{OsStr, OsString};
use std::io::Write as _;
use std::path::Path;

use crate::gates::Finding;

/// The size a blob may reach before its path has to be listed: one mebibyte.
pub(crate) const LIMIT: u64 = 1 << 20;

/// The paths a blob may sit at above [`LIMIT`], each with the most it may
/// reach and why it is there.
///
/// **A ceiling per path**, so a line here admits the file it was written for
/// and not whatever is committed at that path next: an output put in its place
/// is refused at the ceiling as any other blob is at the limit.
pub(crate) const ABOVE_THE_LIMIT: [(&str, u64, &str); 1] = [(
    "xtask/src/check.rs",
    4 << 20,
    "hand-written source, and neither a binary nor a build output: the gate's steps and \
     the tests that hold each of them. A pack stores each version of it as a delta of \
     another, so a clone pays for what a commit changed in it rather than for its size",
)];

/// The types a subject may open with: the ones the version and the changelog
/// read (`NOCTUA.md §22.3`, `NOCTUA.md §22.4`).
pub(crate) const TYPES: [&str; 10] = [
    "feat", "fix", "perf", "refactor", "docs", "test", "chore", "ci", "build", "revert",
];

/// The words an agent or an AI product signs with.
///
/// **Words, compared whole**, so `gpt` is read in `GPT-4o` and `cursor` in
/// `cursoragent@cursor.com`, and neither is read inside a longer word. A name
/// that is also a person's first name is not here: a trailer naming a person of
/// that name would be refused. Such a product is read instead by its site in
/// [`AGENT_SITES`]: in a link, and in a trailer's address.
const AGENT_NAMES: [&str; 22] = [
    "aider",
    "anthropic",
    "bard",
    "chatgpt",
    "claude",
    "codeium",
    "codewhisperer",
    "codex",
    "copilot",
    "cursor",
    "deepseek",
    "gemini",
    "gpt",
    "grok",
    "llama",
    "mistral",
    "openai",
    "openhands",
    "perplexity",
    "qwen",
    "tabnine",
    "windsurf",
];

/// The sites an agent's session or its product page is reached at, each with
/// every name under it.
///
/// **The product's sites and not the vendor's**: a plugin that reaches an AI
/// vendor's interface names the vendor's domain in its manifest, and a commit
/// saying so is no credit. Read in a link, and in a trailer's address, which is
/// where a tool that signs as a product names it.
const AGENT_SITES: [&str; 24] = [
    "aider.chat",
    "aistudio.google.com",
    "all-hands.dev",
    "ampcode.com",
    "bolt.new",
    "chat.deepseek.com",
    "chat.mistral.ai",
    "chat.openai.com",
    "chatgpt.com",
    "claude.ai",
    "claude.com",
    "codeium.com",
    "copilot.microsoft.com",
    "cursor.com",
    "cursor.sh",
    "devin.ai",
    "gemini.google.com",
    "grok.com",
    "jules.google.com",
    "lovable.dev",
    "perplexity.ai",
    "poe.com",
    "v0.dev",
    "windsurf.com",
];

/// The domains of the vendors whose business is agents, each with the agents
/// it makes beside it: what such a vendor's tools sign a trailer from.
///
/// **Read in a trailer's address and nowhere else**, at the domain or under
/// it, and any name there: a tool signs `noreply@` or with another name, and a
/// person who signs from a vendor's own domain is refused with it. A link to
/// one is no credit, since a plugin that reaches the vendor's interface names
/// it. A vendor that makes more than agents, such as GitHub, Google or
/// Microsoft, is not here, so a person's `users.noreply.github.com` address
/// stays a person's.
const AGENT_VENDORS: [&str; 8] = [
    "anthropic.com", // Claude
    "cognition.ai",  // Devin
    "cohere.com",    // Command
    "deepseek.com",  // DeepSeek
    "mistral.ai",    // Le Chat, Codestral
    "moonshot.ai",   // Kimi
    "openai.com",    // ChatGPT, Codex
    "x.ai",          // Grok
];

/// The words a line uses to say how a change was made, before [`BY`].
const MADE: [&str; 7] = [
    "assisted",
    "authored",
    "created",
    "generated",
    "made",
    "produced",
    "written",
];

/// The words between [`MADE`] and what made it.
const BY: [&str; 4] = ["by", "using", "via", "with"];

/// The words a line may put between [`BY`] and the agent it names.
const ARTICLES: [&str; 3] = ["a", "an", "the"];

/// The vendors a line may name between [`BY`] and their agent, which a
/// line such as "generated by GitHub …" names after its maker.
///
/// **Only before an agent's name**, so "made with GitHub Actions" says what
/// built a change and is no credit. A vendor that is an agent's name itself is
/// in [`AGENT_NAMES`] and read there.
const MAKERS: [&str; 9] = [
    "alibaba",
    "amazon",
    "aws",
    "github",
    "google",
    "jetbrains",
    "meta",
    "microsoft",
    "xai",
];

/// The names of [`AGENT_NAMES`] that are an ordinary word of this tree too:
/// read in a trailer, and in a line saying how a change was made only when it
/// is written as a name, with a capital.
///
/// **`cursor` is a terminal's before it is a product's**, and this tree's prose
/// names the terminal's in lower case: "the caret is made with a block cursor"
/// says how a caret is drawn, and the same word written as a name, with a
/// capital, right after "generated with", names the product.
const ALSO_A_WORD: [&str; 1] = ["cursor"];

/// The robot emoji, which a tool puts in front of the line that credits it.
const ROBOT: char = '\u{1F916}';

/// How much of a commit's or a pull request's own text a finding quotes.
const QUOTED: usize = 72;

/// A pull request, as the event that started this run names it.
#[derive(Debug)]
pub(crate) struct PullRequest {
    /// What the merge commit will be titled.
    title: String,
    /// What the merge commit's body will be. Empty when the event's is `null`.
    body: String,
    /// The commit the pull request proposes.
    head: String,
    /// The commit of the branch it targets.
    base: String,
}

/// What reading the history came to.
#[derive(Debug)]
pub(crate) enum History {
    /// This tree has no `.git` of its own, so there is no history to read.
    NoRepository,
    /// The history was read and judged.
    Judged(Judged),
}

/// What the step read, and what it found.
#[derive(Debug)]
pub(crate) struct Judged {
    /// Every rule a commit, a blob or the pull request breaks.
    pub(crate) findings: Vec<Finding>,
    /// How many commits were judged.
    pub(crate) commits: usize,
    /// How many blobs were measured.
    pub(crate) blobs: usize,
    /// How many of those are above [`LIMIT`] at a path [`ABOVE_THE_LIMIT`] lists.
    pub(crate) listed: usize,
    /// Whether a pull request's title and body were judged too.
    pub(crate) pull_request: bool,
}

/// Whether a subject is a conventional one: `type(scope)!: description`.
///
/// The type is one of [`TYPES`], in lower case. The scope is optional and, when
/// written, holds lower-case letters, digits, `-`, `_`, `.` and `/` and nothing
/// else — no space and no capital. The `!` is optional. One space follows the
/// colon, and the description after it is not empty, does not end in
/// whitespace, and does not end with a period. A trailing ` (#<digits>)` is
/// GitHub's, appended to a merge commit's title, and is read off before the
/// rest is judged. The imperative mood is the author's to keep: no rule here
/// reads it.
///
/// **No character a reader cannot see** ([`nc_schema::unseen`]) — a control, a
/// bidirectional mark, a zero-width one — because a subject is what the
/// changelog and the release notes print (`NOCTUA.md §22.4`), and such a
/// character makes an entry read otherwise than it is.
///
/// # Errors
///
/// Returns the first rule the subject breaks.
pub(crate) fn subject(line: &str) -> Result<(), String> {
    if line.chars().any(nc_schema::unseen) {
        return Err(
            "it holds a character a reader cannot see: a control, a bidirectional mark or a \
             zero-width one"
                .to_owned(),
        );
    }
    let line = without_the_pull_request(line);
    let Some((head, after)) = line.split_once(':') else {
        return Err(format!(
            "it has no `:`, so no type opens it. A subject opens with one of {}",
            TYPES.join(", ")
        ));
    };
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        None => (head, None),
        Some((kind, rest)) => {
            let Some(scope) = rest.strip_suffix(')') else {
                return Err(
                    "its scope is not closed by `)` just before the `:`, or the `!` after it"
                        .to_owned(),
                );
            };
            (kind, Some(scope))
        }
    };
    if !TYPES.contains(&kind) {
        return Err(format!(
            "{} is not a type. A subject opens with one of {}, in lower case",
            quoted(kind),
            TYPES.join(", ")
        ));
    }
    if let Some(scope) = scope {
        if scope.is_empty() {
            return Err(
                "its scope is empty: name what the change is about, or leave the parentheses out"
                    .to_owned(),
            );
        }
        if let Some(wrong) = scope.chars().find(|character| !in_a_scope(*character)) {
            return Err(format!(
                "its scope holds {}, and a scope is lower-case letters, digits, `-`, `_`, `.` \
                 and `/`",
                quoted(&wrong.to_string())
            ));
        }
    }
    let Some(description) = after.strip_prefix(' ') else {
        return Err("one space follows the colon".to_owned());
    };
    if description.is_empty() {
        return Err("nothing follows the colon: a subject says what the change does".to_owned());
    }
    if description.starts_with(char::is_whitespace) {
        return Err("one space follows the colon, and only one".to_owned());
    }
    if description.ends_with(char::is_whitespace) {
        return Err("it ends in whitespace".to_owned());
    }
    if description.ends_with('.') {
        return Err("it ends with a period".to_owned());
    }
    Ok(())
}

/// Whether a character may stand in a scope.
fn in_a_scope(character: char) -> bool {
    character.is_ascii_lowercase()
        || character.is_ascii_digit()
        || matches!(character, '-' | '_' | '.' | '/')
}

/// A subject without the ` (#<digits>)` GitHub appends to a merge commit's
/// title.
///
/// **Only digits between `(#` and `)`**, so `(#)` and `(#12a)` stay part of the
/// description and are judged with it.
fn without_the_pull_request(line: &str) -> &str {
    let Some(opened) = line.rfind(" (#") else {
        return line;
    };
    let Some(number) = line[opened + 3..].strip_suffix(')') else {
        return line;
    };
    if number.is_empty() || !number.chars().all(|character| character.is_ascii_digit()) {
        return line;
    }
    &line[..opened]
}

/// Every line of `text` that credits an agent: its number, from 1, and how.
///
/// Four shapes, which are the ones tools write (`NOCTUA.md §2.6`):
///
/// * **a trailer naming an agent or a bot** — a line `<Key>-by:` or
///   `<Key>-with:` (`Co-Authored-By:`, `Assisted-by:`, `Generated-by:` …) whose
///   value names one of [`AGENT_NAMES`], holds an address under one of
///   [`AGENT_SITES`] or [`AGENT_VENDORS`], or names a `[bot]`, read as git
///   reads a trailer ([`a_trailer`]);
/// * **a line saying how it was made** — one of [`MADE`], one of [`BY`], and
///   right after them, past an article and a maker's name, an agent's
///   ([`an_agent_opens`]);
/// * **a link to an agent's site** — a name under one of [`AGENT_SITES`],
///   after `://` or before a `/`, in any line, a trailer of another key
///   included;
/// * **the robot emoji.**
///
/// Naming `AGENTS.md` is none of them: that is a file in this
/// repository.
pub(crate) fn credits(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(at, line)| a_credit(line, &lines[at + 1..]).map(|how| (at + 1, how)))
        .collect()
}

/// How one line credits an agent, if it does, with the lines `below` it.
///
/// **A trailer's value goes on into every line below it that opens with a
/// space or a tab**, because git folds such lines into the value: a key alone
/// on its line, with the agent's name and address under it, is a trailer that
/// names an agent to git and to every tool reading trailers through it.
fn a_credit(line: &str, below: &[&str]) -> Option<String> {
    let lower = line.to_lowercase();
    if let Some((key, value)) = a_trailer(&lower) {
        let folded = below
            .iter()
            .take_while(|next| next.starts_with([' ', '\t']))
            .fold(value.to_owned(), |mut whole, next| {
                whole.push(' ');
                whole.push_str(&next.to_lowercase());
                whole
            });
        if names_an_agent(&folded) {
            return Some(format!("a `{key}:` trailer naming an agent or a bot"));
        }
    }
    let written = words_of(line);
    let words: Vec<String> = written.iter().map(|word| word.to_lowercase()).collect();
    if let Some(at) = (0..words.len().saturating_sub(1)).find(|at| {
        MADE.contains(&words[*at].as_str())
            && BY.contains(&words[at + 1].as_str())
            && an_agent_opens(&written[at + 2..])
    }) {
        let (made, by) = (&words[at], &words[at + 1]);
        return Some(format!("a line saying it was {made} {by} an agent"));
    }
    if let Some(site) = a_linked_site(&lower) {
        return Some(format!("a link to {site}, an agent's site"));
    }
    if line.contains(ROBOT) {
        return Some("the robot emoji".to_owned());
    }
    None
}

/// Whether `written`, a line's words as it writes them, open with an agent's
/// name: what a line saying how a change was made names right after [`BY`].
///
/// **Right after, and not anywhere later**, so "the caret is made with a
/// blinking block, as the terminal's is" says how a caret is drawn whatever the
/// rest of its line names. An article may stand first, and then one of
/// [`MAKERS`] before the agent it makes.
fn an_agent_opens(written: &[&str]) -> bool {
    let named = match written.first() {
        Some(first) if ARTICLES.contains(&first.to_lowercase().as_str()) => &written[1..],
        _ => written,
    };
    match named {
        [first, ..] if an_agent(first) => true,
        [maker, then, ..] if MAKERS.contains(&maker.to_lowercase().as_str()) => an_agent(then),
        _ => false,
    }
}

/// Whether one word, as a line writes it, is an agent's name: one of
/// [`AGENT_NAMES`], and one of [`ALSO_A_WORD`] only with a capital.
fn an_agent(written: &str) -> bool {
    let word = written.to_lowercase();
    AGENT_NAMES.contains(&word.as_str())
        && (!ALSO_A_WORD.contains(&word.as_str()) || written.starts_with(char::is_uppercase))
}

/// The key and the value of a trailer that says who took part, in a line
/// already in lower case.
///
/// The key is letters, digits and `-` and ends `-by` or `-with`: the keys that
/// say who took part. A trailer of another key — a tool's link to the session
/// that made a change — is read by the link it holds, and a line that only
/// contains a colon is prose.
///
/// **Spaces and tabs between the key and the colon are the key's**, as git's
/// trailer parser reads them: `Co-Authored-By : …` is the trailer
/// `Co-Authored-By: …` to git, and so to this step.
fn a_trailer(lower: &str) -> Option<(&str, &str)> {
    let (key, value) = lower.trim_start().split_once(':')?;
    let key = key.trim_end_matches([' ', '\t']);
    let shaped = !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    (shaped && (key.ends_with("-by") || key.ends_with("-with"))).then_some((key, value))
}

/// Whether a trailer's value names an agent or a bot, by its name or by its
/// address.
///
/// **The address is read by its host**: the name after each `@`, held to
/// [`AGENT_SITES`] as a link's is, so a product left out of [`AGENT_NAMES`]
/// for sharing a person's first name is still read when it signs from its own
/// site. And to [`AGENT_VENDORS`], which no link is held to, so a tool that
/// signs from its maker's domain is read whatever its maker is called.
fn names_an_agent(value: &str) -> bool {
    value.contains("[bot]")
        || words_of(value)
            .iter()
            .any(|word| AGENT_NAMES.contains(word))
        || value.split('@').skip(1).any(|after| {
            let host = after
                .split(|character| !in_a_host(character))
                .next()
                .unwrap_or_default();
            the_site_of(host, &AGENT_SITES)
                .or_else(|| the_site_of(host, &AGENT_VENDORS))
                .is_some()
        })
}

/// A text's words: its runs of letters and digits, in order.
fn words_of(text: &str) -> Vec<&str> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

/// The first of [`AGENT_SITES`] a line links to, in a line already in lower
/// case.
///
/// **A link and not a mention**: the name is read after `://`, or before a
/// `/` — a link written without its scheme — and a name in a sentence with
/// neither is left alone.
fn a_linked_site(lower: &str) -> Option<&str> {
    let mut rest = lower;
    while let Some(start) = rest.find(in_a_host) {
        let (gap, from) = rest.split_at(start);
        let end = from
            .find(|character: char| !in_a_host(character))
            .unwrap_or(from.len());
        let (name, after) = from.split_at(end);
        let linked = gap.ends_with("://") || after.starts_with('/');
        if linked && let Some(site) = the_site_of(name, &AGENT_SITES) {
            return Some(site);
        }
        rest = after;
    }
    None
}

/// Whether a character may stand in a host's name.
fn in_a_host(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
}

/// The one of `sites` a host, in lower case, is or is under.
fn the_site_of<'a>(host: &str, sites: &[&'a str]) -> Option<&'a str> {
    let host = host.trim_end_matches('.');
    sites
        .iter()
        .find(|site| host == **site || host.ends_with(&format!(".{site}")))
        .copied()
}

/// A commit's or a pull request's own text, made safe to print in a finding:
/// a message, a title, a body, or the path a commit put a blob at.
///
/// **Every character a reader cannot see is written as its escape, and the
/// text is cut short.** What a commit or a pull request says, and where it
/// puts a file, is written by whoever opened it, a fork's author included, and
/// a finding prints it into CI's log: a control character there moves the
/// terminal, and a line the runner reads as a workflow command is one it
/// obeys. A finding's line opens with the place it names — a commit's or a
/// blob's hash, or the pull request's title or body — so a quotation never
/// opens one, and with the controls escaped it cannot start another.
fn quoted(text: &str) -> String {
    let shown = shown(text);
    if shown.chars().count() <= QUOTED {
        return format!("`{shown}`");
    }
    let cut: String = shown.chars().take(QUOTED).collect();
    format!("`{cut}…`")
}

/// `text` with every character a reader cannot see ([`nc_schema::unseen`])
/// written as its escape, `\u{…}`.
///
/// **Escaped, and not dropped**, so what a finding quotes reads as what it
/// is: a path with a carriage return after it is shown as one, and not as the
/// path it would otherwise read as.
fn shown(text: &str) -> String {
    text.chars()
        .map(|character| {
            if nc_schema::unseen(character) {
                character.escape_unicode().to_string()
            } else {
                character.to_string()
            }
        })
        .collect()
}

/// The pull request this run is for, when `GITHUB_EVENT_NAME` says it is one.
///
/// `name` and `path` are the values of `GITHUB_EVENT_NAME` and
/// `GITHUB_EVENT_PATH`, handed in so a test can give either without touching
/// the process's environment. Any event but `pull_request` is `None`: a push,
/// or a run no event started, judges what `HEAD` reaches.
///
/// **Fail closed.** Under `pull_request` an event that cannot be read, or that
/// names no head, base, title or body, is a refusal rather than a run that
/// judges `HEAD` — which there is the merge commit made for the run — or that
/// judges nothing. Each commit is held to the shape of a hash before git is
/// handed it, so a value the event carries never reaches git as an option.
///
/// # Errors
///
/// Returns what is missing from the event, or why it could not be read.
pub(crate) fn the_pull_request(
    name: Option<&OsStr>,
    path: Option<&OsStr>,
) -> Result<Option<PullRequest>, String> {
    if name != Some(OsStr::new("pull_request")) {
        return Ok(None);
    }
    let path = path.ok_or_else(|| {
        "GITHUB_EVENT_NAME is `pull_request` and GITHUB_EVENT_PATH is not set, so the pull \
         request's commits, title and body could not be read. Unset GITHUB_EVENT_NAME to \
         judge what HEAD reaches"
            .to_owned()
    })?;
    let text = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "the event at GITHUB_EVENT_PATH could not be read, so the pull request was not \
             judged: {error}"
        )
    })?;
    let event: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
        format!("the event at GITHUB_EVENT_PATH is not JSON, so the pull request was not judged: {error}")
    })?;
    let pull_request = event.get("pull_request").ok_or_else(|| {
        "the event at GITHUB_EVENT_PATH holds no `pull_request`, and GITHUB_EVENT_NAME says it \
         is one"
            .to_owned()
    })?;
    let title = pull_request
        .get("title")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "the pull request in the event has no title".to_owned())?;
    let body = match pull_request.get("body") {
        Some(serde_json::Value::Null) => "",
        Some(serde_json::Value::String(body)) => body.as_str(),
        _ => return Err("the pull request in the event has no body, not even `null`".to_owned()),
    };
    Ok(Some(PullRequest {
        title: title.to_owned(),
        body: body.to_owned(),
        head: a_commit_of(pull_request, "head")?,
        base: a_commit_of(pull_request, "base")?,
    }))
}

/// The commit an event's `head` or `base` names, held to the shape of a hash.
fn a_commit_of(pull_request: &serde_json::Value, side: &str) -> Result<String, String> {
    let sha = pull_request
        .get(side)
        .and_then(|named| named.get("sha"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("the pull request in the event names no `{side}.sha`"))?;
    if !matches!(sha.len(), 40 | 64)
        || !sha
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
    {
        return Err(format!(
            "the pull request's `{side}.sha` is {}, which is not a commit's hash",
            quoted(sha)
        ));
    }
    Ok(sha.to_owned())
}

/// The history of `root`, judged: against `pull_request` when one is given,
/// and against what `HEAD` reaches when not.
///
/// # Errors
///
/// Returns why the history could not be read, which is a refusal, never a pass.
pub(crate) fn judged(root: &Path, pull_request: Option<&PullRequest>) -> Result<History, String> {
    judged_against(root, pull_request, &ABOVE_THE_LIMIT)
}

/// The history judged as [`judged`] judges it, with the list of paths allowed
/// above [`LIMIT`] handed in, so a test can give a scratch repository a list of
/// its own.
///
/// **This process's whole environment is handed on**, because what
/// [`inheriting`] removes is what it is handed: a variable of git's own left
/// out of the list would reach git through the environment a child inherits.
fn judged_against(
    root: &Path,
    pull_request: Option<&PullRequest>,
    listed: &[(&str, u64, &str)],
) -> Result<History, String> {
    let inherited: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    judged_inheriting(root, pull_request, listed, &inherited)
}

/// The repository the step reads, and the environment this process would
/// hand the git it starts.
struct Reading<'a> {
    /// The tree whose own repository is read.
    root: &'a Path,
    /// This process's environment, which [`a_git`] hands git all of but
    /// git's own variables ([`inheriting`]).
    inherited: &'a [(OsString, OsString)],
}

/// The history judged as [`judged_against`] judges it, with this process's
/// environment handed in as `inherited`, so a test can hand git a variable no
/// shell exported.
fn judged_inheriting(
    root: &Path,
    pull_request: Option<&PullRequest>,
    listed: &[(&str, u64, &str)],
    inherited: &[(OsString, OsString)],
) -> Result<History, String> {
    // **Asked before git is started**, so a tree with no `.git` of its own —
    // a tarball — stands aside by name. Git is handed this tree's `.git` and
    // searches nowhere else ([`a_git`]), so there it would only refuse.
    let here = root
        .join(".git")
        .try_exists()
        .map_err(|error| format!("whether this tree has a .git could not be read: {error}"))?;
    if !here {
        return Ok(History::NoRepository);
    }
    let reading = Reading { root, inherited };
    not_shallow(&reading)?;
    not_grafted(&reading)?;
    let starts: Vec<&str> = match pull_request {
        Some(pull_request) => vec![pull_request.head.as_str(), pull_request.base.as_str()],
        None => vec!["HEAD"],
    };
    let mut findings = Vec::new();
    let commits = judge_the_commits(&reading, &starts, &mut findings)?;
    let (blobs, above) = measure_the_blobs(&reading, &starts, &commits, listed, &mut findings)?;
    if let Some(pull_request) = pull_request {
        judge_the_pull_request(pull_request, &mut findings);
    }
    Ok(History::Judged(Judged {
        findings,
        commits: commits.len(),
        blobs,
        listed: above,
        pull_request: pull_request.is_some(),
    }))
}

/// Refuses a shallow clone.
///
/// **Refused, not judged.** A shallow clone holds the history down to where it
/// was cut, so every commit below the cut would pass by being absent.
fn not_shallow(reading: &Reading) -> Result<(), String> {
    let shallow = git(reading, &["rev-parse", "--is-shallow-repository"], None)?;
    match String::from_utf8_lossy(&shallow).trim() {
        "false" => Ok(()),
        "true" => Err(
            "this clone is shallow, so the history below where it was cut is not here to \
             judge. Fetch all of it: `git fetch --unshallow`, or a checkout with \
             `fetch-depth: 0`"
                .to_owned(),
        ),
        other => Err(format!(
            "git answered {} when asked whether this clone is shallow",
            quoted(other)
        )),
    }
}

/// Refuses a clone that grafts its history.
///
/// **Refused, not judged**, as a shallow clone is: a graft file gives a commit
/// other parents than the ones it holds, so every commit a graft cuts away
/// would pass by being absent from what git lists. `--no-replace-objects` does
/// not reach a graft, and git asked whether such a clone is shallow says it is
/// not, so the file itself is asked for, where git would read it.
fn not_grafted(reading: &Reading) -> Result<(), String> {
    let named = git(reading, &["rev-parse", "--git-path", "info/grafts"], None)?;
    let named = String::from_utf8(named)
        .map_err(|_| "git named its graft file in something that is not UTF-8".to_owned())?;
    let grafted = reading
        .root
        .join(named.trim_end_matches(['\n', '\r']))
        .try_exists()
        .map_err(|error| {
            format!("whether this clone grafts its history could not be read: {error}")
        })?;
    if grafted {
        return Err(
            "this clone grafts its history: its graft file shows git a commit with other \
             parents than it holds, so the history a graft cuts away is not here to judge. \
             Turn the grafts into replacement refs, which this step reads past: `git replace \
             --convert-graft-file`"
                .to_owned(),
        );
    }
    Ok(())
}

/// Judges every commit reachable from `starts` into `findings`, and answers
/// each one's hash, in the order git listed them.
fn judge_the_commits(
    reading: &Reading,
    starts: &[&str],
    findings: &mut Vec<Finding>,
) -> Result<Vec<String>, String> {
    let listed = String::from_utf8(git(reading, &[&["rev-list"][..], starts].concat(), None)?)
        .map_err(|_| "git listed the commits in something that is not UTF-8".to_owned())?;
    let hashes: Vec<&str> = listed.lines().collect();
    let mut asked = hashes.join("\n").into_bytes();
    asked.push(b'\n');
    let answer = git(reading, &["cat-file", "--batch"], Some(asked))?;
    let commits = the_objects(&answer, &hashes, "commit")?;
    for (hash, raw) in &commits {
        judge_a_commit(hash, raw, findings);
    }
    Ok(commits.into_iter().map(|(hash, _)| hash).collect())
}

/// Measures every blob reachable from `starts` into `findings`.
///
/// Answers how many blobs there were, and how many of the ones above [`LIMIT`]
/// sit only at paths `listed` allows them at. `commits` are every commit
/// that `starts` reaches, and what the paths are read from.
fn measure_the_blobs(
    reading: &Reading,
    starts: &[&str],
    commits: &[String],
    listed: &[(&str, u64, &str)],
    findings: &mut Vec<Finding>,
) -> Result<(usize, usize), String> {
    // **`--missing=print`, so an object this clone lacks is named rather than
    // fetched**: a partial clone holds the commits and not every blob, and a
    // blob that is not here is one nobody measured. **The names alone**
    // (`--no-object-names`): the path `rev-list` prints beside an object is
    // the first it reached the object by, cut at a newline, so it is neither
    // every path the object sits at nor, byte for byte, any one of them.
    let objects = git(
        reading,
        &[
            &[
                "rev-list",
                "--objects",
                "--no-object-names",
                "--missing=print",
            ][..],
            starts,
        ]
        .concat(),
        None,
    )?;
    let named: Vec<&[u8]> = objects
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    if let Some(missing) = named.iter().find(|line| line.first() == Some(&b'?')) {
        return Err(format!(
            "the history names {}, and this clone does not hold it — a partial clone. The step \
             measures every blob ever reachable, so it needs a clone that holds all of them",
            quoted(&String::from_utf8_lossy(&missing[1..]))
        ));
    }
    let sizes = git(
        reading,
        &[
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize)",
        ],
        Some(objects.clone()),
    )?;
    let (blobs, above) = the_blobs(&sizes, &named)?;
    if above.is_empty() {
        return Ok((blobs, 0));
    }
    // **Every commit against its first parent, and a root against nothing**:
    // a path a blob sits at in some commit is either its first parent's, and
    // so placed further down, or one this comparison adds it at. `-z` writes
    // each path as it is, and the rest is closed so the answer has one shape.
    let mut asked = commits.join("\n").into_bytes();
    asked.push(b'\n');
    let changes = git(
        reading,
        &[
            "diff-tree",
            "--stdin",
            "-r",
            "--root",
            "--diff-merges=first-parent",
            "--no-renames",
            "--no-commit-id",
            "--no-abbrev",
            "-z",
        ],
        Some(asked),
    )?;
    let placed = placed(&changes, &above)?;
    Ok((blobs, judge_the_places(&placed, listed, findings)))
}

/// Judges a pull request's title and body into `findings`.
///
/// The title is what GitHub titles the merge commit with, so it is held to
/// both rules a subject is; the body becomes that commit's body, and is held
/// to the one a body is.
fn judge_the_pull_request(pull_request: &PullRequest, findings: &mut Vec<Finding>) {
    /// What a finding about the pull request says to do, first.
    const EDIT: &str = "Edit the pull request: what it says becomes the merge commit's message";
    if let Err(why) = subject(&pull_request.title) {
        findings.push(Finding {
            file: "the pull request's title".to_owned(),
            line: 1,
            what: format!(
                "the title {} is not a conventional subject: {why}",
                quoted(&pull_request.title)
            ),
            fix: format!("{EDIT}, and its title is the subject the history keeps"),
        });
    }
    for (place, text) in [
        ("the pull request's title", pull_request.title.as_str()),
        ("the pull request's body", pull_request.body.as_str()),
    ] {
        for (line, how) in credits(text) {
            findings.push(credited(place, line, &how, EDIT));
        }
    }
}

/// A finding for a line that credits an agent.
fn credited(place: &str, line: usize, how: &str, fix: &str) -> Finding {
    Finding {
        file: place.to_owned(),
        line,
        what: format!("this line credits an agent: {how}"),
        fix: format!(
            "{fix}. Nothing written here credits an agent (`NOCTUA.md §2.6`): the author of a \
             change is the person who asked for it"
        ),
    }
}

/// Every object of `answer`, as `git cat-file --batch` answers `asked`: each
/// hash with its raw contents, in the order asked.
///
/// **Exactly what was asked, in order, of the kind asked.** A reader that
/// stopped part way through the answer would judge the commits before the
/// place it stopped and report the rest as clean, so the answer is held to the
/// list it answers: every hash, once, with nothing left over.
fn the_objects<'a>(
    answer: &'a [u8],
    asked: &[&str],
    kind: &str,
) -> Result<Vec<(String, &'a [u8])>, String> {
    let mut read = Vec::with_capacity(asked.len());
    let mut rest = answer;
    for hash in asked {
        let end = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| format!("git's answer stopped before {}", quoted(hash)))?;
        let header = String::from_utf8_lossy(&rest[..end]);
        let mut fields = header.split(' ');
        let (Some(named), Some(found), Some(size), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(format!(
                "git answered {} for {}, which is not an object it holds",
                quoted(&header),
                quoted(hash)
            ));
        };
        if named != *hash || found != kind {
            return Err(format!(
                "git answered {} when asked for the {kind} {}",
                quoted(&header),
                quoted(hash)
            ));
        }
        let size: usize = size
            .parse()
            .map_err(|_| format!("git gave {} a size of {}", quoted(hash), quoted(size)))?;
        let body = &rest[end + 1..];
        if body.len() < size + 1 || body[size] != b'\n' {
            return Err(format!("git's answer for {} is cut short", quoted(hash)));
        }
        read.push(((*hash).to_owned(), &body[..size]));
        rest = &body[size + 1..];
    }
    if !rest.is_empty() {
        return Err(format!(
            "git answered more than the {} object(s) it was asked for",
            asked.len()
        ));
    }
    Ok(read)
}

/// Judges one commit's message, as git stores it, into `findings`.
fn judge_a_commit(hash: &str, raw: &[u8], findings: &mut Vec<Finding>) {
    let place = format!("commit {}", &hash[..hash.len().min(12)]);
    let (headers, message) = raw
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .map_or((raw, &[][..]), |at| (&raw[..at], &raw[at + 2..]));
    let declared = headers
        .split(|byte| *byte == b'\n')
        .find_map(|line| line.strip_prefix(b"encoding "))
        .map(String::from_utf8_lossy);
    if let Some(declared) = declared
        && !declared.eq_ignore_ascii_case("utf-8")
        && !declared.eq_ignore_ascii_case("utf8")
    {
        findings.push(Finding {
            file: place,
            line: 1,
            what: format!(
                "its message is declared as {}, and every message here is UTF-8",
                quoted(&declared)
            ),
            fix: "Write the message again in UTF-8, before the commit reaches a branch others \
                  share; a shared branch's history is never rewritten"
                .to_owned(),
        });
        return;
    }
    let Ok(message) = std::str::from_utf8(message) else {
        findings.push(Finding {
            file: place,
            line: 1,
            what: "its message is not UTF-8, so nothing in it could be read".to_owned(),
            fix: "Write the message again in UTF-8, before the commit reaches a branch others \
                  share; a shared branch's history is never rewritten"
                .to_owned(),
        });
        return;
    };
    let mut lines = message.split('\n');
    let first = lines.next().unwrap_or_default();
    let reword = "Reword the commit before it reaches a branch others share (`git commit \
                  --amend`, or `git rebase -i` and `reword`); a shared branch's history is \
                  never rewritten";
    if let Err(why) = subject(first) {
        findings.push(Finding {
            file: place.clone(),
            line: 1,
            what: format!(
                "the subject {} is not a conventional one: {why}",
                quoted(first)
            ),
            fix: reword.to_owned(),
        });
    }
    if lines.next().is_some_and(|second| !second.trim().is_empty()) {
        findings.push(Finding {
            file: place.clone(),
            line: 2,
            what: "the line after the subject is not blank, so the subject runs on into it"
                .to_owned(),
            fix: reword.to_owned(),
        });
    }
    for (line, how) in credits(message) {
        findings.push(credited(
            &place,
            line,
            &how,
            "Reword the commit before it reaches a branch others share",
        ));
    }
}

/// A blob above [`LIMIT`], by its hash, with its size.
type Above = (String, u64);

/// A blob above [`LIMIT`], with every path a commit puts it at.
type Placed = (Above, Vec<Vec<u8>>);

/// Every object `git cat-file --batch-check` measured, read against `asked`,
/// the objects `rev-list` named: how many were blobs, and each blob above
/// [`LIMIT`].
///
/// **Every object asked, once, in order, by its own name.** An answer cut
/// short would be fewer blobs measured, and one naming another object would
/// measure the wrong one, so each is a refusal.
fn the_blobs(answer: &[u8], asked: &[&[u8]]) -> Result<(usize, Vec<Above>), String> {
    let text = String::from_utf8_lossy(answer);
    let mut answered = 0_usize;
    let mut blobs = 0_usize;
    let mut above = Vec::new();
    for line in text.split_terminator('\n') {
        let Some(hash) = asked.get(answered) else {
            return Err(format!(
                "git measured more than the {} object(s) the history names",
                asked.len()
            ));
        };
        answered += 1;
        let mut fields = line.split(' ');
        let (Some(named), Some(kind), Some(size), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(format!("git measured an object as {}", quoted(line)));
        };
        if named.as_bytes() != *hash {
            return Err(format!(
                "git measured {} when asked for {}",
                quoted(named),
                quoted(&String::from_utf8_lossy(hash))
            ));
        }
        if kind != "blob" {
            continue;
        }
        blobs += 1;
        let size: u64 = size
            .parse()
            .map_err(|_| format!("git gave a blob the size {}", quoted(size)))?;
        if size <= LIMIT {
            continue;
        }
        above.push((named.to_owned(), size));
    }
    if answered != asked.len() {
        return Err(format!(
            "git measured {answered} object(s) of the {} the history names, so a blob it did \
             not measure is one this step did not judge",
            asked.len()
        ));
    }
    Ok((blobs, above))
}

/// Every path each blob of `above` sits at, as `git diff-tree --stdin -z`
/// answers for every commit: each path byte for byte, the blobs in the order
/// given and each one's paths in order.
///
/// **A path is read from a commit, and not from the blob**: the same bytes sit
/// at as many paths as commits put them at, and a path holds whatever its
/// author wrote — a space at either end, a carriage return, a newline. A
/// record that is not the shape asked for, and a blob no commit places, are
/// refusals: either would leave a path unjudged.
fn placed(changes: &[u8], above: &[Above]) -> Result<Vec<Placed>, String> {
    let mut paths: std::collections::BTreeMap<&str, std::collections::BTreeSet<&[u8]>> = above
        .iter()
        .map(|(hash, _)| (hash.as_str(), std::collections::BTreeSet::new()))
        .collect();
    if changes.last().is_some_and(|byte| *byte != 0) {
        return Err("git's list of changes stops part way through a path".to_owned());
    }
    let mut records = changes.split(|byte| *byte == 0);
    while let Some(header) = records.next() {
        if header.is_empty() && records.clone().next().is_none() {
            break;
        }
        let header = String::from_utf8_lossy(header);
        let fields: Vec<&str> = header.split(' ').collect();
        let (true, [_, _, _, written, "A" | "D" | "M" | "T"], Some(path @ [_, ..])) =
            (header.starts_with(':'), fields.as_slice(), records.next())
        else {
            return Err(format!(
                "git described a change as {}, which is not one path added, removed or changed",
                quoted(&header)
            ));
        };
        if let Some(sits) = paths.get_mut(*written) {
            sits.insert(path);
        }
    }
    above
        .iter()
        .map(|(hash, size)| {
            let sits = paths.remove(hash.as_str()).unwrap_or_default();
            if sits.is_empty() {
                return Err(format!(
                    "the history holds the blob {} of {size} byte(s), and no commit puts it at a \
                     path, so the path it sits at was not judged",
                    quoted(hash)
                ));
            }
            Ok((
                (hash.clone(), *size),
                sits.into_iter().map(<[u8]>::to_vec).collect(),
            ))
        })
        .collect()
}

/// Judges every path a blob above [`LIMIT`] sits at against `listed`
/// into `findings`, and answers how many of the blobs sit only at paths
/// that allow them.
///
/// **A path is listed when its bytes are the listed path's**, so one with a
/// space at either end, or a carriage return or a newline after it, is another
/// path and held to the limit. **Every path the blob sits at**, since a line
/// in `listed` admits the file at its path and not the same bytes elsewhere.
/// **A finding opens with the blob's hash and quotes the path** ([`quoted`]): a
/// path is what a commit's author wrote, as a message is.
fn judge_the_places(
    placed: &[Placed],
    listed: &[(&str, u64, &str)],
    findings: &mut Vec<Finding>,
) -> usize {
    let mut allowed = 0_usize;
    for ((hash, size), sits) in placed {
        let size = *size;
        let place = format!("blob {}", &hash[..hash.len().min(12)]);
        let before = findings.len();
        for path in sits {
            let at = quoted(&String::from_utf8_lossy(path));
            match listed
                .iter()
                .find(|(named, _, _)| named.as_bytes() == path.as_slice())
            {
                Some((_, ceiling, _)) if size <= *ceiling => {}
                Some((_, ceiling, _)) => findings.push(Finding {
                    file: place.clone(),
                    line: 1,
                    what: format!(
                        "a blob of {size} byte(s) at {at}, above the {ceiling} its line in \
                         `ABOVE_THE_LIMIT` allows"
                    ),
                    fix: "A file that outgrew its line is split, or its line is raised with the \
                          reason it needs the room. Either way, a blob already in a shared \
                          branch's history stays there: history is never rewritten"
                        .to_owned(),
                }),
                None => findings.push(Finding {
                    file: place.clone(),
                    line: 1,
                    what: format!(
                        "a blob of {size} byte(s) at {at}, above the limit of {LIMIT} — and \
                         every clone downloads it, even where a later commit removed it"
                    ),
                    fix: "A binary or a build output is a release asset and never a commit \
                          (`NOCTUA.md §17.8`): take it out of the commits before they reach a \
                          branch others share. Source that has to be this large gets a line in \
                          `ABOVE_THE_LIMIT` in `xtask/src/history.rs`, with its reason"
                        .to_owned(),
                }),
            }
        }
        if findings.len() == before {
            allowed += 1;
        }
    }
    allowed
}

/// The git [`git`] starts over the repository `reading` names, before it is
/// told what to do.
///
/// **What the child inherits cannot choose what it reads.** No variable of
/// git's own reaches it ([`inheriting`]). Then:
///
/// * `GIT_DIR` is `.git`, which git reads in the root `-C` takes it to, so git
///   is handed this tree's own repository and searches for none. Searching, it
///   would walk up from a `.git` that holds no repository to one that holds
///   the tree; a list of directories to stop at is no guard, since git splits
///   one at the path separator a directory's name may hold, and drops what is
///   not absolute;
/// * `--no-replace-objects`, so a replacement ref cannot show git another
///   commit in a judged one's place;
/// * `GIT_NO_LAZY_FETCH`, so a clone missing an object fails rather than
///   fetching it, and the step stays on this machine with `--offline` or
///   without.
fn a_git(reading: &Reading) -> std::process::Command {
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let mut command = std::process::Command::new("git");
    inheriting(&mut command, reading.inherited);
    command
        .env("GIT_DIR", ".git")
        .env("GIT_NO_LAZY_FETCH", "1")
        .arg("-C")
        .arg(reading.root)
        .arg("--no-replace-objects");
    command
}

/// Hands `command` every variable of `inherited` as it is, but removes git's
/// own: each whose name begins `GIT_`, in any case.
///
/// **Every one of git's goes**: `GIT_DIR`, `GIT_INDEX_FILE` and
/// `GIT_OBJECT_DIRECTORY`, which a hook running this gate is handed, would
/// point git at another repository or other objects, `GIT_COMMON_DIR` at
/// another's refs, `GIT_GRAFT_FILE` at grafts of its own, and
/// `GIT_CONFIG_PARAMETERS` would hand it configuration nobody wrote here.
/// **In any case**, because Windows reads a variable's name without its case,
/// so git there reads `git_dir` as `GIT_DIR`. **Each named on the command**,
/// removed or set, so what git is handed is what a test reads back from it,
/// and a variable a test hands in reaches git as one inherited would.
///
/// **And every one of `gh`'s goes too** (`crate::release::ghs_own`): the
/// release reads its history through this reader while it holds the App's
/// token in its environment, and no child but `gh` is handed that.
fn inheriting(command: &mut std::process::Command, inherited: &[(OsString, OsString)]) {
    for (name, value) in inherited {
        if name
            .as_encoded_bytes()
            .get(..4)
            .is_some_and(|start| start.eq_ignore_ascii_case(b"GIT_"))
            || crate::release::ghs_own(name)
        {
            command.env_remove(name);
        } else {
            command.env(name, value);
        }
    }
}

/// Runs git over the repository `reading` names, as [`a_git`] starts it, and
/// answers what it printed.
///
/// **Its output is read, never printed**, and what it writes to standard
/// error is returned in the refusal with every character a reader cannot see
/// escaped ([`shown`]), since git names in it what it could not read — a path
/// a commit's author wrote among them. Standard input is written from a thread
/// of its own, so a long answer filling the pipe cannot wait on a question
/// that is still being written.
fn git(reading: &Reading, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut command = a_git(reading);
    command
        .args(args)
        .stdin(if input.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        format!("git could not be started, and the history is read through it: {error}")
    })?;
    let writer = match (input, child.stdin.take()) {
        (Some(bytes), Some(mut stdin)) => Some(std::thread::spawn(move || stdin.write_all(&bytes))),
        _ => None,
    };
    let output = child
        .wait_with_output()
        .map_err(|error| format!("git stopped before it answered: {error}"))?;
    if let Some(writer) = writer {
        writer
            .join()
            .map_err(|_| "the thread writing git's input panicked".to_owned())?
            .map_err(|error| format!("git's input could not be written: {error}"))?;
    }
    if !output.status.success() {
        return Err(format!(
            "`git {}` refused: {}",
            args.join(" "),
            shown(String::from_utf8_lossy(&output.stderr).trim())
        ));
    }
    Ok(output.stdout)
}

/// This tree's own history, as the release reads it (`crate::release`): the
/// tags, and the commits between them.
///
/// **Read as this step reads its own**: git is started over this tree's
/// `.git` and nothing else, with none of git's own variables inherited
/// ([`a_git`]), and a tree with no `.git`, a shallow clone and a grafted one
/// are each refused before the first question — every one of them holds less
/// history than there is, and a version or a changelog computed from part of
/// it is computed wrong.
#[derive(Debug)]
pub(crate) struct Own {
    /// The tree whose own repository is read.
    root: std::path::PathBuf,
    /// The environment git is handed all of but git's own variables.
    inherited: Vec<(OsString, OsString)>,
}

impl Own {
    /// The history of `root`, with `inherited` as the environment git is
    /// started with.
    ///
    /// # Errors
    ///
    /// Returns why the tree has no history to read, or why it holds less than
    /// all of it.
    pub(crate) fn of(root: &Path, inherited: &[(OsString, OsString)]) -> Result<Self, String> {
        let here = root
            .join(".git")
            .try_exists()
            .map_err(|error| format!("whether this tree has a .git could not be read: {error}"))?;
        if !here {
            return Err(
                "this tree has no .git of its own, and a release is computed from the history: \
                 the tags and the commits between them. Release from a clone"
                    .to_owned(),
            );
        }
        let own = Self {
            root: root.to_path_buf(),
            inherited: inherited.to_vec(),
        };
        not_shallow(&own.reading())?;
        not_grafted(&own.reading())?;
        Ok(own)
    }

    /// The reading every question goes through.
    fn reading(&self) -> Reading<'_> {
        Reading {
            root: &self.root,
            inherited: &self.inherited,
        }
    }

    /// Asks git `args`, with `input` on its standard input, and answers what
    /// it printed.
    ///
    /// # Errors
    ///
    /// Returns git's refusal, or why git could not be run.
    pub(crate) fn asked(&self, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
        git(&self.reading(), args, input)
    }

    /// Every commit of `hashes`, as git stores it, in the order asked.
    ///
    /// # Errors
    ///
    /// Returns why git could not answer, or why its answer is not the commits
    /// asked for.
    pub(crate) fn commits(&self, hashes: &[&str]) -> Result<Vec<(String, Vec<u8>)>, String> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }
        let mut asked = hashes.join("\n").into_bytes();
        asked.push(b'\n');
        let answer = git(&self.reading(), &["cat-file", "--batch"], Some(asked))?;
        Ok(the_objects(&answer, hashes, "commit")?
            .into_iter()
            .map(|(hash, raw)| (hash, raw.to_vec()))
            .collect())
    }
}

#[cfg(test)]
/// A repository made in a scratch of its own, with `git init`, for the tests
/// here and for the one that drives the step in `check`.
///
/// **Nothing of this machine's git reaches it**: no global or system
/// configuration, no hook, and no signature — the one place this tree turns
/// `commit.gpgsign` off, because these commits never leave the scratch.
pub(crate) struct ARepository {
    /// The scratch, which goes however the test leaves.
    held: crate::check::LeavesNothingBehind,
}

#[cfg(test)]
impl ARepository {
    /// An empty repository, named for the test that makes it.
    pub(crate) fn new(what: &str) -> Self {
        let held = crate::check::LeavesNothingBehind::new(&format!("history-{what}"));
        std::fs::create_dir_all(held.root.join("repository")).expect("a writable scratch");
        std::fs::write(held.root.join("gitconfig"), "").expect("a writable scratch");
        let made = Self { held };
        made.git(&["init", "-q"]);
        made
    }

    /// Where the repository is.
    pub(crate) fn root(&self) -> std::path::PathBuf {
        self.held.root.join("repository")
    }

    /// Runs git in the repository, as a test commits, and answers what it
    /// printed.
    pub(crate) fn git(&self, args: &[&str]) -> String {
        self.git_fed(args, &[])
    }

    /// Runs git in the repository with `input` on its standard input, and
    /// answers what it printed.
    fn git_fed(&self, args: &[&str], input: &[u8]) -> String {
        #[expect(
            clippy::disallowed_methods,
            reason = "a tool the test drives; its output is read, not printed"
        )]
        let mut command = std::process::Command::new("git");
        let inherited: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        inheriting(&mut command, &inherited);
        let mut child = command
            .env("GIT_CONFIG_GLOBAL", self.held.root.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CEILING_DIRECTORIES", &self.held.root)
            .args([
                "-c",
                "user.name=A Person",
                "-c",
                "user.email=a.person@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "init.defaultBranch=dev",
            ])
            .arg("-C")
            .arg(self.root())
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("git runs");
        child
            .stdin
            .take()
            .expect("git's input is a pipe")
            .write_all(input)
            .expect("git reads its input");
        let output = child.wait_with_output().expect("git runs");
        assert!(
            output.status.success(),
            "`git {}` failed in the scratch: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Writes `files`, commits them with exactly `message`, and answers the
    /// commit's hash.
    pub(crate) fn commit(&self, message: &str, files: &[(&str, &[u8])]) -> String {
        for (path, bytes) in files {
            std::fs::write(self.root().join(path), bytes).expect("a writable scratch");
            self.git(&["add", "--", path]);
        }
        self.commit_bytes(&[], message.as_bytes())
    }

    /// Commits what is staged with exactly the bytes of `message`, with git
    /// told `settings` first, and answers the commit's hash.
    fn commit_bytes(&self, settings: &[&str], message: &[u8]) -> String {
        let written = self.held.root.join("message");
        std::fs::write(&written, message).expect("a writable scratch");
        let file = written.to_string_lossy();
        let asked = [
            settings,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "--no-verify",
                "--cleanup=verbatim",
                "-F",
                &file,
            ][..],
        ]
        .concat();
        self.git(&asked);
        self.head()
    }

    /// Writes a commit over `HEAD` whose message is the bytes of `message` as
    /// they are, moves `HEAD` to it, and answers its hash.
    ///
    /// **Written as an object**, because `git commit` reads a message that is
    /// not UTF-8 as Latin-1 and stores it converted.
    fn commit_raw(&self, message: &[u8]) -> String {
        let tree = self.git_fed(&["mktree"], b"");
        let mut object = format!(
            "tree {}\nparent {}\nauthor A Person <a.person@example.invalid> 0 +0000\n\
             committer A Person <a.person@example.invalid> 0 +0000\n\n",
            tree.trim(),
            self.head()
        )
        .into_bytes();
        object.extend_from_slice(message);
        let commit = self
            .git_fed(&["hash-object", "-t", "commit", "-w", "--stdin"], &object)
            .trim()
            .to_owned();
        self.git(&["update-ref", "HEAD", &commit]);
        commit
    }

    /// The commit `HEAD` names.
    pub(crate) fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    /// The hash of a blob holding `bytes`, written to the repository
    /// when `written` and left out of it when not.
    fn blob(&self, bytes: &[u8], written: bool) -> String {
        let asked: &[&str] = if written {
            &["hash-object", "-w", "--stdin"]
        } else {
            &["hash-object", "--stdin"]
        };
        self.git_fed(asked, bytes).trim().to_owned()
    }

    /// Commits a tree of `entries` — each a path, as its bytes, and a blob's
    /// hash — over `parents`, moves `HEAD` to it, and answers its hash.
    ///
    /// **Made from objects, never from the work tree**, so a path no file
    /// system writes — a carriage return or a newline in it — is one the
    /// commit still holds, as a commit made elsewhere can, and a blob the
    /// repository does not hold is one the tree still names.
    fn commit_a_tree(&self, message: &str, parents: &[&str], entries: &[(&[u8], &str)]) -> String {
        let mut listed = Vec::new();
        for (path, blob) in entries {
            listed.extend_from_slice(format!("100644 blob {blob}\t").as_bytes());
            listed.extend_from_slice(path);
            listed.push(0);
        }
        let tree = self.git_fed(&["mktree", "-z", "--missing"], &listed);
        let written = self.held.root.join("message");
        std::fs::write(&written, message).expect("a writable scratch");
        let file = written.to_string_lossy();
        let mut asked = vec!["commit-tree", tree.trim()];
        for parent in parents {
            asked.extend(["-p", parent]);
        }
        asked.extend(["-F", &file]);
        let commit = self.git(&asked).trim().to_owned();
        self.git(&["update-ref", "HEAD", &commit]);
        commit
    }

    /// Merges `branch` into the branch checked out, with exactly `message`.
    pub(crate) fn merge(&self, branch: &str, message: &str) -> String {
        let written = self.held.root.join("message");
        std::fs::write(&written, message).expect("a writable scratch");
        self.git(&[
            "merge",
            "-q",
            "--no-ff",
            "--no-verify",
            "--cleanup=verbatim",
            "-F",
            &written.to_string_lossy(),
            branch,
        ]);
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    /// The variables of git's own that name this repository, as a hook git
    /// runs can be handed them: its `.git`, and the objects in it.
    fn the_variables_a_hook_names(&self) -> Vec<(OsString, OsString)> {
        let git = self.root().join(".git");
        vec![
            ("GIT_DIR".into(), git.clone().into_os_string()),
            (
                "GIT_OBJECT_DIRECTORY".into(),
                git.join("objects").into_os_string(),
            ),
        ]
    }

    /// Writes an event file in the scratch, and answers its path.
    fn event(&self, json: &str) -> std::path::PathBuf {
        let path = self.held.root.join("event.json");
        std::fs::write(&path, json).expect("a writable scratch");
        path
    }

    /// The history judged as the gate judges it, with `listed` as the list
    /// of paths allowed above the limit.
    fn judged(&self, pull_request: Option<&PullRequest>, listed: &[(&str, u64, &str)]) -> Judged {
        match judged_against(&self.root(), pull_request, listed) {
            Ok(History::Judged(judged)) => judged,
            other => panic!("the scratch repository was not judged: {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::ARepository;

    /// What each finding says, as one line, for an assertion to show.
    fn said(judged: &super::Judged) -> Vec<String> {
        judged
            .findings
            .iter()
            .map(|finding| format!("{}:{} — {}", finding.file, finding.line, finding.what))
            .collect()
    }

    /// Words assembled at run time, so this file never holds the line a test
    /// builds out of them.
    fn joined(parts: &[&str]) -> String {
        parts.concat()
    }

    /// A trailer key that credits a co-author, assembled.
    fn the_coauthor_key() -> String {
        joined(&["Co", "-Authored", "-By"])
    }

    /// An agent's product name, assembled.
    fn an_agent() -> String {
        joined(&["Cla", "ude"])
    }

    /// Every type is a subject's opening, with a scope and a `!` or without.
    #[test]
    fn a_conventional_subject_is_accepted_in_every_shape_it_takes() {
        for kind in super::TYPES {
            for line in [
                format!("{kind}: say what the change does"),
                format!("{kind}(xtask): say what the change does"),
                format!("{kind}!: say what the change does"),
                format!("{kind}(nc-platform)!: say what the change does"),
                format!("{kind}(a_b.c/d-1): say what the change does"),
                format!("{kind}: say what the change does (#12)"),
            ] {
                assert_eq!(super::subject(&line), Ok(()), "`{line}` was refused");
            }
        }
        // The first pull request merged into `dev`, as GitHub titled its merge
        // commit.
        assert_eq!(
            super::subject("docs: state what the code does, not how it came to be (#1)"),
            Ok(())
        );
    }

    /// A subject breaking one rule is refused for that rule.
    #[test]
    fn a_subject_that_breaks_a_rule_is_refused_by_name() {
        for (line, rule) in [
            ("", "no `:`"),
            ("add the history step", "no `:`"),
            ("Merge pull request #3 from a/branch", "no `:`"),
            ("Revert \"feat: add a thing\"", "not a type"),
            ("Feat: add a thing", "not a type"),
            ("feature: add a thing", "not a type"),
            ("fix!(xtask): add a thing", "not a type"),
            ("fix(xtask: add a thing", "not closed"),
            ("fix(): add a thing", "scope is empty"),
            ("fix(Xtask): add a thing", "scope holds"),
            ("fix(two words): add a thing", "scope holds"),
            ("fix:add a thing", "one space follows"),
            ("fix:  add a thing", "and only one"),
            ("fix: ", "nothing follows"),
            ("fix:", "one space follows"),
            ("fix: add a thing ", "ends in whitespace"),
            ("fix: add a thing.", "ends with a period"),
            ("fix: add a thing. (#3)", "ends with a period"),
            ("fix: add\u{7}a thing", "cannot see"),
            ("fix: add a\u{202E} thing", "cannot see"),
        ] {
            match super::subject(line) {
                Ok(()) => panic!("`{}` was accepted", line.escape_debug()),
                Err(why) => assert!(
                    why.contains(rule),
                    "`{}` was refused for `{why}`, and the rule it breaks is `{rule}`",
                    line.escape_debug()
                ),
            }
        }
        // `(#)` and `(#12a)` are not GitHub's suffix, so each stays in the
        // description and is judged there, as any other words are — and then
        // the period before it is not where the description ends.
        assert_eq!(super::subject("fix: add a thing. (#)"), Ok(()));
        assert_eq!(super::subject("fix: add a thing. (#12a)"), Ok(()));
    }

    /// Asserts that `text`, a body's one line, is found once on its line and
    /// as `how`.
    fn found_once_as(text: &str, how: &str) {
        let found = super::credits(&format!("fix: a subject\n\n{text}\n"));
        assert_eq!(found.len(), 1, "`{text}` was found {} time(s)", found.len());
        assert_eq!(found[0].0, 3, "`{text}` was found on the wrong line");
        assert!(
            found[0].1.contains(how),
            "`{text}` was found as `{}`",
            found[0].1
        );
    }

    /// Each shape of credit is found on its line, and nothing like one is.
    #[test]
    fn every_shape_of_an_agent_credit_is_found_and_ordinary_prose_is_not() {
        let agent = an_agent();
        let key = the_coauthor_key();
        for (text, how) in [
            (
                format!("{key}: {agent} <noreply@example.invalid>"),
                "trailer",
            ),
            (
                format!(
                    "{key}: A Person <noreply@{}.com>",
                    joined(&["anth", "ropic"])
                ),
                "trailer",
            ),
            (
                format!("{key}: helper[bot] <1+helper[bot]@users.noreply.github.com>"),
                "trailer",
            ),
            (
                format!("{}: {agent}", joined(&["Assisted", "-by"])),
                "trailer",
            ),
            (
                format!("{}: GPT-4o", joined(&["Generated", "-with"])),
                "trailer",
            ),
            (
                format!("Generated with [{agent} Code]"),
                "saying it was generated with",
            ),
            (
                format!("this was written by {agent}"),
                "saying it was written by",
            ),
            (
                format!("see https://{}/code/session_1", joined(&["cla", "ude.ai"])),
                "a link to claude.ai",
            ),
            (
                format!("see www.{}/c/1", joined(&["chat", "gpt.com"])),
                "a link to chatgpt.com",
            ),
            (
                format!("[x](https://app.{}/sessions/1)", joined(&["dev", "in.ai"])),
                "a link to devin.ai",
            ),
            (format!("{} done", super::ROBOT), "robot emoji"),
        ] {
            found_once_as(&text, how);
        }
        for text in [
            "Read `AGENTS.md` before anything else",
            "Reviewed-by: A Person <a.person@example.org>",
            &format!("{key}: A Person <a.person@example.org>"),
            "docs/ is generated by `cargo xtask docs`",
            "the cursor moves when the line is written with a tab",
            "a plugin reaches api.example.org/v1 and nothing else",
            "see https://github.com/noctua-world/noctua-central/pull/2",
            &format!("the {agent} site is not linked here"),
            "Note: the scope is optional",
        ] {
            assert_eq!(
                super::credits(text),
                Vec::new(),
                "`{text}` was read as a credit"
            );
        }
    }

    /// A product is read by the address it signs from, at its site or its
    /// maker's, and by the link a trailer of another key holds.
    ///
    /// A line saying how a change was made names the agent right after its
    /// words, so nothing later in a sentence, and no ordinary word, is read
    /// as one.
    #[test]
    fn a_product_is_read_by_its_address_and_a_sentence_by_the_name_after_its_words() {
        let agent = an_agent();
        let key = the_coauthor_key();
        for (text, how) in [
            // A product left out of the names, read by the address it signs
            // from, at its site or under it.
            (
                format!("{key}: A Person <1+a@users.{}>", joined(&["dev", "in.ai"])),
                "trailer",
            ),
            (
                format!("{key}: Amp <amp@{}>", joined(&["amp", "code.com"])),
                "trailer",
            ),
            (
                format!(
                    "{key}: {} <a@{}>",
                    joined(&["open", "hands"]),
                    joined(&["all-", "hands.dev"])
                ),
                "trailer",
            ),
            // A trailer of another key, read by the link it holds.
            (
                format!(
                    "{}: https://{}/threads/T-1",
                    joined(&["Amp-Thread", "-ID"]),
                    joined(&["amp", "code.com"])
                ),
                "a link to ampcode.com",
            ),
            // The agent right after the words, past an article.
            (
                format!("this was written with the {agent} app"),
                "saying it was written with",
            ),
            (
                format!("the page is made with care and generated by {agent}"),
                "saying it was generated by",
            ),
            // The agent after its maker's name, and a name that is also a
            // word, written as a name.
            (
                format!("Generated by GitHub {}", joined(&["Co", "pilot"])),
                "saying it was generated by",
            ),
            (
                format!("Generated with {}", joined(&["Cur", "sor"])),
                "saying it was generated with",
            ),
            (
                format!("made with the {} editor", joined(&["Cur", "sor"])),
                "saying it was made with",
            ),
            // A tool that signs from its maker's domain, at it or under it,
            // whatever the maker is called.
            (
                format!("{key}: A Person <noreply@{}>", joined(&["x", ".ai"])),
                "trailer",
            ),
            (
                format!(
                    "{key}: A Person <noreply@{}>",
                    joined(&["cogni", "tion.ai"])
                ),
                "trailer",
            ),
            (
                format!("{key}: A Person <a@mail.{}>", joined(&["co", "here.com"])),
                "trailer",
            ),
        ] {
            found_once_as(&text, how);
        }
        for text in [
            // An agent's name later in the line says nothing of how it was
            // made, a maker's name before something else names no agent, and
            // `cursor` in lower case is the terminal's word here.
            "the caret is made with a blinking block cursor",
            "the selection is made with the cursor",
            "the disk image is made with xtask, and its partition table is GPT",
            "the release is made with GitHub Actions",
            // An address is read in a trailer, and a sentence naming one is
            // prose.
            &format!(
                "its vendor answers at support@{}",
                joined(&["cur", "sor.com"])
            ),
            // A vendor's domain is read in an address and not in a link, which
            // is how a plugin names the interface it reaches. A person's
            // GitHub address, and a host that only ends in a vendor's domain,
            // are a person's.
            &format!("a plugin reaches https://api.{}/v1", joined(&["x", ".ai"])),
            &format!(
                "a plugin reaches api.{}/v1 and nothing else",
                joined(&["open", "ai.com"])
            ),
            &format!("{key}: A Person <1+a.person@users.noreply.github.com>"),
            &format!("{key}: A Person <a.person@onyx.ai>"),
        ] {
            assert_eq!(
                super::credits(text),
                Vec::new(),
                "`{text}` was read as a credit"
            );
        }
    }

    /// A trailer is read as git reads one: with spaces or a tab between its
    /// key and its colon, and with its value folded onto the lines below it
    /// that open with whitespace.
    #[test]
    fn a_trailer_is_read_as_git_reads_one() {
        let agent = an_agent();
        let key = the_coauthor_key();
        let address = || joined(&["dev", "in.ai"]);
        for text in [
            format!(
                "{key} : {} Agent <a@{}>",
                joined(&["Cur", "sor"]),
                joined(&["cur", "sor.com"])
            ),
            format!("{key}\t: A Person <a@{}>", address()),
            format!("{key}  \t : A Person <a@{}>", address()),
            format!("{key}:\n  A Person <a@{}>", address()),
            format!("{key}: A Person\n\t<a@{}>", address()),
            format!("{key}:\n {agent}\n  <noreply@example.invalid>"),
        ] {
            found_once_as(&text, "trailer");
        }
        for text in [
            format!("{key}: A Person\n  <a.person@example.org>"),
            // Only a line opening with whitespace goes on with the value, and
            // a blank line ends it.
            format!("{key}: A Person <a.person@example.org>\nThe {agent} site is not linked"),
            format!("{key}: A Person <a.person@example.org>\n\n    the {agent} site, indented"),
        ] {
            assert_eq!(
                super::credits(&text),
                Vec::new(),
                "`{text}` was read as a credit"
            );
        }
    }

    /// Every commit the history holds is judged, the first and a merged
    /// branch's included, and not only the one `HEAD` names.
    #[test]
    fn every_commit_reachable_is_judged_and_not_only_the_last() {
        let repository = ARepository::new("reachable");
        let first = repository.commit("Initial commit", &[]);
        repository.commit("feat: add the first thing", &[]);
        repository.git(&["checkout", "-q", "-b", "side"]);
        let side = repository.commit("fix: Repair a thing.", &[]);
        repository.git(&["checkout", "-q", "dev"]);
        let merged = repository.merge(
            "side",
            &format!(
                "fix: repair a thing (#4)\n\n{}: {} <noreply@example.invalid>\n",
                the_coauthor_key(),
                an_agent()
            ),
        );
        repository.commit("docs: say what the last thing does", &[]);
        let judged = repository.judged(None, &[]);
        assert_eq!(judged.commits, 5, "{:?}", said(&judged));
        let found = said(&judged);
        for (hash, line, what) in [
            (
                &first,
                1,
                "the subject `Initial commit` is not a conventional one",
            ),
            (&side, 1, "ends with a period"),
            (&merged, 3, "credits an agent"),
        ] {
            assert!(
                found.iter().any(|finding| finding
                    .starts_with(&format!("commit {}:{line} ", &hash[..12]))
                    && finding.contains(what)),
                "commit {hash} was not refused for `{what}`: {found:#?}"
            );
        }
        assert_eq!(found.len(), 3, "{found:#?}");
    }

    /// A history whose every commit is conventional and credits no agent
    /// passes, merge commits titled by GitHub included.
    #[test]
    fn a_conventional_history_passes() {
        let repository = ARepository::new("conventional");
        repository.commit("feat: add the foundations", &[("a.txt", &b"a\n"[..])]);
        repository.git(&["checkout", "-q", "-b", "side"]);
        repository.commit(
            "fix(xtask): read a path as cargo wrote it\n\nThe body says why.\n",
            &[],
        );
        repository.git(&["checkout", "-q", "dev"]);
        repository.merge("side", "fix(xtask): read a path as cargo wrote it (#2)\n\n## What changes\n\nRead `AGENTS.md`.\n");
        let judged = repository.judged(None, &[]);
        assert_eq!(said(&judged), Vec::<String>::new());
        assert_eq!(judged.commits, 3);
        assert!(judged.blobs >= 1, "the blob committed was not measured");
        assert!(!judged.pull_request);
    }

    /// A subject is one line: the line after it is blank.
    #[test]
    fn a_subject_that_runs_on_into_the_body_is_refused() {
        let repository = ARepository::new("one-line");
        let ran_on = repository.commit("fix: read a path\nas cargo wrote it\n", &[]);
        let judged = repository.judged(None, &[]);
        assert_eq!(
            said(&judged),
            vec![format!(
                "commit {}:2 — the line after the subject is not blank, so the subject runs on into it",
                &ran_on[..12]
            )]
        );
    }

    /// A blob above the limit is refused wherever it sits in the history, and
    /// one at the limit is not.
    #[test]
    fn a_blob_above_the_limit_is_refused_even_after_it_was_removed() {
        let at = usize::try_from(super::LIMIT).expect("the limit fits");
        let repository = ARepository::new("size");
        repository.commit(
            "feat: add a file at the limit",
            &[("at.txt", &vec![b'x'; at][..])],
        );
        let judged = repository.judged(None, &[]);
        assert_eq!(
            said(&judged),
            Vec::<String>::new(),
            "a blob at the limit was refused"
        );

        repository.commit(
            "feat: add a file above it",
            &[("above.bin", &vec![b'x'; at + 1][..])],
        );
        repository.git(&["rm", "-q", "--", "above.bin"]);
        repository.commit("fix: take the file out again", &[]);
        let judged = repository.judged(None, &[]);
        assert_eq!(judged.findings.len(), 1, "{:?}", said(&judged));
        assert!(judged.findings[0].file.starts_with("blob "));
        assert!(
            judged.findings[0]
                .what
                .contains(&format!("{} byte(s) at `above.bin`", at + 1)),
            "{:?}",
            said(&judged)
        );
    }

    /// A blob that only a merge commit puts in the history is measured as one
    /// any other commit adds.
    #[test]
    fn a_blob_only_a_merge_adds_is_measured() {
        let at = usize::try_from(super::LIMIT).expect("the limit fits");
        let repository = ARepository::new("merge-adds");
        let base = repository.commit("feat: add the foundations", &[]);
        let side = repository.commit_a_tree("feat: add a side", &[&base], &[]);
        let big = repository.blob(&vec![b'x'; at + 1], true);
        repository.commit_a_tree(
            "feat: merge the side (#3)",
            &[&base, &side],
            &[(b"merged.bin", &big)],
        );
        let judged = repository.judged(None, &[]);
        assert_eq!(
            said(&judged),
            vec![format!(
                "blob {}:1 — a blob of {} byte(s) at `merged.bin`, above the limit of {} — and \
                 every clone downloads it, even where a later commit removed it",
                &big[..12],
                at + 1,
                super::LIMIT
            )]
        );
    }

    /// A listed path is allowed above the limit up to its own ceiling, and no
    /// further.
    #[test]
    fn a_listed_path_is_allowed_up_to_its_ceiling_and_not_past_it() {
        let at = usize::try_from(super::LIMIT).expect("the limit fits");
        let listed = [("big.rs", super::LIMIT * 2, "a reason")];
        let repository = ARepository::new("listed");
        repository.commit(
            "feat: add a large source",
            &[("big.rs", &vec![b'x'; at * 2][..])],
        );
        let judged = repository.judged(None, &listed);
        assert_eq!(said(&judged), Vec::<String>::new());
        assert_eq!(judged.listed, 1, "the listed blob was not counted");

        repository.commit(
            "feat: grow it past its line",
            &[("big.rs", &vec![b'x'; at * 2 + 1][..])],
        );
        let judged = repository.judged(None, &listed);
        assert_eq!(judged.findings.len(), 1, "{:?}", said(&judged));
        assert!(
            judged.findings[0]
                .what
                .contains("its line in `ABOVE_THE_LIMIT` allows")
        );
        // And the same blob at a path nobody listed is refused at the limit.
        let judged = repository.judged(None, &[]);
        assert_eq!(judged.findings.len(), 2, "{:?}", said(&judged));
    }

    /// A listed path is its bytes, and every path a blob sits at is judged.
    ///
    /// The same blob a space, a carriage return or a newline away from the
    /// listed path is held to the limit there.
    #[test]
    fn a_listed_path_is_its_bytes_and_every_path_a_blob_sits_at_is_judged() {
        let at = usize::try_from(super::LIMIT).expect("the limit fits");
        let listed = [("big.rs", super::LIMIT * 2, "a reason")];
        let repository = ARepository::new("exact");
        let first = repository.commit("feat: add the foundations", &[]);
        let big = repository.blob(&vec![b'x'; at + 1], true);
        let allowed =
            repository.commit_a_tree("feat: add a large source", &[&first], &[(b"big.rs", &big)]);
        let judged = repository.judged(None, &listed);
        assert_eq!(said(&judged), Vec::<String>::new());
        assert_eq!(judged.listed, 1, "the listed blob was not counted");

        repository.commit_a_tree(
            "feat: put it beside itself",
            &[&allowed],
            &[
                (b"big.rs", &big),
                (b" big.rs", &big),
                (b"big.rs ", &big),
                (b"big.rs\r", &big),
                (b"big.rs\nx", &big),
            ],
        );
        let judged = repository.judged(None, &listed);
        let found = said(&judged);
        for path in [
            "` big.rs`",
            "`big.rs `",
            "`big.rs\\u{d}`",
            "`big.rs\\u{a}x`",
        ] {
            assert!(
                found.iter().any(|finding| finding
                    == &format!(
                        "blob {}:1 — a blob of {} byte(s) at {path}, above the limit of {} — and \
                         every clone downloads it, even where a later commit removed it",
                        &big[..12],
                        at + 1,
                        super::LIMIT
                    )),
                "the blob at {path} was not refused: {found:#?}"
            );
        }
        assert_eq!(found.len(), 4, "{found:#?}");
        assert_eq!(
            judged.listed, 0,
            "a blob at a path nobody listed was allowed"
        );
    }

    /// A path a finding names holds nothing a terminal or a runner acts on.
    ///
    /// What a reader cannot see in it is escaped, and the finding opens with
    /// the blob's hash.
    #[test]
    fn a_path_a_finding_names_holds_nothing_a_terminal_or_a_runner_acts_on() {
        let at = usize::try_from(super::LIMIT).expect("the limit fits");
        let repository = ARepository::new("path-quoted");
        let first = repository.commit("feat: add the foundations", &[]);
        let big = repository.blob(&vec![b'x'; at + 1], true);
        repository.commit_a_tree(
            "feat: add a file",
            &[&first],
            &[(b"x\r::warning::forged\x1b[2J", &big)],
        );
        let judged = repository.judged(None, &[]);
        assert_eq!(judged.findings.len(), 1, "{:?}", said(&judged));
        let finding = &judged.findings[0];
        assert_eq!(finding.file, format!("blob {}", &big[..12]));
        for text in [&finding.file, &finding.what] {
            assert!(
                !text.chars().any(nc_schema::unseen),
                "`{}` holds a character a reader cannot see",
                text.escape_debug()
            );
        }
        assert!(
            finding
                .what
                .contains("at `x\\u{d}::warning::forged\\u{1b}[2J`"),
            "{}",
            finding.what
        );
    }

    /// A clone that lacks a blob the history names is refused, rather than
    /// judged without it.
    #[test]
    fn a_clone_missing_a_blob_is_refused_rather_than_judged() {
        let repository = ARepository::new("partial");
        let first = repository.commit("feat: add the foundations", &[]);
        let absent = repository.blob(b"never written\n", false);
        repository.commit_a_tree("feat: add a file", &[&first], &[(b"gone.txt", &absent)]);
        match super::judged_against(&repository.root(), None, &[]) {
            Err(why) => assert!(
                why.contains("partial clone") && why.contains(&absent),
                "{why}"
            ),
            Ok(judged) => panic!("a clone missing a blob was judged: {judged:?}"),
        }
    }

    /// A replacement ref does not stand in for the commit it replaces: the
    /// commit itself is what is judged.
    #[test]
    fn a_replacement_ref_does_not_stand_in_for_the_commit_it_replaces() {
        let repository = ARepository::new("replaced");
        let replaced = repository.commit("Not a conventional subject", &[]);
        let head = repository.commit("feat: add a thing", &[]);
        let stand_in = repository.commit_a_tree("feat: a conventional subject", &[], &[]);
        repository.git(&["update-ref", "HEAD", &head]);
        repository.git(&["replace", &replaced, &stand_in]);
        let judged = repository.judged(None, &[]);
        assert_eq!(judged.commits, 2, "{:?}", said(&judged));
        assert_eq!(
            said(&judged),
            vec![format!(
                "commit {}:1 — the subject `Not a conventional subject` is not a conventional \
                 one: it has no `:`, so no type opens it. A subject opens with one of {}",
                &replaced[..12],
                super::TYPES.join(", ")
            )]
        );
    }

    /// A message declared in another encoding, and one that is not UTF-8, are
    /// refused for what they are rather than read.
    #[test]
    fn a_message_in_another_encoding_or_in_none_is_refused() {
        let repository = ARepository::new("encoding");
        let declared = repository.commit_bytes(
            &["-c", "i18n.commitEncoding=ISO-8859-1"],
            b"fix: read a caf\xe9 menu\n",
        );
        let undeclared = repository.commit_raw(b"fix: read a caf\xe9 menu\n");
        let judged = repository.judged(None, &[]);
        assert_eq!(
            said(&judged),
            vec![
                format!(
                    "commit {}:1 — its message is not UTF-8, so nothing in it could be read",
                    &undeclared[..12]
                ),
                format!(
                    "commit {}:1 — its message is declared as `ISO-8859-1`, and every message \
                     here is UTF-8",
                    &declared[..12]
                ),
            ]
        );
    }

    /// A pull request is judged as its event names it: the head's and the
    /// base's history, the title and the body — never the merge commit the
    /// checkout is at.
    #[test]
    fn a_pull_request_is_judged_by_its_event_and_not_by_the_merge_made_for_it() {
        let repository = ARepository::new("pull-request");
        let base = repository.commit("feat: add the foundations", &[]);
        repository.git(&["checkout", "-q", "-b", "proposed"]);
        let head = repository.commit("fix: repair a thing", &[]);
        repository.git(&["checkout", "-q", "dev"]);
        repository.merge("proposed", &format!("Merge {head} into {base}\n"));

        // Off a pull request, `HEAD` is what is judged, and the merge made for
        // the run is refused.
        let judged = repository.judged(None, &[]);
        assert_eq!(judged.findings.len(), 1, "{:?}", said(&judged));

        let event = |title: &str, body: &str| {
            let path = repository.event(
                &serde_json::json!({
                    "pull_request": {
                        "title": title,
                        "body": body,
                        "head": { "sha": head },
                        "base": { "sha": base },
                    }
                })
                .to_string(),
            );
            super::the_pull_request(Some("pull_request".as_ref()), Some(path.as_os_str()))
                .expect("the event is read")
                .expect("the event is a pull request")
        };
        let clean = event("fix: repair a thing", "It says why.");
        let judged = repository.judged(Some(&clean), &[]);
        assert_eq!(said(&judged), Vec::<String>::new());
        assert_eq!(judged.commits, 2, "the head and the base reach two commits");
        assert!(judged.pull_request);

        let titled = event("Repair a thing", "It says why.");
        let judged = repository.judged(Some(&titled), &[]);
        assert_eq!(judged.findings.len(), 1, "{:?}", said(&judged));
        assert_eq!(judged.findings[0].file, "the pull request's title");

        let credited = event(
            "fix: repair a thing",
            &format!(
                "It says why.\n\n{}: {} <noreply@example.invalid>",
                the_coauthor_key(),
                an_agent()
            ),
        );
        let judged = repository.judged(Some(&credited), &[]);
        assert_eq!(judged.findings.len(), 1, "{:?}", said(&judged));
        assert_eq!(judged.findings[0].file, "the pull request's body");
        assert_eq!(judged.findings[0].line, 3);
    }

    /// An event that cannot say what to judge is a refusal, and any other
    /// event is no pull request.
    #[test]
    fn an_event_that_cannot_be_read_is_refused_and_another_event_is_none() {
        let repository = ARepository::new("event");
        let named = Some("pull_request".as_ref());
        assert!(super::the_pull_request(named, None).is_err());
        assert!(
            super::the_pull_request(None, None)
                .expect("no event")
                .is_none()
        );
        let push = repository.event("{}");
        assert!(
            super::the_pull_request(Some("push".as_ref()), Some(push.as_os_str()))
                .expect("a push is read")
                .is_none()
        );
        let sha = "a".repeat(40);
        for (json, refused) in [
            ("not json".to_owned(), "not JSON"),
            ("{}".to_owned(), "no `pull_request`"),
            (
                format!(
                    r#"{{"pull_request":{{"body":null,"head":{{"sha":"{sha}"}},"base":{{"sha":"{sha}"}}}}}}"#
                ),
                "no title",
            ),
            (
                format!(
                    r#"{{"pull_request":{{"title":"fix: x","head":{{"sha":"{sha}"}},"base":{{"sha":"{sha}"}}}}}}"#
                ),
                "no body",
            ),
            (
                format!(
                    r#"{{"pull_request":{{"title":"fix: x","body":null,"base":{{"sha":"{sha}"}}}}}}"#
                ),
                "no `head.sha`",
            ),
            (
                format!(
                    r#"{{"pull_request":{{"title":"fix: x","body":null,"head":{{"sha":"--output=x"}},"base":{{"sha":"{sha}"}}}}}}"#
                ),
                "not a commit's hash",
            ),
        ] {
            let path = repository.event(&json);
            match super::the_pull_request(named, Some(path.as_os_str())) {
                Err(why) => assert!(why.contains(refused), "`{json}` was refused for `{why}`"),
                Ok(read) => panic!("`{json}` was read as {read:?}"),
            }
        }
        let path = repository.event(&format!(r#"{{"pull_request":{{"title":"fix: x","body":null,"head":{{"sha":"{sha}"}},"base":{{"sha":"{sha}"}}}}}}"#));
        let read = super::the_pull_request(named, Some(path.as_os_str()))
            .expect("a pull request with an empty body is read")
            .expect("it is a pull request");
        assert_eq!(read.body, "");
    }

    /// A tree with no `.git` of its own has no history, and the repository
    /// around it is not judged in its place.
    #[test]
    fn a_tree_with_no_repository_of_its_own_is_not_judged_by_the_one_around_it() {
        let repository = ARepository::new("tarball");
        repository.commit("Not a conventional subject", &[]);
        let inside: PathBuf = repository.root().join("tarball");
        std::fs::create_dir_all(&inside).expect("a writable scratch");
        assert!(matches!(
            super::judged_against(&inside, None, &[]),
            Ok(super::History::NoRepository)
        ));
        assert_eq!(repository.judged(None, &[]).findings.len(), 1);
    }

    /// A shallow clone is refused, because the history below its cut is not
    /// there to judge.
    #[test]
    fn a_shallow_clone_is_refused_rather_than_judged() {
        let repository = ARepository::new("shallow");
        repository.commit("Not a conventional subject", &[]);
        let head = repository.commit("feat: add a thing", &[]);
        let shallow: &Path = &repository.root().join(".git").join("shallow");
        std::fs::write(shallow, format!("{head}\n")).expect("a writable scratch");
        match super::judged_against(&repository.root(), None, &[]) {
            Err(why) => assert!(why.contains("shallow"), "{why}"),
            Ok(judged) => panic!("a shallow clone was judged: {judged:?}"),
        }
    }

    /// A clone that grafts its history is refused, because the commits a
    /// graft cuts away are not what git lists.
    #[test]
    fn a_clone_that_grafts_its_history_is_refused_rather_than_judged() {
        let repository = ARepository::new("grafted");
        repository.commit("Not a conventional subject", &[]);
        let head = repository.commit("feat: add a thing", &[]);
        let info = repository.root().join(".git").join("info");
        std::fs::create_dir_all(&info).expect("a writable scratch");
        std::fs::write(info.join("grafts"), format!("{head}\n")).expect("a writable scratch");
        match super::judged_against(&repository.root(), None, &[]) {
            Err(why) => assert!(why.contains("grafts its history"), "{why}"),
            Ok(judged) => panic!("a grafted clone was judged: {judged:?}"),
        }
    }

    /// A tree whose `.git` holds no repository is not judged by the
    /// repository around it: git is handed the tree's own and searches for
    /// none.
    ///
    /// **Under a directory whose name holds the path separator too**, which
    /// a list of directories for git to stop at would be split at.
    #[test]
    fn a_tree_whose_git_holds_no_repository_is_not_judged_by_the_one_around_it() {
        let repository = ARepository::new("ceiling");
        repository.commit("Not a conventional subject", &[]);
        let separated = std::env::join_paths(["a", "b"]).expect("two plain names join");
        for inside in [
            repository.root().join("inside"),
            repository.root().join(separated).join("inside"),
        ] {
            std::fs::create_dir_all(inside.join(".git")).expect("a writable scratch");
            match super::judged_against(&inside, None, &[]) {
                Err(why) => assert!(why.contains("refused"), "{}: {why}", inside.display()),
                Ok(judged) => panic!(
                    "the repository around {} was judged: {judged:?}",
                    inside.display()
                ),
            }
        }
    }

    /// A variable of git's own that this process inherited does not choose
    /// what is read.
    ///
    /// **`GIT_DIR` and `GIT_OBJECT_DIRECTORY` naming another repository**
    /// leave this one's history the one judged.
    #[test]
    fn a_git_variable_this_process_inherited_does_not_choose_what_is_read() {
        let here = ARepository::new("inherits-here");
        here.commit("feat: add the foundations", &[]);
        let elsewhere = ARepository::new("inherits-elsewhere");
        elsewhere.commit("Not a conventional subject", &[]);
        let mut inherited: Vec<(std::ffi::OsString, std::ffi::OsString)> =
            std::env::vars_os().collect();
        inherited.extend(elsewhere.the_variables_a_hook_names());
        match super::judged_inheriting(&here.root(), None, &[], &inherited) {
            Ok(super::History::Judged(judged)) => {
                assert_eq!(said(&judged), Vec::<String>::new());
                assert_eq!(judged.commits, 1, "another repository's history was read");
            }
            other => panic!("this repository was not judged: {other:?}"),
        }
    }

    /// What tells this test binary, run again, that it is the child of
    /// [`a_git_variable_in_this_process_does_not_choose_what_is_read`], and
    /// which repository it judges.
    const CHILD: &str = "NC_HISTORY_TESTS_CHILD";

    /// A variable of git's own in this process's own environment does not
    /// choose what is read, judged through the entry the gate calls.
    ///
    /// **This test binary is run again with the variables exported**, the
    /// one way to give a process an environment without setting it in this
    /// one. The child writes what it judged beside the repository, so a child
    /// that ran no test, or was started without them, fails here.
    #[test]
    fn a_git_variable_in_this_process_does_not_choose_what_is_read() {
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let judged = if std::env::var_os("GIT_OBJECT_DIRECTORY").is_none() {
                "the child was started without the variables it is about".to_owned()
            } else {
                match super::judged(&root, None) {
                    Ok(super::History::Judged(judged)) => {
                        format!("{} commit(s), {:?}", judged.commits, said(&judged))
                    }
                    other => format!("{other:?}"),
                }
            };
            std::fs::write(root.with_file_name("judged"), judged).expect("a writable scratch");
            return;
        }
        let here = ARepository::new("environment-here");
        here.commit("feat: add the foundations", &[]);
        let elsewhere = ARepository::new("environment-elsewhere");
        elsewhere.commit("Not a conventional subject", &[]);
        let ran = the_child(&here.root(), &elsewhere.the_variables_a_hook_names());
        assert!(ran.success(), "the child failed: {ran}");
        let judged = std::fs::read_to_string(here.root().with_file_name("judged"))
            .expect("the child writes what it judged, when it runs this test by its name");
        assert_eq!(judged, "1 commit(s), []");
    }

    /// This test binary, run again as [`CHILD`] says, with `variables` in
    /// its environment and nothing it prints shown.
    #[expect(
        clippy::disallowed_methods,
        reason = "the child is this same test binary, its output discarded: the one way to \
                  give a process an environment without setting it in this one"
    )]
    fn the_child(
        root: &Path,
        variables: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> std::process::ExitStatus {
        std::process::Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "history::tests::a_git_variable_in_this_process_does_not_choose_what_is_read",
                "--test-threads=1",
            ])
            .env(CHILD, root)
            .envs(variables.iter().map(|(name, value)| (name, value)))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the child runs")
    }

    /// Git is started with this process's environment less every variable
    /// of its own and every one of `gh`'s, in any case, handed this tree's own
    /// repository, and with the lazy fetch and the replacement refs closed.
    ///
    /// No two names here are one name to Windows, which reads them without
    /// their case.
    #[test]
    fn git_is_started_with_nothing_of_its_own_from_this_environment() {
        let inherited: Vec<(std::ffi::OsString, std::ffi::OsString)> = [
            ("GIT_DIR", "elsewhere"),
            ("GIT_OBJECT_DIRECTORY", "elsewhere"),
            ("git_index_file", "elsewhere"),
            ("Git_Config_Parameters", "'core.x=y'"),
            ("GIT", "a word"),
            ("PATH", "the path"),
            ("GITHUB_SHA", "a commit"),
            ("MY_GIT_DIR", "a variable of another tool"),
            ("GH_TOKEN", "the release App's token"),
            ("GITHUB_TOKEN", "the workflow's token"),
            ("gh_debug", "api"),
            ("GH", "a word"),
        ]
        .map(|(name, value)| (name.into(), value.into()))
        .to_vec();
        let root = Path::new("above").join("root");
        let command = super::a_git(&super::Reading {
            root: &root,
            inherited: &inherited,
        });
        let mut started: Vec<(String, Option<String>)> = command
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();
        started.sort();
        let set = |name: &str, value: &str| (name.to_owned(), Some(value.to_owned()));
        let removed = |name: &str| (name.to_owned(), None);
        assert_eq!(
            started,
            [
                set("GH", "a word"),
                removed("GH_TOKEN"),
                set("GIT", "a word"),
                set("GITHUB_SHA", "a commit"),
                removed("GITHUB_TOKEN"),
                set("GIT_DIR", ".git"),
                set("GIT_NO_LAZY_FETCH", "1"),
                removed("GIT_OBJECT_DIRECTORY"),
                removed("Git_Config_Parameters"),
                set("MY_GIT_DIR", "a variable of another tool"),
                set("PATH", "the path"),
                removed("gh_debug"),
                removed("git_index_file"),
            ]
        );
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();
        assert_eq!(
            args,
            [
                std::ffi::OsStr::new("-C"),
                root.as_os_str(),
                std::ffi::OsStr::new("--no-replace-objects"),
            ]
        );
    }

    /// An answer that is not the one asked for is a refusal: cut short, with
    /// more in it, or naming another object.
    #[test]
    fn an_answer_that_is_not_what_was_asked_is_refused() {
        let hash = "a".repeat(40);
        let whole = format!("{hash} commit 3\nabc\n");
        let read = super::the_objects(whole.as_bytes(), &[&hash], "commit").expect("whole");
        assert_eq!(read, vec![(hash.clone(), &b"abc"[..])]);
        for answer in [
            format!("{hash} commit 3\nab"),
            format!("{hash} commit 3\nabc\n{hash} commit 3\nabc\n"),
            format!("{} commit 3\nabc\n", "b".repeat(40)),
            format!("{hash} blob 3\nabc\n"),
            format!("{hash} missing\n"),
            String::new(),
        ] {
            assert!(
                super::the_objects(answer.as_bytes(), &[&hash], "commit").is_err(),
                "`{}` was read as the answer",
                answer.escape_debug()
            );
        }
        // What `cat-file --batch-check` measured: every object asked, once,
        // in order, by its own name.
        let big = super::LIMIT + 1;
        let asked: [&[u8]; 2] = [b"x", b"y"];
        assert_eq!(
            super::the_blobs(format!("x blob {big}\ny tree 9\n").as_bytes(), &asked),
            Ok((1, vec![("x".to_owned(), big)]))
        );
        for answer in [
            "x blob 1\n".to_owned(),
            "x blob 1\ny tree 9\nz blob 1\n".to_owned(),
            "y tree 9\nx blob 1\n".to_owned(),
            "x blob 1 a\ny tree 9\n".to_owned(),
            "x missing\ny tree 9\n".to_owned(),
        ] {
            assert!(
                super::the_blobs(answer.as_bytes(), &asked).is_err(),
                "`{}` was read as the measure",
                answer.escape_debug()
            );
        }
        // Where `diff-tree -z` puts each blob: a blob no commit places, and a
        // change of another shape, are refused.
        let above = [("x".to_owned(), big)];
        let added = ":000000 100644 0000 x A\0a\nb\0:100644 100644 0000 x M\0c\0:100644 000000 x 0000 D\0d\0";
        assert_eq!(
            super::placed(added.as_bytes(), &above),
            Ok(vec![(
                ("x".to_owned(), big),
                vec![b"a\nb".to_vec(), b"c".to_vec()]
            )])
        );
        for changes in [
            "",
            ":000000 100644 0000 y A\0a\0",
            ":100644 100644 x x R100\0a\0b\0",
            "000000 100644 0000 x A\0a\0",
            ":000000 100644 0000 x A\0",
            ":000000 100644 0000 x A\0\0",
            ":000000 100644 0000 x A\0a",
        ] {
            assert!(
                super::placed(changes.as_bytes(), &above).is_err(),
                "`{}` was read as where the blob sits",
                changes.escape_debug()
            );
        }
    }

    /// What a finding quotes from a commit has nothing a terminal or a runner
    /// would act on, and is cut short.
    #[test]
    fn a_quotation_escapes_what_a_reader_cannot_see_and_is_cut_short() {
        assert_eq!(
            super::quoted("a\u{1b}[2Jb\r::warning::c"),
            "`a\\u{1b}[2Jb\\u{d}::warning::c`"
        );
        let long = "x".repeat(super::QUOTED + 1);
        assert_eq!(
            super::quoted(&long),
            format!("`{}…`", "x".repeat(super::QUOTED))
        );
    }

    /// No line of this file, of the one that plants its mutants, of the one
    /// that writes its rule down, or of the release that publishes notes and
    /// pull requests is a line the step reads as a credit.
    ///
    /// **The whole file, its tests included**, because every commit that
    /// carries a line is history: a must-fail case exists only at run time,
    /// assembled from parts, and a comment that shows a credit line leaves the
    /// product's name out.
    #[test]
    fn the_files_that_hold_this_step_hold_no_credit_of_their_own() {
        for (file, text) in [
            ("xtask/src/history.rs", include_str!("history.rs")),
            ("xtask/mutants.py", include_str!("../mutants.py")),
            ("AGENTS.md", include_str!("../../AGENTS.md")),
            ("xtask/src/release.rs", include_str!("release.rs")),
        ] {
            assert_eq!(
                super::credits(text),
                Vec::new(),
                "{file} holds a line this step reads as a credit: name the product by its \
                 shape, or assemble the line from parts inside a test"
            );
        }
    }
}
