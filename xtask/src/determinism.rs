//! Nothing the emitter reads may differ between two runs.
//!
//! Acceptance criterion 4 is that two generations are byte-identical, and it is
//! checked by a `check` step that generates twice in one process and compares.
//!
//! **That cannot see the failure that matters most.** Both generations happen
//! milliseconds apart in
//! the same process, so a stamp with day granularity — a date in a header, a
//! year in a licence line — is identical in both and the check passes every day
//! until it does not. The CI job that builds twice and diffs would catch it, on
//! the day CI first runs, in a repository that has no CI yet (`§5.3`).
//!
//! So this reads the source instead. A clock, a random number, an unordered map,
//! the environment or the process's own identity reaching the emitter is a
//! finding at the line that names it — before the output is ever compared, and
//! without waiting for midnight.
//!
//! **And not two toolchains.** `rust-toolchain.toml` names a moving channel, so
//! the compiler this repository builds with changes on its own about every six
//! weeks. Every check here — this scan, the two generations compared in one
//! process, and the CI job that builds twice — compares runs of the *same*
//! toolchain. What none of them can see is a new `rustfmt` that wraps a line
//! differently, or a derive that expands to something else: generated output
//! moves on a day nobody edited anything. Regenerating and diffing after a
//! toolchain change is the developer's step, and it is written here because a
//! limit nobody wrote down is a limit somebody will mistake for a guarantee.
//!
//! **It reads this repository's own lines and not its dependencies'.** A crate
//! from the registry that read a clock would be invisible here, and nothing a
//! source scan can do would change that. What covers it is the other two
//! checks — two generations compared in one process, and CI building twice —
//! which are about the output rather than about the source. This gate exists
//! because those two are blind to a day boundary, not because they are useless.
//!
//! # The hatch
//!
//! `allow-nondeterminism:` with a reason, on the same line, because rustfmt is
//! free to move a comment that sits above a statement. `ls` the markers to see
//! how many there are; what they cover is a `read_dir` whose caller sorts, a
//! process id and a temporary directory that name a scratch path and reach no
//! output. The marker is how the next reader learns that without going to look.

use std::path::Path;

use crate::gates::{Finding, walk};
use crate::relative_to;

/// The crates whose output is compared byte for byte.
///
/// `nc-canon` is here with the emitter because the chain and the catalog both
/// hash what it writes: a digest that differs between runs is the same defect
/// one layer down, and it verifies on one side and not the other.
pub(crate) const EMITS: [&str; 4] = [
    "crates/nc-catalog-compiler/src",
    "crates/nc-canon/src",
    // The judge the compiler holds every command to: it decides whether the
    // catalog compiles at all, so a clock or an unordered map in it is a build
    // that passes on one run and refuses on the next.
    "crates/nc-schema/src",
    // **The last stage of generation, and it was outside the scan.**
    // `every_crate_the_emitter_reaches_is_read` walks the dependency closure of
    // `nc-catalog-compiler`, so `xtask` can never enter it — and
    // `artifacts` in that file is what pipes every generated file through
    // rustfmt and produces the bytes that reach disk. A `std::env::var("USER")`
    // there, appended to each generated file, gave:
    //
    // ```text
    // //! Built by noctua.
    // ok    15 generated file(s) match the catalog
    // ok    two generations are byte-identical
    // ok    <n> line(s) of the emitter read nothing that changes
    // check passed
    // ```
    //
    // The identical line one directory over fires the `env` needle instantly.
    // Listed by hand because no dependency edge leads here: the closure test
    // reaches the crates the emitter *calls*, and this is the crate that calls
    // the emitter.
    "xtask/src/build.rs",
];

/// The directories [`EMITS`] covers, for a floor to count against.
///
/// **A count with no floor is a number, not a result.** `inspected_something`
/// asks only that the walk read something, so a filter keeping one file left
/// `ok 1 line(s) of the emitter read nothing that changes` and `check passed`.
/// Every entry of [`EMITS`], including the file: leaving it out was 268 lines of
/// slack in an exact comparison, and it is the stage that writes the bytes.
pub(crate) const EMITS_UNDER: &[&str] = &[
    "crates/nc-catalog-compiler/src",
    "crates/nc-canon/src",
    "crates/nc-schema/src",
    // **And the file, which was 268 lines of free slack.** The floor counted
    // two of `EMITS`' three entries while the walk read all three, so
    // `read = 7148` against `expected = 6880` — and one `continue` dropping
    // `xtask/src/build.rs`, the stage that runs rustfmt and produces the bytes
    // that reach disk, left `ok 6880 line(s)` with a live `SystemTime::now()`
    // in it. `lines_under` takes a path and does not care that this one is a
    // file.
    "xtask/src/build.rs",
];

/// The crate whose output is written to disk and compared byte for byte.
///
/// [`EMITS`] is checked against this crate's own workspace dependencies, so a
/// third crate the emitter starts calling has to be listed here or the test
/// below fails. An inclusion list nothing ties to reality is the same defect as
/// an exclusion list nothing tests.
#[cfg(test)]
const EMITTER: &str = "nc-catalog-compiler";

/// The crate that is in [`EMITS`] for a reason the dependency graph cannot give.
///
/// `nc-canon` is not a dependency of the emitter. It is here because the journal
/// hashes what it writes and the catalog hashes what the emitter writes: a
/// digest that differs between runs is the same defect one layer down, and it
/// verifies on one side and not the other.
#[cfg(test)]
const ALSO: &str = "nc-canon";

/// What a second run may answer differently, and what to do instead.
/// A slice rather than a sized array, so adding a needle is one edit and not
/// two — a hand-written length beside a list is the shape `counts.rs` refuses
/// in prose, and it is no better in code.
const CHANGES: &[(&str, &str)] = &[
    (
        "SystemTime",
        "a stamp with day granularity is identical in two runs a millisecond apart and \
         differs across midnight, which is the one shape both determinism checks are \
         blind to. Generated output carries no time",
    ),
    (
        "UNIX_EPOCH",
        "the same clock by another name. Generated output carries no time",
    ),
    // **The bare type name, because `::now` is one of its spellings.**
    // `SystemTime` and `HashMap` survive being renamed at the `use` because the
    // needle is the type; these three did not. Measured: `use std::time::Instant
    // as Tick;` and then `Tick::now()` gave **0 findings**, and `Utc` is the
    // severe one — a date at day granularity is invisible to the in-process
    // two-generation check and to CI's same-day cross-checkout diff alike, so
    // nothing else in this repository would ever have said so.
    (
        "Instant",
        "a duration is a measurement of this machine under this load. Nothing measured \
         belongs in a file that is compared byte for byte, and renaming the type at the \
         `use` does not change what it reads",
    ),
    (
        "chrono",
        "a date library in the emitter means a date in the output. Generated output \
         carries no time",
    ),
    (
        "OffsetDateTime",
        "likewise, and the offset makes it differ by timezone as well as by day",
    ),
    (
        "HashMap",
        "iteration order is unspecified and seeded per process, so two runs order the \
         same keys differently. `BTreeMap`, which is what every registry uses",
    ),
    (
        "HashSet",
        "the same, and a set that reaches a file reaches it in an order. `BTreeSet`",
    ),
    ("current_exe", "which binary is running, and from where"),
    ("available_parallelism", "how many cores this machine has"),
    (
        "accessed",
        "when a file was last read, which changes without anything being written",
    ),
    (
        "current_dir",
        "where the process happens to be standing. Paths reaching generated output \
         come from the emitter's own root argument",
    ),
    // **The path segment, not the module path and not the macro.** The needles
    // were `env::var`, `env::vars`, `env::var_os` and `env::args`, and a
    // reviewer got `std::env::consts::OS` past them with one line — putting
    // `//! Platform: linux` into nine generated files, identical on two runs of
    // one machine and different on every other. `env::vars_os` was a hole
    // *created* by the whole-name fix: it matches neither `env::var`, blocked by
    // the `s`, nor `env::vars`, blocked by the `_`. `env::current_exe` and
    // `env::temp_dir` were never needles at all.
    //
    // **Then the module spelling was itself walked past by an alias**, which is
    // the same lesson a third time and the one the neighbouring
    // `process::Command` entry had already been repaired for and this one had
    // not. Measured, in `nc-catalog-compiler`:
    //
    // ```text
    // use std::env as machine;
    // fn probe() -> String { machine::var("USER").unwrap_or_default() }
    //   ->  ok  9070 line(s) of the emitter read nothing that changes
    //       check passed
    // ```
    //
    // The import avoids the `::` and so does every call through the alias, so a
    // needle carrying the separator can see neither. `usize::BITS` on the line
    // beside it was reported instantly, which is what says the walk reached the
    // file. The rule is the *segment*: `env` bounded by anything that is not
    // part of a name matches `std::env as machine`, `use std::{env, fs}`,
    // `env::var` and the build-time macro alike, while `option_env!` and
    // `target_env` are longer names and are not it.
    //
    // The macro is no longer an entry of its own for exactly that reason: it
    // is this needle, and two entries for one line report it twice under two
    // reasons — which `no_needle_is_listed_twice` exists to refuse.
    //
    // Nothing in `std::env` is a property of the catalog, so the module is the
    // rule and its functions are not worth enumerating. Listing them was the
    // mistake twice over, and the second time it opened a hole while closing one.
    (
        "env",
        "the environment, the command line and the platform this ran on are properties \
         of the machine rather than of the catalog, and the build-time macro spelling \
         bakes one of them into the binary so that the same source emits different \
         output on two machines. Everything the output depends on arrives through the \
         catalog",
    ),
    (
        "option_env!",
        "the same, and the absent case makes the difference silent",
    ),
    // **The segment, for the reason `env` above is the segment.** This was
    // `process::id`, and a needle carrying `::` can see neither half of
    // `use std::process as machine;` … `machine::id()` — measured, 0 findings —
    // nor the braced import wrapped over lines, which is the form rustfmt
    // itself writes once the group is long enough:
    //
    // ```text
    // use std::process::{
    //     id as pid,
    // };
    // ```
    //
    // The one-line braced form was repaired here and the wrapped one is the
    // same import; `mentions` reads a line at a time, so it saw `process::{`
    // with nothing after it. The bare segment sees every spelling of the
    // import, because `std::process` is the only door to any of this and no
    // alias can be written without naming it once.
    //
    // `process::Command` was a second entry and is this one: it matched only
    // the spelling with the separator in it, and every line it caught names
    // `process` too. Two entries for one line is what `no_needle_is_listed_twice`
    // refuses.
    (
        "process",
        "the identifier of the running program differs on every run by definition, and \
         a child command answers with whatever is installed on this machine rather \
         than with the catalog",
    ),
    // **The clock without its type name.** `SystemTime` and `Instant` are how a
    // clock is usually spelled, and a reviewer got a day-granular stamp past this
    // list twice without writing either: `metadata(p).modified()` returns a
    // `SystemTime` and names nothing, and `.elapsed()` turns one into a duration.
    // A needle list is only as good as its worst-informed entry, so the
    // *operations* are needles too, not only the types.
    (
        "modified",
        "a file's modification time is the machine and the moment the checkout \
         happened, and it is a `SystemTime` without the word in it. Generated output \
         carries no time",
    ),
    (
        "created",
        "a file's creation time, the same by another name",
    ),
    // **And the same three operations under their POSIX names**, which is a
    // hole this list had by construction: `modified`, `created` and `accessed`
    // are `std::fs::Metadata`'s spellings, and `std::os::unix::fs::MetadataExt`
    // offers `mtime`, `ctime` and `atime` for the same three fields — plus
    // `uid`, `gid`, `ino`, `dev` and `nlink`, which are the checkout's own
    // filesystem rather than the catalog. Measured: `mtime()` planted inside
    // `build.rs::artifacts` — the stage that produces the bytes — left
    // `ok 10611 line(s) of the emitter read nothing that changes` and
    // `ok two generations are byte-identical`, because both determinism checks
    // run in one process on one checkout and neither can see a stamp that is
    // the same twice in a row.
    //
    // The needle is the path `std::os`, and it took two tries to get there.
    // **`os::unix` was the first and is wrong**: `std::os::linux::fs::
    // MetadataExt` offers the same three timestamps as `st_mtime` / `st_atime`
    // / `st_ctime`, plus `st_ino`, `st_dev` and `st_uid`, and matches neither
    // that needle nor the bare `mtime` — `mentions` refuses a match whose
    // preceding character is part of a name, and `_` is one. Measured, planted
    // in `build.rs`'s own writing loop: `ok 10692 line(s) of the emitter read
    // nothing that changes` and `ok two generations are byte-identical`, which
    // is the identical transcript the `mtime()` plant produced before any of
    // this existed. `os::fd` is the same gap.
    //
    // **And the bare segment `unix` was tried before that and is also wrong**:
    // `#[cfg(target_family = "unix")]` is an existing, legitimate line in this
    // tree's own fixtures, and the two needles then reported it twice under two
    // reasons — which is what `no_needle_is_listed_twice` refuses, in the other
    // direction. `std::os` is the whole module: every path under it is written
    // once, whatever the import is renamed to, including
    // `use std::os::unix::prelude::*`, which brings the trait in without
    // spelling `MetadataExt`.
    (
        "std::os",
        "the POSIX metadata a file carries on this machine — its three timestamps under \
         their C names, its inode, device, link count and ownership — none of which is \
         a property of the catalog. Generated output carries no time and no filesystem",
    ),
    // And the timestamps by name as well, in both spellings, because the trait
    // can be in scope from a crate that re-exports it and the call still says
    // what it reads. `st_mtime` is a separate needle from `mtime` and not a
    // longer spelling of it: the boundary rule treats `_` as part of a name, so
    // one cannot see the other. A needle list is only as good as its
    // worst-informed entry.
    (
        "st_mtime",
        "a file's modification time under the C name `std::os::linux` gives it. \
         Generated output carries no time",
    ),
    (
        "st_ctime",
        "when a file's metadata last changed, under its C name. Generated output \
         carries no time",
    ),
    (
        "st_atime",
        "when a file was last read, under its C name. Generated output carries no \
         time",
    ),
    (
        "mtime",
        "a file's modification time, which is `modified` under the name libc gives it. \
         Generated output carries no time",
    ),
    (
        "ctime",
        "when a file's metadata last changed, which moves when nothing was written. \
         Generated output carries no time",
    ),
    // **And three machine facts already in this list, in spellings it could not
    // see.** `MAIN_SEPARATOR_STR` is one underscore past the `MAIN_SEPARATOR`
    // needle and `mentions` refuses a match whose next character is part of a
    // name; `std::path::absolute` is the process's working directory, which
    // `current_dir` and `canonicalize` are both needles for, under a third
    // name; and `size_of::<*const u8>()` is the pointer width, which
    // `size_of::<usize>` and `usize::BITS` are both needles for. All three were
    // planted inside `build.rs::artifacts` — the stage that writes the bytes —
    // and the run read three more lines and reported nothing.
    (
        "MAIN_SEPARATOR_STR",
        "the path separator differs by platform, and a generated file that holds one \
         differs with it. A generated path is written with a forward slash",
    ),
    // **And the third spelling of the separator, which asks rather than
    // tells.** `std::path::is_separator(c)` answers `true` for `\\` on Windows
    // and `false` everywhere else, so a generated file whose content depends on
    // it differs by platform exactly as one holding the character does — and
    // `path::absolute` was a needle while `path::is_separator` beside it was
    // not.
    (
        "path::is_separator",
        "whether a character separates path segments is a platform fact, and a \
         generated file that branches on it differs by platform. Compare against \
         a forward slash",
    ),
    (
        "path::absolute",
        "where the process happens to be standing, resolved against it. Paths reaching \
         generated output come from the emitter's own root argument",
    ),
    // **The pointer width as a *value*, not only as a measurement.**
    // `usize::MAX` is `18446744073709551615` on a 64-bit target and
    // `4294967295` on a 32-bit one, and a generated file that holds either
    // differs between them — the same fact `usize::BITS` and `size_of::<usize>`
    // are both needles for, written the way a constant actually gets written.
    // Measured: `pub const WIDEST: usize = usize::MAX;` in the emitter's own
    // crate read three more lines and reported nothing, while `usize::BITS as
    // usize` on the same line was a finding.
    (
        "usize::MAX",
        "the largest address this machine can hold differs by pointer width, and a \
         generated file that holds the number differs with it. Write the number the \
         format needs, or a fixed-width type",
    ),
    (
        "isize::MAX",
        "the same pointer width with a sign on it. A generated file that holds the \
         number differs by target",
    ),
    // **And the two that ask a value for its size.** `mentions` refuses a match
    // whose next character is part of a name, so `size_of` cannot see
    // `size_of_val` and `align_of` cannot see `align_of_val` — the same
    // underscore that hid `st_mtime` from `mtime`.
    (
        "size_of_val",
        "the size of a value is the machine's, and a generated file that holds one \
         differs by target",
    ),
    (
        "align_of_val",
        "alignment is the machine's, and a generated file that holds one differs by \
         target",
    ),
    // A reference is a pointer, and two of them for a slice or a `str`: sixteen
    // bytes on a 64-bit target and eight on a 32-bit one, past both the
    // `size_of::<usize>` and the `size_of::<*` needles.
    (
        "atime",
        "when a file was last read, which changes without anything being written at \
         all. Generated output carries no time",
    ),
    (
        "elapsed",
        "a duration since a clock reading is a clock reading. Written with a leading \
         dot, this could never fire: a needle matches only when the character before \
         it is not part of a name, and the one before the dot of `start.…()` always \
         is. Generated output carries no time",
    ),
    (
        "Utc",
        "a clock, whatever crate it came from and whatever it is renamed to at the `use`",
    ),
    (
        "Local",
        "a clock and a timezone, so the output differs by where the machine is",
    ),
    // **Randomness**, which this module's header promised was a finding and no
    // needle matched.
    (
        "rand",
        "a random number differs on every run by definition. Generated output is a \
         function of the catalog and of nothing else",
    ),
    ("getrandom", "the same, one layer down"),
    (
        "RandomState",
        "the default hasher is seeded per process, which is why `HashMap` order \
         differs between two runs of the same binary. `BTreeMap`",
    ),
    // **A subprocess**, which can answer anything at all.
    // **The import, not only the call.** `Command::new` is a whole name, so
    // `use std::process::Command as Shell;` walks past it and `Shell::new("date")`
    // names nothing on the list — while the needle's own fix text calls `date`
    // "the shortest way to put a clock in a file". A `use` of the module is what
    // an alias cannot hide.
    // **The platform, however it is asked.** `env::consts::OS` is a needle;
    // `cfg!(target_os = …)` is the same fact as a compile-time test, and
    // `MAIN_SEPARATOR` is the same fact as a constant. Each puts the machine
    // that ran the build into the file.
    (
        "cfg!",
        "a compile-time test asks about the platform, the architecture or the pointer \
         width of the machine the emitter was built on, rather than about the catalog",
    ),
    (
        "MAIN_SEPARATOR",
        "the path separator differs by platform, and a generated file that holds one \
         differs with it",
    ),
    // **Every `size_of` whose answer the machine decides, which is most of
    // them.** This carried `usize`, then `*`, then `&` — an enumeration one
    // token short each time, and `size_of::<String>()` (24 bytes on a 64-bit
    // target, 12 on a 32-bit one) and `size_of::<Box<_>>()` walked through both
    // rounds of widening. So the needle is the call and the *exception* is
    // written down: a fixed-width primitive answers the same everywhere, and
    // everything else holds a pointer, a length or a capacity. The exception is
    // this sentence and no test names it: the needle reports every
    // `size_of::<…>` and the reader is told which ones are the machine's. The
    // citation here used to point at `a_size_the_machine_decides`, which is a
    // test no file in this tree has ever held.
    (
        "size_of::<",
        "the size of anything but a fixed-width primitive is the machine's — a \
         pointer, a length or a capacity is in there. Everything the output depends \
         on arrives through the catalog",
    ),
    // **The machine's own files.** `/etc/hostname` is its identity, read through
    // `fs` rather than through any needle above.
    // **And the devices beside them.** `/dev/urandom` is the machine's randomness
    // by another road, and `/dev/` is where every such road starts.
    (
        "\"/dev/",
        "a device is the machine's, and reading one at build time is the machine \
         reaching the output",
    ),
    (
        "thread::current",
        "a thread's identity differs between runs of the same build, so output that \
         holds one differs from itself",
    ),
    // **The process's own input, not a child's.** `stdin` alone also matched
    // `.stdin(Stdio::piped())` and `child.stdin` — the emitter pipes source *into*
    // rustfmt, which is a write and not a read — and reported three lines of the
    // build step that read nothing at all. `io::stdin` is the function that reads
    // what the terminal or the pipe held.
    (
        "io::stdin",
        "input read while building is whatever the terminal or the pipe held, which \
         is not the catalog",
    ),
    (
        // **Not anchored to the opening quote.** Written `"\"/etc/"`, the needle
        // spoke only for a literal that *begins* with the path, so
        // `format!("{}/etc/machine-id", dir)` — the ordinary way a root is
        // joined to one — was invisible to this half as well as to the
        // absolute-path half. `/etc/` cannot be a relative segment of anything
        // in this repository, so the quote was carrying no weight.
        "/etc/",
        "a file under `/etc` describes the machine, not the catalog",
    ),
    (
        "\"/proc/",
        "a file under `/proc` describes this process and this kernel",
    ),
    (
        "\"/sys/",
        "a file under `/sys` describes this machine's hardware",
    ),
    (
        "TcpStream",
        "a socket answers with whatever is on the other end of it, which is not \
         the catalog and is not the same twice",
    ),
    (
        "UdpSocket",
        "a socket answers with whatever is on the other end of it, which is not \
         the catalog and is not the same twice",
    ),
    (
        "Command::new",
        "a subprocess answers with whatever is installed on this machine — `date` \
         being the shortest way to put a clock in a file. Everything the output \
         depends on arrives through the catalog",
    ),
    (
        "read_dir",
        "the filesystem hands entries over in its own order, which differs between two \
         machines and sometimes between two runs. Collect and sort before using them, \
         and mark the line saying where the sort is",
    ),
    // **The platform asked as an attribute rather than as a macro.** `cfg!` is
    // on this list and the attribute spelling is the same question with the same
    // answer — a reviewer put `Platform: linux, 64 bit` into all nine generated
    // files with a `#[cfg(target_os = ...)]`-selected constant and `usize::BITS`,
    // and `cargo xtask check` printed `check passed`. That is verbatim the
    // failure the `env::` entry above says it was written to close, reached by
    // spelling the same needle a second way.
    //
    // The needle is the platform *keys*, not the attribute: `cfg(test)` and
    // `cfg(feature = ...)` are about this crate rather than about the machine
    // and appear in every file, so a needle on the attribute itself would be a
    // marker on six lines that say nothing about determinism — and a marker
    // that has to be written six times for no reason is the exclusion list this
    // module refuses to have.
    (
        "target_os",
        "a `cfg` on the operating system selects code by the machine the emitter was \
         built on, which `cfg!` above is the macro spelling of",
    ),
    ("target_arch", "the same for the architecture"),
    ("target_pointer_width", "the same for the pointer width"),
    (
        "target_endian",
        "the same for byte order, which decides how numbers reach a file",
    ),
    // **The other eight, because the four above were the four somebody was
    // bitten by.** This list's own comment records the incident that made it —
    // `Platform: linux, 64 bit` in all nine generated files, selected by a
    // `#[cfg(target_os = …)]` constant, with `check passed` — and the fix named
    // the key that had done it and its three nearest relatives. Every key below
    // selects code by the machine or by the build profile just as well, and each
    // was measured writing a line into all nine generated files with the whole
    // gate green:
    //
    // ```text
    // //! Built on unix/gnu, profile debug.
    //   ->  ok  15 generated file(s) match the catalog
    //   ->  ok  two generations are byte-identical
    //   ->  ok  7785 line(s) of the emitter read nothing that changes
    //   ->  check passed
    // ```
    //
    // `unix` and `windows` take no `=`, which is why they are listed as bare
    // words and why the needle is the key rather than the comparison.
    (
        "target_family",
        "the same for the family, of which `unix` is one value",
    ),
    (
        "target_env",
        "the C environment — `gnu` and `musl` produce different binaries",
    ),
    (
        "target_vendor",
        "the vendor triple field, which differs between distributions",
    ),
    (
        "target_feature",
        "the instruction set the emitter was compiled for",
    ),
    ("target_abi", "the ABI, which is part of the same triple"),
    (
        "debug_assertions",
        "the build profile: a value selected by it differs between `cargo build` and \
         `--release`, so two generations on one machine can disagree",
    ),
    ("overflow_checks", "the same, for arithmetic behaviour"),
    (
        "panic = ",
        "`abort` or `unwind`, which is a profile setting and reaches code through `cfg`. \
         Written with its `=` because the bare word is `panic!`, which is not this",
    ),
    // **`unix` and `windows` are not here, and the rule about them is not a
    // needle.** They were written `cfg(unix` and `cfg(windows`, which
    // `#[cfg(any(unix, windows))]` does not contain — measured, four such lines
    // added to a crate the emitter reads gave `ok 7840 line(s) of the emitter
    // read nothing that changes` against a baseline of 7836, and `check
    // passed`. The same hole covered `not(windows)` and `all(unix, …)`. And the
    // bare words cannot replace them: `windows` is `slice::windows`, which this
    // tree uses twice in the catalog loader, so the needle reported the loader
    // for iterating a slice.
    //
    // A predicate is not a substring. `conditional_compilation` below reads the
    // whole attribute instead — `conditional_on` hands it the predicate, from
    // either `cfg` or `cfg_attr` — which is the shape rather than any spelling
    // of it.
    //
    // **This sentence named `no_conditional_compilation_but_test`, and no such
    // function has ever existed anywhere in this tree.** A comment naming the
    // test that decides a rule is what a reader checks the rule by, and this one
    // sent them looking for nothing — while the rule was in fact decided by
    // nothing: `code.find("#[cfg(")` misspelt to `code.find("#[cfgZ(")` left the
    // emitter's line count unmoved, all 353 tests in `xtask` green and `check
    // passed`. `a_cfg_that_is_not_test_is_a_finding` is the test now, and it
    // exists.
    (
        "usize::BITS",
        "the pointer width of the machine that built this, which `size_of::<usize>` \
         above is the other spelling of",
    ),
    (
        "isize::BITS",
        "the same pointer width with a sign on it, and one token from the needle \
         above",
    ),
    (
        "align_of",
        "an alignment is a property of the target, and it differs between \
         architectures the same way a pointer width does",
    ),
    (
        "{:p}",
        "an address is where this run happened to put something, and it differs \
         between two runs of one binary",
    ),
    (
        "canonicalize",
        "the absolute path of a file on this disk, which is where the checkout \
         happens to be. Paths reaching generated output come from the emitter's own \
         root argument",
    ),
    (
        "IsTerminal",
        "whether a terminal is attached differs between a developer's shell and CI, \
         and output that changes with it is output nobody can compare",
    ),
    ("is_terminal", "the same question as a method call"),
];

/// The marker that says a line is deterministic for a reason worth reading.
pub(crate) const ALLOW: &str = "allow-nondeterminism:";

/// Reads the emitting crates and reports everything that could differ.
///
/// Returns the findings and how many lines were read, so a walk that stopped
/// descending is visible rather than green.
///
/// # Errors
///
/// Returns why the sources could not be read.
pub(crate) fn nothing_the_emitter_reads_changes(
    root: &Path,
) -> Result<(Vec<Finding>, usize), String> {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    for named in EMITS {
        let here = root.join(named);
        // **A file as well as a directory.** The last stage of generation is one
        // file — `xtask/src/build.rs`, which runs rustfmt and writes what the
        // emitter produced — and it is not under any of the emitter's own source
        // directories, because no dependency edge leads from the emitter to the
        // crate that calls it.
        let mut files = Vec::new();
        if here.is_file() {
            files.push(here.clone());
        } else if here.is_dir() {
            walk(&here, &mut |path| {
                if path.extension().is_some_and(|kind| kind == "rs") {
                    files.push(path.to_path_buf());
                }
            })?;
        } else {
            return Err(format!(
                "{named} is neither a file nor a directory, so this gate inspected nothing \
                 where the emitter is supposed to be"
            ));
        }
        files.sort();
        for path in files {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
            let relative = relative_to(root, &path);
            // Two counters, for the reason `gates.rs` carries on the walks
            // beside this one: the first says a line became this gate's to
            // judge, the second is the last statement a judged line reaches,
            // and a filter written between them makes them disagree.
            let mut entered = 0_usize;
            let mut judged = 0_usize;
            // **The lines, kept, because an attribute is not a line.** rustfmt
            // *rewrites* a long one-line `#[cfg(all(…))]` into a wrapped one, so
            // the rule below is handed the lines under each one and joins them
            // until the predicate closes.
            let all: Vec<&str> = text.lines().collect();
            // **The whole file through the stripper, not each line on its own.**
            // A string literal that opens on one line and closes three below is
            // in no literal when its middle line is read alone — so an assertion
            // message whose continuation line names `read_dir` was reported as a
            // call. `wall.rs` carries the same sentence for the same reason.
            // The line count is preserved, so the two walks stay in step.
            let spent = crate::gates::blanked(&text);
            let without_literals: Vec<&str> = spent.lines().collect();
            // **And the whole file through the comment stripper too.** The five
            // needles routed to the line *as written* were judged against a
            // text in which a `/* … */` block is still code: both copies of the
            // wave-11 repair call `without_comments` on **one line at a time**,
            // so they know only the `//` spelling, while `blanked` beside them
            // spends block comments character for character. One rule written
            // twice inside one walk, with the second copy knowing one of the
            // two comment forms. Measured by a reviewer in wave 17. The line
            // count is preserved for the same reason it is above.
            let spoken = crate::gates::without_comments(&text, "//");
            let without_comments: Vec<&str> = spoken.lines().collect();
            for (index, line) in all.iter().enumerate() {
                read += 1;
                entered += 1;
                // A doc comment naming one of these is prose about the rule, not
                // a use of it — and this module's own header names eight.
                let code = line.trim_start();
                if code.starts_with("//") {
                    judged += 1;
                    continue;
                }
                // Rust, so `#` opens no comment here: `r#"…"#` is a raw string
                // and `#[derive]` an attribute, and both excused their line.
                let excused =
                    crate::references::marked_in(line, ALLOW, crate::references::Comments::Slashes);
                conditional_compilation(
                    (code, without_literals.get(index).copied().unwrap_or(code)),
                    (
                        all.get(index + 1..).unwrap_or_default(),
                        without_literals.get(index + 1..).unwrap_or_default(),
                    ),
                    line,
                    &relative,
                    index,
                    excused,
                    &mut findings,
                );
                what_this_line_reads(
                    without_comments.get(index).copied().unwrap_or(code),
                    without_literals.get(index).copied().unwrap_or(code),
                    line,
                    excused,
                    &relative,
                    index,
                    &mut findings,
                );
                findings.extend(the_absolute_path_in(
                    without_comments.get(index).copied().unwrap_or(code),
                    line,
                    excused,
                    &relative,
                    index,
                ));
                judged += 1;
            }
            crate::gates::everything_reached_a_verdict(
                "line",
                &relative,
                entered,
                judged,
                &mut findings,
            );
        }
    }
    Ok((findings, read))
}

/// Every workspace crate one manifest depends on, in either TOML spelling.
///
/// `nc-x = { … }` under `[dependencies]`, and `[dependencies.nc-x]` as a section
/// of its own. `[dev-dependencies]` is deliberately not one: a test's clock
/// reaches no generated file.
#[cfg(test)]
fn workspace_dependencies(manifest: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            inside = section == "dependencies";
            if let Some(name) = section.strip_prefix("dependencies.")
                && name.starts_with("nc-")
            {
                found.push(name.to_owned());
            }
            continue;
        }
        if inside
            && let Some(name) = line.split(['=', ' ', '.']).next()
            && name.starts_with("nc-")
        {
            found.push(name.to_owned());
        }
    }
    found.dedup();
    found
}

/// Reports a line compiled under any predicate but `test`.
///
/// In code that produces committed output the only `cfg` that cannot change
/// what is produced is `test` — every other predicate selects by the machine or
/// by the build profile, and `#[cfg(debug_assertions)]` makes two builds on
/// *one* machine disagree. Lifted out of its caller only because the two
/// together cross clippy's line limit.
fn conditional_compilation(
    here: (&str, &str),
    below: (&[&str], &[&str]),
    line: &str,
    relative: &str,
    index: usize,
    excused: bool,
    findings: &mut Vec<Finding>,
) {
    // **Conditional compilation, as a predicate rather than a
    // spelling.** In code that produces committed output the only
    // `cfg` that cannot change what is produced is `test` — every
    // other predicate selects by the machine or by the build
    // profile, and `#[cfg(debug_assertions)]` makes two builds on
    // *one* machine disagree. There are no non-`test` ones in this
    // scope today, so the rule costs nothing to hold.
    //
    // **Decided in the code and named from the line as written.** Each pair is
    // `(as written, with literals and comments spent)`. This rule was handed
    // the raw line while the two rules beside it in the same walk were handed
    // the whole-file stripped texts — one walk, three readers, and the third
    // still asking `find("#[cfg(")` of text in which a comment and a string
    // literal are both code. So it held "those six characters appear nowhere on
    // the line except after a leading `//`": a trailing comment and a literal
    // were read as attributes, and the emitter this rule guards writes
    // attribute text as literals twenty-six times in `emit.rs` alone. That is
    // the too-strong direction — legitimate prose refused, with a fix line
    // telling its author to "write it unconditionally".
    //
    // The blanked text cannot *name* the predicate, because it spends the
    // contents of `#[cfg(feature = "probe")]` along with everything else, and
    // the incident this rule was written for is a `#[cfg(target_os = "linux")]`
    // constant. So the finding is decided on the spent text and worded from the
    // written one. Found by a reviewer in wave 19, and the naming half by the
    // skeptic who measured the repair.
    //
    // **And the fallback is reachable, which is why it is written rather than
    // argued.** `#[cfg(feature = "a(b")]` is the line: in the text as written
    // the parenthesis inside the literal raises the depth, so the attribute
    // never closes and that walk runs past its wrap bound to `None`; in the
    // spent text it is a space and the attribute closes. The finding is then
    // worded from the spent predicate — the value gone, the rule still firing —
    // which is the direction to fail in. A clause whose reachability is argued
    // instead of shown is what wave 15 deleted five of and waves 16 to 19
    // reached three.
    //
    // **And the question is asked of the spent text, not merely opened there.**
    // Wave 19 wrote the paragraph above and then compared the *written*
    // predicate against `test` — so the sentence claimed one thing and the line
    // did another, on a repair whose whole subject was which text a rule reads.
    // `conditional_on` takes the first of the four openers by byte offset, so a
    // comment or a literal spelling `#[cfg(test)]` ahead of a real
    // `#[cfg(unix)]` answered `test` from the written walk and `unix` from the
    // spent one, and the rule stood down for a genuinely conditional line. The
    // mirror was live too: `/* #[cfg(unix)] */ #[cfg(test)]` is a `test`-only
    // line that was refused. Found by a reviewer in wave 20, and the mirror by
    // the skeptic who measured the repair.
    // **And the wording is read at the offset the decision was made at.** The
    // two walks each take the *first* of the four openers by byte offset, so
    // they need not be looking at the same attribute: a commented-out
    // `#[cfg(test)]` in front of a live `#[cfg(unix)]` made the written walk
    // answer `test` while the spent one answered `unix` — the gate fired,
    // correctly, and then said "this is compiled only when `test`" over a fix
    // line reading "`test` is the one predicate that cannot reach the output".
    // The sentence and its own remedy contradicted each other, and the reader
    // was sent to the attribute in the comment. `blanked` preserves the
    // character count, so the offset the spent text decided at is the offset
    // the written text is read from. Found by a reviewer in wave 21.
    let Some((at, spent)) = conditional_on_at(here.1, below.1) else {
        return;
    };
    let predicate = the_predicate_at(here.0, below.0, at).unwrap_or_else(|| spent.clone());
    if spent != "test"
        && !(excused
            && crate::references::reason_after(line, ALLOW, crate::references::Comments::Slashes)
                .is_some_and(|reason| crate::references::names(reason, "cfg")))
    {
        findings.push(Finding {
            file: relative.to_owned(),
            line: index + 1,
            what: format!("this is compiled only when `{predicate}`"),
            fix: "the bytes this crate writes are committed, so anything that \
                  selects code by the machine or the build profile makes two \
                  checkouts disagree about a file both of them claim is \
                  generated. `test` is the one predicate that cannot reach \
                  the output. Write it unconditionally, or mark the line \
                  `allow-nondeterminism: <reason>` naming `cfg`"
                .to_owned(),
        });
    }
}

/// The predicate of a `cfg` attribute opening on this line, if it carries one.
///
/// `#[cfg(test)]` gives `test`; `#[cfg(any(unix, windows))]` gives
/// `any(unix, windows)`. `cfg_attr` is read the same way, because its first
/// argument is the same predicate, and the inner spellings `#![cfg(…)]` and
/// `#![cfg_attr(…)]` are the same attribute written from inside the item.
/// Returns `None` for a line with no `cfg` attribute — including `cfg!(…)`,
/// which is a macro the needle list already names.
///
/// **An attribute is not a line, and this read one.** It did `code.find("#[cfg(")`
/// and matched parentheses inside that same string, so a wrapped attribute —
///
/// ```text
/// #[cfg(all(
///     unix,
///     not(windows)
/// ))]
/// ```
///
/// — returned `None` and the whole rule stepped over it. Measured: five such
/// lines in `crates/nc-canon/src/lib.rs` left `check passed`, with the output
/// differing from a clean run only in the four line counters. `#![cfg(unix)]`
/// was missed the same way, by spelling. Neither is exotic: **rustfmt rewrites**
/// a long one-line `cfg` into the wrapped form, and `xtask/src/main.rs` and
/// `crates/nc-log/src/ship.rs` already carry multi-line attributes — so the rule
/// this module's needle list calls "the shape rather than any spelling of it"
/// could be switched off by running the formatter.
///
/// So the lines under the opening one are handed in, and joined until the
/// parentheses close. Bounded, because an unbalanced `(` inside a string
/// literal would otherwise join the rest of the file to every line above it.
fn conditional_on(here: &str, below: &[&str]) -> Option<String> {
    conditional_on_at(here, below).map(|(_, predicate)| predicate)
}

/// The predicate of the `cfg` attribute opening at `from`, if one opens there.
///
/// The offset is the caller's, not this reader's: `conditional_compilation`
/// decides on the text with comments and literals spent and words the finding
/// from the text as written, and the two must be talking about the **same**
/// attribute. Both walks take the first opener by offset, so a commented-out
/// `cfg` in front of a live one put them on different ones. `blanked` preserves
/// the character count, which is what makes one walk's offset meaningful to the
/// other.
fn the_predicate_at(here: &str, below: &[&str], from: usize) -> Option<String> {
    let at = here.char_indices().nth(from).map(|(byte, _)| byte)?;
    conditional_on(here.get(at..)?, below)
}

/// Where a `cfg` attribute opens on this line, as a character offset, with its
/// predicate.
fn conditional_on_at(here: &str, below: &[&str]) -> Option<(usize, String)> {
    /// How many lines an attribute may wrap onto before this stops reading.
    ///
    /// rustfmt breaks a predicate one term to a line, so nothing in this tree
    /// is near this. The bound is here because an unbalanced `(` inside a
    /// string literal would otherwise join the rest of the file to every line
    /// above it.
    const WRAPS_ONTO: usize = 64;

    let (at, opener) = ["#[cfg(", "#[cfg_attr(", "#![cfg(", "#![cfg_attr("]
        .into_iter()
        .filter_map(|opener| here.find(opener).map(|at| (at, opener)))
        .min_by_key(|&(at, _)| at)?;
    // The caller indexes the *other* text with this, and `blanked` preserves
    // characters rather than bytes.
    let opens = here[..at].chars().count();
    let mut whole = here[at..].to_owned();
    let mut wrapped = below.iter().take(WRAPS_ONTO);
    while still_open(&whole) > 0 {
        let Some(next) = wrapped.next() else {
            break;
        };
        whole.push(' ');
        whole.push_str(next.trim());
    }
    let bracket = whole.find('(')?;
    let mut depth = 0_usize;
    for (offset, character) in whole[bracket..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let predicate = &whole[bracket + 1..bracket + offset];
                    // `cfg_attr`'s predicate is its first argument, and the
                    // attribute it applies is the rest.
                    let predicate = predicate
                        .split_once(", ")
                        .filter(|_| opener.ends_with("cfg_attr("))
                        .map_or(predicate, |(predicate, _)| predicate);
                    // Written back as one line, because a predicate joined out
                    // of four is otherwise reported with the wrapping still in
                    // it — and the finding names the predicate.
                    return Some((
                        opens,
                        predicate
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .replace("( ", "(")
                            .replace(" )", ")")
                            .replace(" ,", ","),
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

/// How many parentheses `text` leaves open.
///
/// The question [`conditional_on`] asks of a line to decide whether the
/// attribute it opened is finished or wraps onto the next one.
fn still_open(text: &str) -> usize {
    let mut depth = 0_usize;
    for character in text.chars() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// The finding an absolute path makes, when no needle already made one.
///
/// **The shape those four roots are instances of.** `CHANGES` names `/dev/`,
/// `/etc/`, `/proc/` and `/sys/`, and the prose beside them says "the machine's
/// own files" — but `/etc/machine-id` is a symlink to
/// `/var/lib/dbus/machine-id`, one directory outside the list, and reading
/// *that* changed the generated crate with `two generations are byte-identical`,
/// `10824 line(s) of the emitter read nothing that changes` and `check passed`
/// all green. The other two mechanisms cannot see it by construction: both
/// generations run on one machine.
///
/// The caller asks it only where no needle spoke, so the four roots keep their reasons
/// and a line is reported once. The emitter's own `"//` and `"///` — the doc
/// comments of the crate it writes — are not paths, which is what the character
/// after the slash decides.
fn the_absolute_path_in(
    code: &str,
    line: &str,
    excused: bool,
    relative: &str,
    index: usize,
) -> Option<Finding> {
    // **A trailing comment is not code here either.** `mentions` strips one and
    // carries the paragraph saying why — *prose about the file after code on the
    // same line became a finding* — and this half was written later without it.
    // One rule written twice with the second copy narrower is a shape this
    // repository has produced before; this is its too-strong direction, which is
    // the one that is green until something legitimate arrives. Measured: a
    // trailing comment saying that a line opens **no** machine file was reported
    // as an absolute path reaching generated output.
    if !names_an_absolute_path(&crate::gates::without_comments(code, "//")) {
        return None;
    }
    let excused_here = excused
        && crate::references::reason_after(line, ALLOW, crate::references::Comments::Slashes)
            .is_some_and(|reason| crate::references::names(reason, "absolute"));
    (!excused_here).then(|| Finding {
        file: relative.to_owned(),
        line: index + 1,
        what: "an absolute path reaches generated output".to_owned(),
        fix: THE_ABSOLUTE_PATH.to_owned(),
    })
}

/// What to do about an absolute path, written once.
const THE_ABSOLUTE_PATH: &str = "an absolute path is a file on this machine rather than in this \
     repository, so what it holds is the machine reaching the generated output. Read it through \
     the catalog, or mark the line `allow-nondeterminism: <reason>` naming `absolute` and saying \
     why this one cannot differ between machines";

/// Whether a line names an absolute path.
///
/// A literal that opens with `/` followed by anything but another `/` is a path
/// on this machine. The emitter writes `"//"` and `"///"` — the doc comments of
/// the crate it generates — and those are not paths, which is the whole of the
/// difference.
fn names_an_absolute_path(code: &str) -> bool {
    let characters: Vec<char> = code.chars().collect();
    // **A doubled slash is a path, not a comment.** The `!= '/'` was written to
    // keep the emitter's own generated `//` and `///` doc comments out of this
    // rule, and it hid every absolute path spelled with one slash too many —
    // POSIX resolves `//x` and `///x` to `/x`, so
    // `"//var/lib/dbus/machine-id"` reads the file this rule's own paragraph
    // names as the reason it exists, and `check passed`. What tells a comment
    // from a path is not how many slashes there are but what follows them: a
    // comment has a space, a `!` or an escape, and a path has a *name*. Written
    // the other way round — "a path is anything that is not a space" — the
    // emitter's own `"///\n"` and `"//!"` were read as paths, so the test is on
    // what a path segment is made of rather than on what a comment is not.
    let absolute = characters.windows(2).enumerate().any(|(at, window)| {
        if window[0] != '"' || window[1] != '/' {
            return false;
        }
        let mut end = at + 1;
        while characters.get(end) == Some(&'/') {
            end += 1;
        }
        characters.get(end).is_some_and(|after| {
            // **And `{`, which is how a path is interpolated.** The rule
            // read a *name* after the slashes, and in a format string what
            // follows is a placeholder: `format!("/{}/machine-id", dir)` and
            // `format!("/{dir}/machine-id")` open the file this rule exists
            // for, and neither was seen.
            after.is_alphanumeric() || matches!(after, '.' | '_' | '-' | '~' | '{')
        })
    });
    // **And a path that climbs out of the repository reaches the same files.**
    // The rule generalised the four named roots in one direction only:
    // `"../../../etc/machine-id"` is the very file the paragraph above names as
    // the reason this exists, and it was not seen.
    //
    // **A segment, not an opening.** Written as "the literal begins `..`", it
    // read `"../../../etc/machine-id"` and walked past `"./../../../etc/machine-id"`
    // and `"src/../../../etc/machine-id"`, which open the same file — one
    // character and one segment in front of the climb, and `check passed` both
    // times. A `..` is a path segment wherever it is, so it is bounded the way a
    // segment is: by a separator or by the end of the literal. That is also
    // narrower in the direction that matters — a literal beginning `"..and then"`
    // is prose and used to be a finding.
    let climbs = characters.windows(4).any(|window| {
        (window[0] == '"' || window[0] == '/')
            && window[1] == '.'
            && window[2] == '.'
            && (window[3] == '/' || window[3] == '"')
    });
    absolute || climbs
}

/// What one line of the emitter reads that can differ between runs.
///
/// Its own function because the walk that calls it is at clippy's line limit,
/// which is the same reason its neighbours in this file are functions. `code` is
/// the line as written and `spent` the same line with every string literal's
/// characters spent: a needle that is a name is looked for in the second, and a
/// needle that is a path or a format — `/etc/`, `{:p}` — in the first, because
/// what those describe *is* a literal.
fn what_this_line_reads(
    code: &str,
    spent: &str,
    line: &str,
    excused: bool,
    relative: &str,
    index: usize,
    findings: &mut Vec<Finding>,
) {
    for &(needle, instead) in CHANGES {
        // A name is looked for in the code with every literal spent;
        // a path or a format is what a literal holds, and is looked
        // for in the line as written. `mentions` decides which by
        // the needle's own spelling.
        //
        // **`'"'` was a third character here and no needle could decide it.**
        // All sixty-eight of `CHANGES` were classified in wave 15: the only
        // three holding a quote are `"/dev/`, `"/proc/` and `"/sys/`, and each
        // holds a slash as well, so the two spellings of this test answered the
        // same on every input this tree can produce. A clause nothing can
        // decide is a clause that is not there; `MUST_STAY` pins the needle
        // list in both directions, so a needle that would need it back cannot
        // arrive without a person editing that list.
        let read_in = if needle.contains(['/', '{']) {
            code
        } else {
            spent
        };
        if !mentions(read_in, needle) {
            continue;
        }
        // **And the reason names the needle.** The marker was tested
        // once, before the loop, so one comment excused all thirty
        // needles on its line — and the two live markers in the
        // byte-producing stage sit on a `Command::new("rustfmt")`
        // builder chain, where anything appended to the line lands
        // permanently unjudged. The rule `allow-logging:` and
        // `allow-count:` already follow, after the same measurement.
        if excused
            && crate::references::reason_after(line, ALLOW, crate::references::Comments::Slashes)
                .is_some_and(|reason| crate::references::names(reason, needle))
        {
            continue;
        }
        if excused {
            findings.push(Finding {
                file: relative.to_owned(),
                line: index + 1,
                what: format!("this line is excused and the reason does not name `{needle}`"),
                fix: format!(
                    "a marker that names nothing excuses whatever else lands on \
                             its line, and `cargo fmt` moves things onto lines. Say \
                             `{ALLOW} …{needle}…` so an excuse that arrived by \
                             reformatting fails rather than widens"
                ),
            });
            continue;
        }
        findings.push(Finding {
            file: relative.to_owned(),
            line: index + 1,
            what: format!("`{needle}` reaches generated output"),
            fix: format!(
                "{instead} — or mark the line `{ALLOW} <reason>` if this one \
                         cannot differ between runs and the reason is worth reading"
            ),
        });
    }
    // **Asked on every line, which it was not.** This sat under
    // `if !named_a_needle`, so any line naming a needle — and
    // excused with a reason that names it, which is what the hatch
    // is for — was permanently outside the absolute-path and
    // path-climb rule. Measured on 2026-09-08 on the emitter's own
    // locale loader, whose `read_dir` carries a written excuse of
    // its own: the argument changed from
    // `&directory` to `directory.join("../i18n")` gave
    // `check passed` and `determinism:: 15 passed`, while the
    // identical climb three lines up — on a line with no needle —
    // is reported at once.
    //
    // The two rules are about different things: a needle is a call
    // that differs between *runs*, and an absolute path is a machine
    // this output would carry to another. A hatch written for the
    // first switched off the second, on the eight excused lines this
    // tree has. Both other determinism mechanisms are blind to it by
    // construction — they run two generations in one process on one
    // checkout.
}

/// Whether `code` uses `needle` as a name rather than as part of a longer one.
///
/// `env!` is a substring of `option_env!`, so a line using the second was two
/// findings — and a gate that reports the same defect twice under two names is
/// one a reader learns to skim.
fn mentions(code: &str, needle: &str) -> bool {
    // **A trailing comment is not code.** The `/etc/` needle lost its opening
    // quote so an interpolated root would be seen, and prose about the file
    // after code on the same line became a finding — while the identical
    // sentence about `/proc/`, whose needle kept its quote, did not. The line
    // walk above skips a line that *begins* with `//`; this is the one that has
    // to skip the rest of one. Every other needle here is a name or a quoted
    // path, so stripping comments costs them nothing.
    let code = &crate::gates::without_comments(code, "//");
    // **And a name inside a string literal is not a call.** Comments were
    // stripped and literals were not, so an assertion message explaining what a
    // sort is for — *"leaves the rest wherever `read_dir` put them"* — was
    // reported as a `read_dir` reaching generated output. Found by this
    // repository's own acceptance in round seventy-one wave 11, on a message
    // written one repair earlier.
    //
    // **Only for the needles that are names.** Several of them are *about* a
    // literal and live inside one by construction — `/etc/`, `/dev/`, `"/proc/`,
    // `"/sys/`, `{:p}` — and blanking literals for those would make them match
    // nothing at all; `/etc/` lost its opening quote on purpose, so that an
    // interpolated root would be seen. What separates the two kinds is whether
    // the needle is spelled as Rust names it: a path or a format holds a `/`, a
    // `{` or a quote, and a name holds none of them.
    //
    // So the text a needle is searched in is chosen by the needle. `blanked`
    // spends a literal's characters; a path needle reads the line as written.

    let part_of_a_name = |c: char| c.is_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = code[from..].find(needle) {
        let here = from + at;
        from = here + needle.len();
        let before = code[..here].chars().next_back().is_some_and(part_of_a_name);
        // **And after**, which the first version did not check: `env::vars` and
        // `env::var_os` both begin with `env::var`, so a line using either was
        // two findings under two names — and `created` would have matched
        // `created_tables`. A gate that reports the same defect twice is one a
        // reader learns to skim.
        // A needle ending in punctuation has already said where it ends, so the
        // character after it decides nothing: `env::` is the module and
        // `env::consts` is inside it, and `"/etc/` is a directory and
        // `"/etc/hostname` is a file in it. Written as "does the needle end in a
        // name character" rather than as a list of the punctuation marks used so
        // far, because that list is what needed adding to the second time.
        let ends_in_a_name = needle.chars().next_back().is_some_and(part_of_a_name);
        let after = ends_in_a_name && code[from..].chars().next().is_some_and(part_of_a_name);
        if !before && !after {
            return true;
        }
    }
    // **And the braced import, which is the same path written another way.**
    // `use std::process::{id as pid};` is `process::id` and the needle did not
    // match it — measured, 0 findings, with `pid()` then used freely below.
    // `a::b` and `a::{…, b, …}` are one import to rustc, so they are one needle
    // here.
    if let Some((path, name)) = needle.rsplit_once("::") {
        let opens = format!("{path}::{{");
        let mut from = 0;
        while let Some(at) = code[from..].find(&opens) {
            let group = from + at + opens.len();
            from = group;
            let closes = code[group..]
                .find('}')
                .map_or(code.len(), |end| group + end);
            if mentions(&code[group..closes], name) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        ALLOW, ALSO, CHANGES, EMITS, EMITTER, mentions, nothing_the_emitter_reads_changes,
        workspace_dependencies,
    };
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    #[test]
    fn the_emitter_reads_nothing_that_changes() {
        let (findings, read) = nothing_the_emitter_reads_changes(root()).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        assert!(read > 0, "no line was read, so this proved nothing");
    }

    #[test]
    fn every_shape_is_a_finding_and_the_marker_excuses_it() {
        // Fired on purpose, one needle at a time: a list of twelve where one
        // never matches is eleven rules and a decoration, and nothing would say
        // which. `§19.3`.
        let scratch = std::env::temp_dir().join(format!("nc-determinism-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        for directory in EMITS {
            std::fs::create_dir_all(scratch.join(directory)).expect("writable");
        }
        let source = scratch.join(EMITS[0]).join("emit.rs");

        // **A name inside a string literal is prose, and the literal may span
        // lines.** Measured in round seventy-one wave 11 by this repository's own
        // acceptance: an assertion message explaining what a sort is for — its
        // continuation line naming `read_dir` — was reported as a `read_dir`
        // reaching generated output, because comments were stripped and literals
        // were not, and because each line was read alone. Both halves are here:
        // the name on one line of a literal, and on the second line of one.
        for written in [
            "fn f() {{ assert!(true, \"sorted, so no read_dir order leaks\"); }}\n",
            "fn f() {{\n    assert!(\n        true,\n        \"sorted, so no \\\n         read_dir order leaks\"\n    );\n}}\n",
        ] {
            std::fs::write(&source, written.replace("{{", "{").replace("}}", "}"))
                .expect("writable");
            let (findings, _) = nothing_the_emitter_reads_changes(&scratch).expect("readable");
            assert!(
                findings.is_empty(),
                "a name inside a string literal is prose, not a call: {findings:#?}"
            );
        }

        // **And the needles that live inside a literal, written as they really
        // are.** The classification sends a needle holding `/`, `{` or `"` to
        // the line as written and every other to the blanked copy — and only
        // the `"` member was decided by anything, because the corpus below
        // writes each needle as bare code, where both texts hold it. In real
        // emitter source `/etc/` and `{:p}` can only ever appear *inside* a
        // literal, so reading them in the blanked copy would make those rules
        // dead and nothing would say so. Named by a reviewer in wave 12.
        for (written, needle) in [
            (
                "fn f() { let p = read_to_string(\"/etc/hostname\"); }\n",
                "/etc/",
            ),
            ("fn f() { let p = format!(\"{:p}\", &x); }\n", "{:p}"),
        ] {
            std::fs::write(&source, written).expect("writable");
            let (findings, _) = nothing_the_emitter_reads_changes(&scratch).expect("readable");
            assert!(
                findings.iter().any(|finding| finding.what.contains(needle)),
                "`{needle}` inside a literal is what that needle is about, and it was not \
                 a finding: {findings:#?}"
            );
        }

        for &(needle, _) in CHANGES {
            // **A line of Rust, quotes and all.** Some needles are the opening
            // of a string literal — `"/etc/` is one — so writing them followed
            // by `();` left the line with an unterminated quote. That is not
            // source anybody would write, and `references::marked` now reads
            // whether the marker is inside a string: on a line whose string
            // never closes, the trailing `//` is inside it, and the fixture was
            // asserting the marker does not excuse what it does excuse in real
            // source.
            let tail = if needle.starts_with('"') {
                "\";"
            } else {
                "();"
            };
            std::fs::write(&source, format!("fn f() {{ let x = {needle}{tail} }}\n"))
                .expect("writable");
            let (findings, _) = nothing_the_emitter_reads_changes(&scratch).expect("readable");
            // **At least one, because two rules now read every line.** The
            // absolute-path rule used to be asked only where no needle matched,
            // which is the hole a reviewer measured: a line naming a needle and
            // carrying a written excuse for it was outside that rule for good.
            // Both rules are asked now, so a needle that *is* an absolute path —
            // `"/dev/`, `"/etc/`, `"/proc/` — is two findings about two
            // different things, and the assertion is that the needle produced
            // one rather than that the line produced only one.
            assert!(
                findings.iter().any(|finding| finding.what.contains(needle)),
                "`{needle}` in the emitter was not a finding: {findings:#?}"
            );

            // The same line, marked **and naming what it excuses** — because a
            // marker tested once before the needle loop excused all thirty
            // needles on its line, and `cargo fmt` puts things on lines.
            std::fs::write(
                &source,
                format!(
                    "fn f() {{ let x = {needle}{tail} }} // {ALLOW} this `{needle}` is \
                     sorted before use\n"
                ),
            )
            .expect("writable");
            let (findings, _) = nothing_the_emitter_reads_changes(&scratch).expect("readable");
            assert!(
                !findings.iter().any(|finding| finding.what.contains(needle)),
                "a marked `{needle}` was reported: {findings:#?}"
            );

            // And a marker naming something else does not excuse this one.
            std::fs::write(
                &source,
                format!("fn f() {{ let x = {needle}{tail} }} // {ALLOW} about something else\n"),
            )
            .expect("writable");
            let (findings, _) = nothing_the_emitter_reads_changes(&scratch).expect("readable");
            assert!(
                findings.iter().any(|finding| finding.what.contains(needle)),
                "a marker naming nothing excused `{needle}`: {findings:#?}"
            );

            std::fs::write(
                &source,
                format!("fn f() {{ let x = {needle}{tail} }} // {ALLOW}\n"),
            )
            .expect("writable");
            let (findings, _) = nothing_the_emitter_reads_changes(&scratch).expect("readable");
            assert!(
                findings.iter().any(|finding| finding.what.contains(needle)),
                "a bare marker with no reason excused `{needle}`: {findings:#?}"
            );
        }

        // Prose naming the rule is not a use of it, or this module's own header
        // would be twelve findings.
        std::fs::write(&source, "// SystemTime is not read here.\nfn f() {}\n").expect("writable");
        let (findings, _) = nothing_the_emitter_reads_changes(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");

        // And a missing emit directory is an error, not an empty pass.
        std::fs::remove_dir_all(scratch.join(EMITS[1])).expect("writable");
        let problem = nothing_the_emitter_reads_changes(&scratch).expect_err("inspected nothing");
        assert!(problem.contains(EMITS[1]), "{problem}");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A clock read under its POSIX name is a finding.
    ///
    /// **The three `std::fs::Metadata` spellings were needles and their
    /// `MetadataExt` twins were not**, which is a list covering an operation by
    /// one of its names. Measured, planted inside the stage that produces the
    /// bytes: `mtime()` left `ok 10611 line(s) of the emitter read nothing that
    /// changes` and `ok two generations are byte-identical`, because both
    /// determinism checks run in one process on one checkout and a stamp that
    /// is the same twice in a row is invisible to each. The only thing that
    /// would have caught it is CI's second-checkout diff, in a job whose own
    /// header says it has never run.
    #[test]
    fn a_clock_under_its_posix_name_is_a_finding() {
        let scratch = std::env::temp_dir().join(format!("nc-posix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        for directory in EMITS {
            std::fs::create_dir_all(scratch.join(directory)).expect("writable");
        }
        let source = scratch.join(EMITS[0]).join("emit.rs");
        let judged = |body: &str| {
            std::fs::write(&source, body).expect("writable");
            nothing_the_emitter_reads_changes(&scratch)
                .expect("readable")
                .0
        };
        for planted in [
            "fn stamp(m: &std::fs::Metadata) -> i64 { m.mtime() }\n",
            "fn stamp(m: &std::fs::Metadata) -> i64 { m.ctime() }\n",
            "fn stamp(m: &std::fs::Metadata) -> i64 { m.atime() }\n",
            // The import, which is what `.uid()`, `.ino()` and `.dev()` need
            // and what no per-method needle would ever enumerate.
            "use std::os::unix::fs::MetadataExt;\n",
            // **And the other family under the same module**, which `os::unix`
            // could not see: `std::os::linux::fs::MetadataExt` offers the same
            // fields under their C names, and `st_mtime` is not a longer
            // spelling of `mtime` to a reader that treats `_` as part of a name.
            "use std::os::linux::fs::MetadataExt;\n",
            // Spelled through the trait method rather than the full path, so
            // each of these lands on exactly one needle: a line naming both the
            // module and the C name is reported twice, once per reason, and a
            // count of two stays two when one of the two stops firing.
            "fn stamp(m: &std::fs::Metadata) -> i64 { m.st_mtime() }\n",
            "fn stamp(m: &std::fs::Metadata) -> i64 { m.st_ctime() }\n",
            "fn stamp(m: &std::fs::Metadata) -> i64 { m.st_atime() }\n",
            // And the prelude, which brings the trait in without naming it.
            "use std::os::unix::prelude::*;\n",
            // And renamed, because a needle a rename walks past is the lesson
            // this list has learned four times.
            "use std::os::unix as posix;\n",
        ] {
            let found = judged(planted);
            assert_eq!(
                found.len(),
                1,
                "`{planted}` in the emitter was not a finding: {found:#?}"
            );
        }
        // And the control: the same file with nothing of the sort in it.
        assert_eq!(
            judged("fn f() -> usize { 1 }\n"),
            [] as [crate::gates::Finding; 0]
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A `cfg` that is not `test` is a finding, and the marker excuses it.
    ///
    /// **An entire rule of this module was decided by no test.**
    /// [`super::conditional_compilation`] runs on every line this gate judges,
    /// and every fixture beside it plants a *needle* — so misspelling the
    /// attribute it reads, `code.find("#[cfg(")` to `code.find("#[cfgZ(")`,
    /// left `9063 line(s) of the emitter read nothing that changes` unmoved,
    /// all 353 tests in this crate green and `check passed`. The rule's own
    /// paragraph names the incident it exists for: `Platform: linux, 64 bit` in
    /// all nine generated files, selected by a `#[cfg(target_os = …)]` constant.
    ///
    /// The predicate here is `unix`, and it is chosen because `CHANGES` is
    /// deliberately without it — the needle list's own comment says why. A
    /// predicate some needle also matches would be a fixture measuring its
    /// neighbour: `#[cfg(target_os = "linux")]` is two findings, one of them
    /// this rule's and one of them the `target_os` needle's, and a count of two
    /// stays two when this rule stops firing.
    #[test]
    fn a_cfg_that_is_not_test_is_a_finding() {
        let scratch = std::env::temp_dir().join(format!("nc-conditional-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        for directory in EMITS {
            std::fs::create_dir_all(scratch.join(directory)).expect("writable");
        }
        let source = scratch.join(EMITS[0]).join("emit.rs");
        let judged = |body: &str| {
            std::fs::write(&source, body).expect("writable");
            nothing_the_emitter_reads_changes(&scratch)
                .expect("readable")
                .0
        };

        let found = judged("#[cfg(unix)]\nfn f() {}\n");
        assert_eq!(
            found.len(),
            1,
            "`#[cfg(unix)]` in the emitter was not a finding: {found:#?}"
        );
        assert!(
            found[0].what.contains("compiled only when `unix`"),
            "the finding does not name the predicate: {}",
            found[0].what
        );

        // `cfg_attr` carries the same predicate, as its first argument — which
        // is why `conditional_on` reads both spellings and why both are here.
        let found = judged("#[cfg_attr(unix, derive(Debug))]\nstruct S;\n");
        assert_eq!(
            found.len(),
            1,
            "`#[cfg_attr(unix, …)]` was not a finding: {found:#?}"
        );
        assert!(
            found[0].what.contains("compiled only when `unix`"),
            "`cfg_attr`'s predicate is its first argument: {}",
            found[0].what
        );

        // `test` is the one predicate that cannot reach the output, and this is
        // the floor: a rule reporting every `cfg` would satisfy the lines above
        // and report `mod tests` in every file the walk reads.
        let found = judged("#[cfg(test)]\nmod tests {}\n");
        assert!(
            found.is_empty(),
            "`#[cfg(test)]` was reported, and it is the one `cfg` this rule allows: \
             {found:#?}"
        );

        // And the marker excuses it only when the reason names `cfg`, for the
        // reason the needle loop above states: `cargo fmt` moves things onto
        // lines, and a marker that names nothing excuses whatever lands beside it.
        let found = judged(&format!(
            "#[cfg(unix)] // {ALLOW} this `cfg` selects nothing that reaches a file\nfn f() {{}}\n"
        ));
        assert!(found.is_empty(), "a marked `cfg` was reported: {found:#?}");
        let found = judged(&format!(
            "#[cfg(unix)] // {ALLOW} about something else\nfn f() {{}}\n"
        ));
        assert_eq!(
            found.len(),
            1,
            "a marker naming nothing excused a `cfg`: {found:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A `cfg` the formatter wrapped is still a `cfg`, and so is an inner one.
    ///
    /// **The rule read a line and an attribute is not one.** `conditional_on`
    /// did `code.find("#[cfg(")` and then matched parentheses inside that same
    /// line, so an attribute broken across five lines returned no predicate at
    /// all — and `#![cfg(unix)]`, the inner spelling, was missed by the needle.
    /// Neither is exotic: **rustfmt rewrites** a long one-line `cfg` into the
    /// wrapped form, and two files in this tree already carry multi-line
    /// attributes, so the rule its own needle list calls "the shape rather than
    /// any spelling of it" could be switched off by running the formatter.
    ///
    /// Split out of [`a_cfg_that_is_not_test_is_a_finding`] because the two
    /// together cross a hundred lines: that one holds the predicate and the
    /// marker, this one holds the shapes the attribute can be written in.
    #[test]
    fn a_cfg_the_formatter_wrapped_is_still_a_finding() {
        let scratch = std::env::temp_dir().join(format!("nc-cfg-wrapped-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        for directory in EMITS {
            std::fs::create_dir_all(scratch.join(directory)).expect("writable");
        }
        let source = scratch.join(EMITS[0]).join("emit.rs");
        let judged = |body: &str| {
            std::fs::write(&source, body).expect("writable");
            nothing_the_emitter_reads_changes(&scratch)
                .expect("readable")
                .0
        };

        // **Wrapped, which is the form `cargo fmt` writes.** This read a line
        // and matched parentheses inside it, so an attribute broken across five
        // lines returned no predicate and the rule stepped over it — measured
        // on `crates/nc-canon/src/lib.rs`, where five such lines left `check
        // passed`, the output differing from a clean run only in the four line
        // counters. rustfmt *rewrites* a long one-line `cfg` into this.
        let found = judged("#[cfg(all(\n    unix,\n    not(windows)\n))]\nfn f() {}\n");
        assert_eq!(
            found.len(),
            1,
            "a `cfg` wrapped onto four lines was not a finding, and it is the form \
             `cargo fmt` writes: {found:#?}"
        );
        assert!(
            found[0]
                .what
                .contains("compiled only when `all(unix, not(windows))`")
                && found[0].line == 1,
            "the finding does not name the wrapped predicate, or is not reported where \
             the attribute opens: {:?}",
            found[0]
        );
        // The same shape under `cfg_attr`, whose predicate is its first argument
        // however many lines the attribute takes.
        let found = judged("#[cfg_attr(\n    unix,\n    derive(Debug)\n)]\nstruct S;\n");
        assert_eq!(
            found.len(),
            1,
            "a wrapped `cfg_attr` was not a finding: {found:#?}"
        );
        assert!(
            found[0].what.contains("compiled only when `unix`"),
            "a wrapped `cfg_attr`'s predicate is still its first argument: {}",
            found[0].what
        );

        // **The inner spelling**, missed by the needle rather than by the
        // wrapping: `#[cfg(` is not a substring of `#![cfg(`.
        let found = judged("#![cfg(unix)]\nfn f() {}\n");
        assert_eq!(
            found.len(),
            1,
            "`#![cfg(unix)]` — a whole module compiled only on one platform — was not \
             a finding: {found:#?}"
        );
        let found = judged("#![cfg_attr(unix, allow(dead_code))]\nfn f() {}\n");
        assert_eq!(
            found.len(),
            1,
            "`#![cfg_attr(unix, …)]` was not a finding: {found:#?}"
        );
        // And the one allowed predicate is still allowed in both shapes, or
        // this rule would report `mod tests`.
        for allowed in [
            "#![cfg(test)]\nfn f() {}\n",
            "#[cfg(\n    test\n)]\nfn f() {}\n",
        ] {
            let found = judged(allowed);
            assert!(
                found.is_empty(),
                "`test` is the one `cfg` this rule allows, and {allowed:?} was \
                 reported: {found:#?}"
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A `cfg` a comment or a string literal merely spells is not an attribute.
    ///
    /// **One walk, three readers, and this was the one still reading the raw
    /// line.** Its two neighbours are handed the whole-file stripped texts; this
    /// rule did `find("#[cfg(")` of the line as written, with one defence — a
    /// leading `//`. So it held "those six characters appear nowhere else on the
    /// line", and a trailing comment, a `/* … */` block and a string literal
    /// were all read as attributes. The carrier is live rather than theoretical:
    /// `emit.rs` pushes attribute text as string literals twenty-six times, and
    /// the day one of them is a `cfg` the emitter's own `push_str` is reported
    /// as conditionally compiled, with a fix line telling its author to write it
    /// unconditionally.
    ///
    /// The last case is the other half, and it is why the finding is decided on
    /// the spent text but **worded from the written one**: the blanked text
    /// spends a predicate's own literal, so reading the name off it would report
    /// `#[cfg(target_os = "linux")]` — the constant this rule was written for —
    /// as `target_os = " "`.
    #[test]
    fn a_cfg_in_a_comment_or_a_literal_is_not_an_attribute() {
        let scratch = std::env::temp_dir().join(format!("nc-cfg-spelled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        for directory in EMITS {
            std::fs::create_dir_all(scratch.join(directory)).expect("writable");
        }
        let source = scratch.join(EMITS[0]).join("emit.rs");
        let judged = |body: &str| {
            std::fs::write(&source, body).expect("writable");
            nothing_the_emitter_reads_changes(&scratch)
                .expect("readable")
                .0
        };

        for spelled in [
            "fn f() {} // never #[cfg(unix)]\n",
            "fn f() {} /* not #[cfg(unix)] either */\n",
            "const NOTE: &str = \"#[cfg(unix)]\";\n",
            "fn f() { out.push_str(\"#[cfg(unix)]\\n\"); }\n",
        ] {
            let found = judged(spelled);
            assert!(
                found.is_empty(),
                "{spelled:?} names a `cfg` without being one, and it was reported: \
                 {found:#?}"
            );
        }

        // **A `cfg` hidden behind one the rule allows.** `conditional_on` takes
        // the first opener on the line, so a comment or a literal spelling
        // `#[cfg(test)]` before a real `#[cfg(unix)]` answered `test` from the
        // written text — and the rule stood down for a line that is genuinely
        // conditional. The second pair is the mirror: a `test`-only line whose
        // comment or literal spells something else, refused for the same reason.
        for (behind, conditional) in [
            ("/* #[cfg(test)] */ #[cfg(unix)]\nfn f() {}\n", true),
            (
                "const NOTE: &str = \"#[cfg(test)]\"; #[cfg(unix)] fn f() {}\n",
                true,
            ),
            ("/* #[cfg(unix)] */ #[cfg(test)]\nfn f() {}\n", false),
            (
                "const NOTE: &str = \"#[cfg(unix)]\"; #[cfg(test)] fn f() {}\n",
                false,
            ),
        ] {
            let found = judged(behind);
            assert_eq!(
                found.len(),
                usize::from(conditional),
                "{behind:?} is {} conditional, and the gate said otherwise: {found:#?}",
                if conditional { "" } else { "not " }
            );
        }

        // **And the finding names the attribute it fired about.** The two walks
        // each take the first opener by offset, so a commented-out `cfg` in
        // front of a live one put them on different attributes: the gate fired
        // for `unix` and said `test`, over a fix line reading "`test` is the one
        // predicate that cannot reach the output". The sentence and its own
        // remedy contradicted each other and the reader was sent to the comment.
        let found = judged("/* #[cfg(test)] */ #[cfg(unix)]\nfn f() {}\n");
        assert!(
            found
                .iter()
                .any(|it| it.what == "this is compiled only when `unix`"),
            "the finding named an attribute other than the one it fired about: {found:#?}"
        );

        // The floor: the same six characters written as the attribute they look
        // like are still a finding, so the four above are green for their own
        // reason and not because the rule stopped reading.
        let found = judged("#[cfg(unix)]\nfn f() {} // and a comment beside it\n");
        assert_eq!(
            found.len(),
            1,
            "a real attribute stopped being a finding: {found:#?}"
        );

        // **The one line that reaches the fallback.** The parenthesis inside the
        // literal stops the written text's attribute ever closing, so only the
        // spent text can decide this is a `cfg` at all — and what the finding
        // can then say about the predicate is whatever survived blanking.
        let found = judged("#[cfg(feature = \"a(b\")]\nfn f() {}\n");
        assert!(
            found.iter().any(|it| it
                .what
                .starts_with("this is compiled only when `feature = ")),
            "an attribute the written text cannot close was not reported at all: {found:#?}"
        );

        // And the predicate is named from the line as written, literal and all.
        // The same line is two findings — the needle walk beside this rule
        // reports `target_os` on its own account — so the question asked here is
        // whether *one of them* is this rule's, worded from the written text.
        let found = judged("#[cfg(target_os = \"linux\")]\nfn f() {}\n");
        assert!(
            found
                .iter()
                .any(|it| it.what == "this is compiled only when `target_os = \"linux\"`"),
            "the predicate was named from the text its own literal had been spent \
             in: {found:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// What claims `rust-toolchain.toml` pins says what it actually pins.
    ///
    /// **A marker's reason is the whole of why a line is excused, and this one
    /// was not true.** `xtask/src/build.rs` excuses its `Command::new` on the
    /// ground that rustfmt is "pinned by rust-toolchain.toml"; that file says
    /// `channel = "stable"`, which is a name that moves under this repository
    /// every six weeks, and CI does not read the file at all — it installs the
    /// toolchain through an action pointed at the same moving channel by name.
    /// What is pinned is the *component list*:
    /// `rustfmt` and `clippy` are asked for, so they are installed. The version
    /// that formats the generated files is whatever `stable` is on the day.
    ///
    /// `README.md` describes this correctly. The marker's reason and
    /// `AGENTS.md`'s repository map did not, and a reason that overstates is
    /// worse than none: it is the sentence a reviewer checks the excuse by.
    ///
    /// This is a rule about the two sentences and not about the channel. If the
    /// channel is ever pinned to a version on purpose, the first assertion
    /// below fails and says so — which is the moment to write both sentences
    /// again.
    #[test]
    fn what_claims_the_toolchain_is_pinned_says_what_is_pinned() {
        let toolchain = std::fs::read_to_string(root().join("rust-toolchain.toml"))
            .expect("rust-toolchain.toml is readable");
        // **Parsed, because both keys were read by `lines().find_map`, which
        // knows nothing of the table they are in.** rustup reads them under
        // `[toolchain]` and nowhere else, and the two blindnesses are not the
        // same:
        //
        // ```text
        // components moved under an [extra] table   ->  386 passed
        //   rustup ignores it entirely — proved with a component that does not
        //   exist: under [toolchain] rustup warns, under [extra] it never looks
        //
        // a decoy `channel` at the root of the file   ->  found first by
        //   find_map and the assertion fails, and a [toolchain] with no
        //   `channel` makes rustup refuse the file outright
        // ```
        //
        // So the component list could be moved out of the table rustup reads
        // with every test in this repository green and a fresh clone left with
        // neither `rustfmt` nor `clippy` — the exact defect the paragraph below
        // was written for, one table along. The `toml` crate is already in this
        // workspace and answers the question that was being asked: what does
        // `[toolchain]` say.
        let table = toolchain
            .parse::<toml::Table>()
            .expect("rust-toolchain.toml is TOML rustup can read");
        let pinned = table
            .get("toolchain")
            .and_then(toml::Value::as_table)
            .expect(
                "`rust-toolchain.toml` has no `[toolchain]` table, and rustup refuses a \
                 file without one",
            );
        let channel = pinned
            .get("channel")
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
            .expect("`[toolchain]` declares a channel, and rustup refuses the file without one");
        assert!(
            ["stable", "beta", "nightly"].contains(&channel.as_str()),
            "`rust-toolchain.toml` names `{channel}`, which is a version rather than a \
             moving channel. That is a real pin, and the two sentences this test holds \
             were written for a channel that moves: say so in them and here"
        );

        // **And the component list, which is the thing all three sentences say
        // is pinned and which nothing read.** Deleting
        // `components = ["rustfmt", "clippy"]` left every test in this
        // repository green, and with `profile = "minimal"` beside it a fresh
        // clone then has neither binary: `cargo xtask build` fails at the spawn
        // in `build.rs`, whose marker excuses that spawn on precisely this
        // ground. CI stays green through all of it, because
        // the workflow's own action names both components itself — so the
        // one machine that would have reported it is the one machine that does
        // not read this file.
        //
        // The three sentences below are held to saying what is pinned; this is
        // what holds the file to still pinning it.
        let components: Vec<String> = pinned
            .get("components")
            .and_then(toml::Value::as_array)
            .expect(
                "`[toolchain]` declares no component list, and the component list is what \
                 the marker in `xtask/src/build.rs`, `AGENTS.md`'s repository map and \
                 `README.md` all say this file pins. A `components` key in any other \
                 table is a key rustup never looks at",
            )
            .iter()
            .filter_map(|name| name.as_str().map(str::to_owned))
            .collect();
        for needed in ["rustfmt", "clippy"] {
            assert!(
                components.iter().any(|name| name == needed),
                "`rust-toolchain.toml` pins {components:?} and not `{needed}`. It is what \
                 the gate spawns, and with `profile = \"minimal\"` a fresh clone that \
                 does not ask for it does not have it"
            );
        }

        // **The sentences themselves, by digest, and not a word test on them.**
        // The rule here was `said.contains("pin") && !(said.contains("component")
        // || said.contains("channel"))`, which asks whether three words are
        // present and nothing about what the sentence says with them: *"the
        // channel is pinned to a version and the component list is not"* — the
        // exact opposite of the truth — carries `channel` and passes. A prose
        // rule made of word tests moves the rot rather than stopping it, because
        // the words survive every rewrite that changes the meaning.
        //
        // So the three sentences are pinned the way `NOCTUA.md §3.6`'s alarms
        // are pinned: by digest, over the words with the wrapping normalised, so
        // that reflowing a paragraph is not a failure and changing what it says
        // is. Rewriting one is then a two-place edit whose second place is a
        // reviewer reading the new sentence against `rust-toolchain.toml`.
        //
        // **And three, not two.** `README.md` makes the same claim in the same
        // words — *"pins the channel and the components"* — and nothing read it;
        // it is the first account of this a newcomer meets.
        let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");

        let source = std::fs::read_to_string(root().join("xtask/src/build.rs")).expect("readable");
        let marker = source
            .lines()
            .find(|line| line.contains(ALLOW) && line.contains(r#"Command::new("rustfmt")"#))
            .and_then(|line| {
                crate::references::reason_after(line, ALLOW, crate::references::Comments::Slashes)
            })
            .map(words)
            .expect("the formatter's call site carries a marker");

        let agents = std::fs::read_to_string(root().join("AGENTS.md")).expect("readable");
        let mapped = agents
            .lines()
            .find(|line| line.contains("rust-toolchain.toml") && line.starts_with('\u{251c}'))
            .map(words)
            .expect("the repository map names rust-toolchain.toml");

        // The whole bullet, because the claim wraps over four lines and half of
        // it says nothing on its own.
        let readme = std::fs::read_to_string(root().join("README.md")).expect("readable");
        let opens = readme
            .find("- **Rust**, via rustup.")
            .expect("README.md's requirements open with the toolchain");
        let closes = readme[opens + 1..]
            .find("\n- **")
            .map_or(readme.len(), |at| opens + 1 + at);
        let bullet = words(&readme[opens..closes]);

        for (named, sentence, pinned) in [
            (
                "xtask/src/build.rs",
                &marker,
                "dad58095d0c1d74ab22b70fe921af3ad",
            ),
            ("AGENTS.md", &mapped, "281a4d27ce27d73c403d726137dfa78b"),
            ("README.md", &bullet, "ad1b185d1d9c5d30f273f948170a331b"),
        ] {
            // The floor under the digest: a locator that matched an empty line
            // would pin the digest of nothing, and every rewrite would pass.
            assert!(
                sentence.contains("rust-toolchain.toml") && sentence.len() > 40,
                "the sentence found in {named} is not the claim this pins: {sentence}"
            );
            let measured = blake3::hash(sentence.as_bytes()).to_hex();
            assert_eq!(
                &measured.as_str()[..32],
                pinned,
                "{named} no longer says what it said about `rust-toolchain.toml`. What \
                 that file pins is the *component list*; the channel it names is `stable`, \
                 which moves under this repository every six weeks, and CI installs the \
                 toolchain through an action pointed at the same moving name. Read the new \
                 sentence against the file, then put its digest here in the same edit — \
                 it is `{measured}`. The sentence read was: {sentence}"
            );
        }
    }

    /// A needle renamed at the `use` is the same needle.
    ///
    /// **Four of the list's entries could be walked past by writing `as`.**
    /// `SystemTime` and `HashMap` survive being aliased because the needle is
    /// the bare type name; `Instant::now`, `Utc::now` and `Local::now` were the
    /// *call*, so `use std::time::Instant as Tick;` and then `Tick::now()` was
    /// **0 findings**, measured. `process::id` was the same hole through the
    /// other spelling of an import: `use std::process::{id as pid};` was 0
    /// findings too, because `process::id` is not a substring of
    /// `process::{id`.
    ///
    /// `Utc` is the severe one. A date at day granularity is identical in two
    /// generations a millisecond apart and identical again in CI's same-day
    /// cross-checkout diff, so neither of the two determinism checks would ever
    /// have said a word — this list is the whole of what stands there.
    ///
    /// **`process::Command` is not in here, and that is a correction to an
    /// earlier record**: it is backstopped. clippy resolves the alias by
    /// def-id, so `disallowed_methods` fires on a renamed `Command::new` and
    /// `check` fails at lints. The four above have no backstop at all.
    #[test]
    fn a_needle_renamed_at_the_use_is_the_same_needle() {
        let scratch = std::env::temp_dir().join(format!("nc-aliased-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        for directory in EMITS {
            std::fs::create_dir_all(scratch.join(directory)).expect("writable");
        }
        let source = scratch.join(EMITS[0]).join("emit.rs");
        let judged = |body: &str| {
            std::fs::write(&source, body).expect("writable");
            nothing_the_emitter_reads_changes(&scratch)
                .expect("readable")
                .0
        };

        for (aliased, needle) in [
            ("use std::time::Instant as Tick;", "Instant"),
            ("use chrono::Utc as Clock;", "Utc"),
            ("use chrono::Local as Here;", "Local"),
            ("use std::process::{id as pid};", "process"),
            ("use std::process::{self, id};", "process"),
            // **The alias, and the wrapped braced import.** Neither could be
            // seen by a needle carrying `::`: `machine::id()` names no
            // `process::id`, and `mentions` reads one line, so a group rustfmt
            // has wrapped leaves `process::{` with nothing after it on the line.
            ("use std::process as machine;", "process"),
            ("use std::process::{", "process"),
        ] {
            let found = judged(&format!("{aliased}\nfn f() {{}}\n"));
            assert!(
                found.iter().any(|one| one.what.contains(needle)),
                "`{aliased}` was not reported as `{needle}`, so the needle is a spelling \\
                 rather than the thing: {found:#?}"
            );
        }

        // And a longer name that merely begins the same way is still not a use
        // of it, or the bare names above would report half the tree.
        for innocent in [
            "struct Instantiation;",
            "struct Locale;",
            "fn utc_ish() {}",
            "fn process_ids() {}",
        ] {
            let found = judged(&format!("{innocent}\n"));
            assert!(
                found.is_empty(),
                "`{innocent}` names none of these and was reported: {found:#?}"
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A braced import is the same path, and a needle carrying `::` reads it.
    ///
    /// **The branch that does that was decided by nothing.** Both fixtures for
    /// the braced form pass the needle `process`, which is a bare segment the
    /// plain scan matches on its own — so the branch's own reason for existing,
    /// `process::id`, stopped being a needle and nothing noticed. Measured, the
    /// `rsplit_once` filtered to `None`: the whole gate byte-identical to a
    /// clean run, and `use std::io::{stdin as read_input};` planted into a
    /// scanned crate reported nothing where the unplanted tree reports
    /// `` `io::stdin` reaches generated output ``.
    ///
    /// Ten needles carry `::` today, `Command::new` among them.
    #[test]
    fn a_needle_that_carries_a_path_reads_a_braced_import() {
        assert!(
            mentions("use std::io::{stdin as read_input};", "io::stdin"),
            "a braced import is the same import written another way"
        );
        assert!(
            mentions("use std::io::{self, stdin};", "io::stdin"),
            "and so is one that brings the module in beside the name"
        );
        // What it must not do: report the group that does not hold the name,
        // or a name that merely begins the same way.
        assert!(
            !mentions("use std::io::{stdout, Write};", "io::stdin"),
            "a group without the name is not a use of it"
        );
        assert!(
            !mentions("use std::io::{stdin_locked};", "io::stdin"),
            "a longer name is not a use of the shorter one, inside a group either"
        );
        assert!(
            !mentions("use std::fs::{stdin};", "io::stdin"),
            "the path in front of the group is part of the needle"
        );
        // And the group ends at its own brace: a name written after it is not
        // in it. Measured, the closing brace replaced by the end of the text —
        // a parameter called `stdin` beside an unrelated import was reported as
        // a use of `io::stdin`, and every other assertion here stayed green.
        assert!(
            !mentions("use std::io::{stdout}; fn f(stdin: u8) {}", "io::stdin"),
            "a name after the group is not inside it"
        );
    }

    #[test]
    fn a_longer_name_is_not_a_use_of_the_shorter_one() {
        assert!(mentions("let x = env!(\"A\");", "env!"));
        assert!(!mentions("let x = option_env!(\"A\");", "env!"));
        assert!(mentions("let x = option_env!(\"A\");", "option_env!"));
        // And the other end of the name, which `env::vars` needed: a longer name
        // that *begins* with the needle is not a use of it either.
        // The `env::` module rule reaches every one of these, which is why they
        // are no longer four needles that each had to be remembered.
        for spelling in [
            "let a = env::var(\"A\");",
            "for (k, v) in env::vars() {}",
            "let n = env::var_os(\"A\");",
            "let n = env::args().count();",
            "let n = env::vars_os().count();",
            "let p = env::consts::OS;",
            "let e = env::current_exe();",
            // **And the two spellings that carry no `::` at all**, which is why
            // the needle is the segment. `use std::env as machine;` renames the
            // module at the import and every call through the alias then names
            // nothing on this list — measured past the whole gate.
            "use std::env as machine;",
            "use std::{env, fs};",
            // The build-time macro is this needle too, which is why it is no
            // longer an entry of its own.
            "let x = env!(\"A\");",
        ] {
            assert!(mentions(spelling, "env"), "{spelling}");
        }
        // And a longer name that merely contains it is not it, at either end.
        for spelling in [
            "let x = option_env!(\"A\");",
            "#[cfg(target_env = \"gnu\")]",
            "let e = environment();",
            "let e = my_env;",
        ] {
            assert!(!mentions(spelling, "env"), "{spelling}");
        }
        assert!(!mentions("for name in created_tables(&s) {}", "created"));
        // A needle that ends in punctuation matches what follows it: the
        // module, the directory, the path.
        assert!(mentions("let p = env::consts::OS;", "env::"));
        assert!(mentions("read_to_string(\"/etc/hostname\")", "/etc/"));
        assert!(mentions("let at = meta.created();", "created"));
        // And a name that merely ends the same way.
        assert!(!mentions("let m = MyHashMap::new();", "HashMap"));
        assert!(mentions(
            "let m: std::collections::HashMap<u8, u8>;",
            "HashMap"
        ));
    }

    #[test]
    fn every_crate_the_emitter_reaches_is_read() {
        // The entries listed by hand, because no dependency edge reaches them.
        const BY_HAND: [&str; 1] = ["xtask/src/build.rs"];
        // **`EMITS_UNDER` is the floor's scope, and nothing tied it to `EMITS`.**
        // Dropping `crates/nc-canon/src` from it left `ok 9015 line(s) of the
        // emitter read nothing that changes` and `check passed` — 1,864 lines of
        // silent slack. Spending it: one `return` in the walk's visitor skipping
        // `docs.rs`, the file that emits every `docs/*.md` page, gave
        // `ok 8216 line(s)`; then `let _ = std::time::SystemTime::now();` at the
        // top of `docs::all` — a live clock in the emitter — also `check passed`,
        // 968 tests green. `EMITS` is pinned by this test; its copy was not.
        for named in super::EMITS {
            assert!(
                super::EMITS_UNDER.contains(&named),
                "`EMITS` reads `{named}` and `EMITS_UNDER` — the floor's scope — does \
                 not, so the floor is short by whatever that path holds and the slack \
                 is spendable on a filter inside the walk"
            );
        }

        // **`EMITS` was two hard-coded directories with nothing tying them to
        // what emits**, and the first fix did not tie them either: it read the
        // emitter's `[dependencies]` looking for `nc-` crates, and the emitter
        // has none — so `reached` was `[EMITTER, ALSO]`, the same two strings
        // `EMITS` already held, and replacing the whole manifest walk with an
        // empty string left the test green. It was also blind to the table form
        // `[dependencies.nc-ids]`, which `gates.rs` already documents for
        // `[lints]`: a real dependency written that way put a clock in a crate
        // the emitter calls, and the gate never opened the directory.
        //
        // So this walks the *closure*: every workspace crate reachable from the
        // emitter, by either TOML spelling, transitively.
        let mut reached: Vec<String> = vec![EMITTER.to_owned(), ALSO.to_owned()];
        let mut at = 0_usize;
        while at < reached.len() {
            let crate_ = reached[at].clone();
            at += 1;
            let Ok(manifest) =
                std::fs::read_to_string(root().join(format!("crates/{crate_}/Cargo.toml")))
            else {
                continue;
            };
            for name in workspace_dependencies(&manifest) {
                // **The closure stops at generated crates.** `nc-canon` depends
                // on `nc-catalog`, whose `src/` *is* the output — already
                // compared byte for byte by the drift check, and already
                // regenerated from the emitter this gate reads. Scanning it
                // would report a finding whose fix is in a different file, which
                // is the wrong place to send a reader.
                let generated = nc_catalog_compiler::GENERATED_DIRECTORIES
                    .iter()
                    .any(|directory| *directory == format!("crates/{name}/src"));
                if !generated && !reached.contains(&name) {
                    reached.push(name);
                }
            }
        }
        reached.sort();
        reached.dedup();
        // The floor: a walk that read no manifest would agree with `EMITS` by
        // finding nothing, which is how the first version passed.
        assert!(
            reached.len() >= 2,
            "the dependency walk found {reached:?}, so it proves nothing"
        );

        for crate_ in &reached {
            assert!(
                EMITS
                    .iter()
                    .any(|directory| *directory == format!("crates/{crate_}/src")),
                "`{crate_}` reaches generated output and EMITS does not read it. \
                 Reached: {reached:?}"
            );
        }
        // **And nothing else, except what no dependency edge can reach.** The
        // closure walks from the emitter *outwards*, so it can never find the
        // crate that calls it — and `xtask/src/build.rs`, which runs rustfmt and
        // writes the bytes, was outside the scan for exactly that reason. A
        // `std::env::var("USER")` there reached every generated file with all
        // three determinism gates green.
        //
        // So the equality is against the closure plus the entries listed by
        // hand, and each of those is named here: adding a fourth without saying
        // why fails, and a crate the closure finds is still required to be in
        // `EMITS` by the loop above.
        for named in BY_HAND {
            assert!(
                EMITS.contains(&named),
                "`{named}` is listed here as read by hand and EMITS does not read it"
            );
        }
        assert_eq!(
            EMITS.len(),
            reached.len() + BY_HAND.len(),
            "EMITS holds {} entries, the emitter reaches {} crate(s) ({reached:?}) and \
             {} is listed by hand",
            EMITS.len(),
            reached.len(),
            BY_HAND.len()
        );
    }

    #[test]
    fn a_dependency_is_found_in_either_toml_spelling() {
        // The table form is what `gates.rs` already documents for `[lints]`, and
        // the walk read only the inline one — so a real dependency written
        // `[dependencies.nc-ids]` was invisible.
        let inline = "[dependencies]\nnc-ids = { path = \"../nc-ids\" }\nserde.workspace = true\n";
        assert_eq!(workspace_dependencies(inline), vec!["nc-ids".to_owned()]);

        let table = "[package]\nname = \"x\"\n\n[dependencies.nc-ids]\npath = \"../nc-ids\"\n";
        assert_eq!(workspace_dependencies(table), vec!["nc-ids".to_owned()]);

        let both = format!("{inline}\n[dependencies.nc-canon]\npath = \"../nc-canon\"\n");
        assert_eq!(
            workspace_dependencies(&both),
            vec!["nc-ids".to_owned(), "nc-canon".to_owned()]
        );

        // And a `[dev-dependencies]` is not one: a test's clock reaches no
        // generated file.
        let dev = "[dev-dependencies]\nnc-ids = { path = \"../nc-ids\" }\n";
        assert_eq!(workspace_dependencies(dev), [] as [String; 0]);
    }

    /// Needles whose deletion must turn this suite red.
    ///
    /// **`CHANGES` had no membership assertion**, unlike `SUITES`, `SHAPES` and
    /// `CORE_POLICIES` — `every_shape_is_a_finding…` iterates the list, so
    /// deleting an entry only shortens the loop and every test stays green.
    /// Measured: all nine needles added in one round removed together, suite
    /// green; and with `target_os` alone gone, a `#[cfg(target_os = "linux")]`
    /// constant in `nc-canon` put the build machine's platform into nine
    /// generated files with `check passed` — verbatim the failure that needle's
    /// own comment says it closes.
    ///
    /// **The whole list**, and it was fourteen of about forty. The criterion
    /// was "a needle a measurement was written against", which is a fact about
    /// this module's history rather than about what the needle is worth:
    /// measured, `HashSet`, `getrandom`, `Utc::now` and `is_terminal` deleted
    /// **together** left the whole suite green, and `HashSet` sits directly
    /// beside `HashMap` — which was protected — and is exactly as dangerous.
    ///
    /// A needle nobody has been burned by yet is still a needle somebody wrote
    /// for a reason. Removing one is now a two-place edit whichever it is, and
    /// the list below is checked against `CHANGES` in **both** directions, like
    /// `SUITES` and `SHAPES`: a needle added and not named here fails too, so
    /// the pair cannot drift.
    const MUST_STAY: [&str; 68] = [
        // **The nine added when a reviewer showed the four were four of twelve.**
        // `CHANGES`'s own comment records the incident that made it — a
        // `#[cfg(target_os = …)]` constant putting `Platform: linux, 64 bit` into
        // all nine generated files with `check passed` — and the fix named the
        // key that had done it and its three nearest relatives. Each of these
        // selects code by the machine or by the build profile just as well, and
        // each was measured writing a line into all nine generated files with
        // the whole gate green.
        "target_family",
        "target_env",
        "target_vendor",
        "target_feature",
        "target_abi",
        "debug_assertions",
        "overflow_checks",
        "panic = ",
        "SystemTime",
        "UNIX_EPOCH",
        "Instant",
        "chrono",
        "OffsetDateTime",
        "HashMap",
        "HashSet",
        "current_exe",
        "available_parallelism",
        "accessed",
        "std::os",
        "st_mtime",
        "st_ctime",
        "st_atime",
        "mtime",
        "ctime",
        "atime",
        "MAIN_SEPARATOR_STR",
        "path::absolute",
        "path::is_separator",
        "size_of::<",
        "isize::BITS",
        "io::stdin",
        "thread::current",
        "\"/dev/",
        "size_of_val",
        "align_of_val",
        "usize::MAX",
        "isize::MAX",
        "current_dir",
        "env",
        "option_env!",
        "process",
        "modified",
        "created",
        "elapsed",
        "Utc",
        "Local",
        "rand",
        "getrandom",
        "RandomState",
        "cfg!",
        "MAIN_SEPARATOR",
        "/etc/",
        "\"/proc/",
        "\"/sys/",
        "Command::new",
        "TcpStream",
        "UdpSocket",
        "read_dir",
        "target_os",
        "target_arch",
        "target_pointer_width",
        "target_endian",
        "usize::BITS",
        "align_of",
        "{:p}",
        "canonicalize",
        "IsTerminal",
        "is_terminal",
    ];

    #[test]
    fn the_needle_list_and_the_written_list_are_the_same_list() {
        // **Both directions.** One direction says every named needle is still
        // in `CHANGES`; the other says `CHANGES` holds nothing this list does
        // not name — without which a needle added today is unprotected
        // tomorrow, which is how the list came to cover fourteen of forty.
        let mut declared: Vec<&str> = MUST_STAY.to_vec();
        declared.sort_unstable();
        let mut listed: Vec<&str> = CHANGES.iter().map(|(needle, _)| *needle).collect();
        listed.sort_unstable();
        assert_eq!(
            declared, listed,
            "MUST_STAY and CHANGES disagree about which needles exist. Removing a needle \
             is a two-place edit, and adding one is too — otherwise the new needle is the \
             one nothing protects"
        );
    }

    #[test]
    fn no_needle_is_listed_twice() {
        // Two entries for one needle report the same line twice under two
        // reasons, which is how a reader learns to skim a gate — and it happened
        // to `env::vars` the moment the list grew in two places at once.
        let mut seen = std::collections::BTreeSet::new();
        for &(needle, _) in CHANGES {
            assert!(seen.insert(needle), "`{needle}` is in CHANGES twice");
        }
        assert!(seen.len() > 1, "nothing was compared");
    }

    /// A machine path is seen however its root is written.
    ///
    /// **The needle was anchored to the literal's opening quote**, so it spoke
    /// only for a path written out whole. `format!("{}/etc/machine-id", dir)` —
    /// the ordinary way a root is joined to a path — was invisible to it, and to
    /// the absolute-path half too, which needs a `"/` to start at. `/etc/`
    /// cannot be a relative segment of anything here, so the quote was carrying
    /// no weight.
    #[test]
    fn a_machine_path_is_seen_however_its_root_is_written() {
        // **Each row says what it expects, because a disjunction can be paid by
        // the wrong half.** This was `spoke || !reported.contains("/etc/")`, and
        // the fourth row holds no `/etc/` at all — so its right-hand half was
        // true whatever the gate did, and the row decided nothing while the
        // paragraph above claimed it did. Measured in wave 28: with the
        // disjunct removed the row fails, and the failure is real.
        //
        // It is real and it is **not a defect in the gate**. `root.join("etc")`
        // is a path into the machine only if `root` is `/`, and `root` is a
        // value this reader cannot resolve — in this repository it is the
        // repository. A rule that flagged every `.join("etc")` would refuse a
        // scratch directory. So the limit is written down here rather than
        // hidden behind a half that cannot fail: a join-built path is seen only
        // when its own text names an absolute root.
        for (reported, seen) in [
            (r#"read_to_string("/etc/machine-id")"#, true),
            (r#"read_to_string(format!("{}/etc/machine-id", dir))"#, true),
            (r#"read_to_string(format!("{dir}/etc/machine-id"))"#, true),
            (
                r#"read_to_string(Path::new("/etc").join("machine-id"))"#,
                true,
            ),
            (
                r#"read_to_string(root.join("etc").join("machine-id"))"#,
                false,
            ),
        ] {
            let spoke = super::names_an_absolute_path(reported)
                || super::CHANGES
                    .iter()
                    .any(|(needle, _)| super::mentions(reported, needle));
            assert_eq!(
                spoke, seen,
                "a path into the machine is a finding however its root is written, and a \
                 path whose root is a value this reader cannot resolve is written down as \
                 unseen rather than carried by a disjunct that cannot fail: {reported}"
            );
        }
        // And a relative path built the same way is not one — this repository
        // writes `format!("{DIRECTORY}/{name}.md")` and means a file in the tree.
        let relative = r#"format!("{DIRECTORY}/{name}.md")"#;
        assert!(
            !super::names_an_absolute_path(relative),
            "a placeholder followed by a slash is not an absolute path: {relative}"
        );
    }

    /// A `..` is a segment wherever it stands, not only at the opening quote.
    ///
    /// `/etc/machine-id` is the file this whole rule names as its reason, and
    /// three spellings reach it. Written as "the literal begins `..`", the rule
    /// saw one of them.
    #[test]
    fn a_path_that_climbs_out_is_seen_wherever_the_climb_is() {
        for reported in [
            // The spelling the rule was shown.
            r#"read_to_string("../../../etc/machine-id")"#,
            // One character in front of it, opening the same file.
            r#"read_to_string("./../../../etc/machine-id")"#,
            // One segment in front of it, opening the same file.
            r#"read_to_string("src/../../../etc/machine-id")"#,
            // A bare climb, which is what `Path::join` is handed.
            r#"root.join("..")"#,
            // And the absolute form the rule has always caught.
            r#"read_to_string("/etc/machine-id")"#,
            // One slash too many reaches the same file: POSIX resolves `//x`
            // and `///x` to `/x`, and this is the file the rule exists for.
            r#"read_to_string("//var/lib/dbus/machine-id")"#,
            r#"read_to_string("///etc/machine-id")"#,
            // Interpolated, which is how a path is written when part of it is
            // computed — and still the same file.
            r#"read_to_string(format!("/{}/machine-id", dir))"#,
            r#"read_to_string(format!("/{dir}/machine-id"))"#,
        ] {
            assert!(
                super::names_an_absolute_path(reported),
                "a path out of the repository is a finding: {reported}"
            );
        }

        for allowed in [
            // Relative paths inside the tree, which are the ordinary case.
            r#"read_to_string("catalog/errors.toml")"#,
            r#"root.join("crates").join("nc-ids")"#,
            // A version range is not a path, and neither is prose that opens
            // with a full stop — the old rule reported the latter.
            r#"let range = "1.0..2.0";"#,
            r#"let note = "..and then the digest changes";"#,
            // A doubled slash opens a URL, which the absolute half already
            // excludes for its own reason.
            r#"let home = "https://example.invalid/x";"#,
            // And the emitter's own generated doc comments, which are what the
            // carve-out was written for. A comment has a space after its
            // slashes; a path has a name.
            r#"out.push_str("/// The identifier this row names.");"#,
            r#"out.push_str("// generated, do not edit");"#,
            // `//!` is the inner doc comment the emitter writes at the head of
            // every generated module: neither a space nor a name.
            r#"out.push_str("//! One struct per event.");"#,
            // And the two the emitter writes most: a bare `///` line, whose
            // next character is the backslash of an escape, and a `//!` head.
            r#"out.push_str("///\n");"#,
            r#"out.push_str("//!\n");"#,
        ] {
            assert!(
                !super::names_an_absolute_path(allowed),
                "this names nothing outside the repository: {allowed}"
            );
        }
    }

    #[test]
    fn every_finding_says_what_to_do_instead() {
        // The rule `gates.rs` holds every finding in this repository to: a fix
        // that restates the defect sends a reader back where they started.
        for &(needle, instead) in CHANGES {
            // `mentions` rather than `contains`, for the same reason the gate
            // itself uses it: "a **rand**om number differs on every run" does
            // not restate the needle `rand`, and a substring test called it one.
            assert!(
                !instead.is_empty() && !mentions(instead, needle),
                "`{needle}`'s fix restates it: {instead}"
            );
        }
    }
}
