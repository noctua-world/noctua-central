# Canaries

Files here are **not compiled**. They are outside every crate's `src/`, so cargo
never sees them — and the gate each one is for does, because that gate walks the
tree by file extension. *Not* every gate: see below.

**Which gate, and therefore which directory.** A canary has to live inside the
scope of the gate it proves. The blanket-allow walk and the two citation gates
read the whole tree, so their canaries are here. The §15.1 walk reads the
crates, the plugins and the migrations and never opens `xtask/`, so its canary is
`crates/nc-canon/canaries/a_code_written_by_hand.rs` — put here, it would be a
file no gate ever reads and a green run that proves nothing.

Each one carries a real instance of what a gate refuses. `cargo xtask check`
asserts the gate **found it**, and then drops that finding from what it reports.

## The review loop, deliverable by deliverable

`NOCTUA-PLAN.md` Stage 10 is ten deliverables. Each one ends the same way: its
own tests, then `cargo xtask check` green, then an audit by someone who did not
write the code, with every finding fixed and proved by planting the defect. The
loop ends for a deliverable when two independent full passes over it return
nothing, for the reason given below.

| | Deliverable | Scope items | Acceptance criteria it carries |
|---|---|---|---|
| D1 | Repository skeleton, toolchain, xtask spine, documents | 1, 2, 3 | 1 |
| D2 | The catalog compiler and its registries | 4 | 4, 6, 7 |
| D3 | `nc-ids` — Snowflake, machine flake, Denote naming | 6 | 8, 9 |
| D4 | `nc-journal` — the hash-chained command journal | 5 | 3 |
| D5 | The log-event type wall and the central log service | 7 | 2 |
| D6 | The row-limit pruner and the unbounded-table gate | 8 | 5 |
| D7 | The `policy/` skeleton and its negative tests | 11 | 11 |
| D8 | `cargo xtask` complete, the gate suite, CI, determinism | 9 | 1, 4 |
| D9 | The local development stack | 10 | 10 |
| D10 | Documents across the workspace, and the stage closed | `NOCTUA-PLAN.md` §P0.2.5, §P0.5 | — |

**Each criterion is a command, and the command is what is written down.** A
result pasted here is a measurement of the day it was pasted, and a mutation
tally pasted here goes wrong that way. Run these; do not trust a transcript of
them.

| | What it proves | Command |
|---|---|---|
| 1 | the gate is green, no failures and no warnings | `cargo xtask check` |
| 1 | …and on a clone with no workspace above it, which is what CI has | the same, in a copy outside `repos/` — every warn line an announced `not run:`, and a pass |
| 2 | a compile-fail fixture refuses **for the stated reason** | `cargo xtask check`, the `fixture(s) refuse to compile` line |
| 3 | the chain breaks at **every** position of a synthetic history | `cargo test -p nc-journal --test tampering` |
| 4 | two generations are byte-identical | `cargo xtask check`, the `two generations` line; and the CI job that builds twice and diffs |
| 5 | a table with no bound fails, and the error names it | `cargo test -p xtask the_unbounded_table_predicate_decides_every_clause` (both halves: the driver requires the refusal to quote a table the migration wrote) |
| 6 | a key missing from one locale fails, and the error names it | `cargo test --workspace a_key_missing_from_one_locale_fails_the_build_and_the_error_names_it` |
| 7 | a renumbered, repointed or removed code is refused | `cargo test --workspace ledger::tests` |
| 8 | `parse(build(x)) == x` under property test | `cargo test -p nc-ids --test properties` |
| 9 | monotonic, unique across flakes, refuses a backwards clock | `cargo test -p nc-ids --lib snowflake` |
| 10 | an emptied stack comes back up and the gate passes against it | `cargo xtask dev --down && cargo xtask dev && cargo xtask check` |
| 11 | every core policy has a denial test, counted | `cargo xtask check`, the `each have a denial test` line |

Criteria 5, 6 and 7 are proven by **making the failure happen** inside a test that
restores the tree afterwards. A gate nobody has watched fire is a gate nobody
knows is wired up.

Three things hold about how such an audit behaves, and each is a rule for
running one.

**Independent auditors converge on the same defect.** Two of them, from
different subsystems and neither looking for it, report the same first finding
— because both ran the baseline before planting anything, which is the first
thing an audit does.

**A repair is where the next defect hides.** A floor calibrated on the wrong
tree, a shared reader that makes two counters agree about nothing, a doc comment
that changes owner in the edit that adds the ratchet against doc comments
changing owner, a gate narrower than it claims, a scan reading the wrong text
for most of the bodies it inspects — each is a defect a repair introduces, and
an audit of the original code alone finds none of them. That is the argument
for a *new* auditor rather than a second pass by the same one.

**And one clean pass is evidence, not proof.** A deliverable that one full pass
returned nothing on can still carry a defect a different auditor finds — a
`critical` error code raised for a fault that is not that fault, and a one-line
repair, closing a measured exploit, that can be reverted with the whole gate
green. A deliverable is settled by two independent clean passes, not one.

## Why a count could not do this

Every floor in this repository was a number: how many files a walk read, how many
lines it judged, compared against a second count. Five of them were defeated the
same way — the counter runs before the judgement, so a filter placed between them
leaves the number at or above its baseline while the gate judges nothing.
Measured, with a blanket `#![allow(clippy::all)]` over acceptance criterion 3's
own suite and a hand-written error code in the tree:

```text
ok    63561 source line(s) leave the workspace lints on     (baseline 63544)
ok    24609 source line(s) name no error code as text       (unchanged)
check passed
```

No position for a counter fixes that: a filter can always be written above it.
And three of the second counts turned out to share the walk or the skip-list with
the gate they audited, so a filter expressed through either moved both numbers
together.

A canary is not a number, and it answers a different question: a count says how
much a walk *believes* it read, and a canary says whether the walk actually
reached one known file and the rule actually fired on it.

**It is not a replacement for a floor.** A filter
expressed over *all* lines hides the canary too. A filter expressed over a
*subset* — `if relative.starts_with("crates/nc-journal/")`, `if index > 10`,
`if !path.contains("/canaries/")` — leaves the canary reported and the rest of
the tree unread, each measured passing. The canary sits at a fixed, documented
path, which is exactly what makes it easy to write a filter around.

So both are kept, and neither is load-bearing alone: the line floors catch a walk
that stops early, and the canaries catch a walk that reads nothing it is supposed
to judge.

**And a filter shaped to slip between them takes one line, four times.** That
such a filter "has to be written on purpose and read like one in a diff" is no
defence, and it is not even hard: both counters sit above the judgement, so

```rust
read += 1;
if index > 40 { continue; }
```

leaves every number where it was, and the canary's fixed documented path is what
makes `if !relative.contains("/canaries/") { continue; }` writable. Measured, all
four canaried gates filtered at once with four real defects planted in the tree —
a blanket allow over the tamper suite, a hand-written `NC-KERN-4242`, a dangling
path and a dangling section:

```text
ok    64653 source line(s) leave the workspace lints on      (baseline 64631)
ok    25329 source line(s) name no error code as text        (baseline 25319)
ok    1453 section citation(s) name a real section           (baseline 1452)
ok    786 citation(s) resolve                                (baseline 785)
check passed
```

Every number **up**, every canary still reported.

## The third mechanism: a fixture whose address is not knowable

No count can close this, because the counter runs before the judgement — a filter
after the increment is invisible to any floor derived from it. What closes it is
a defect planted where a filter cannot name it.

`gates.rs`'s `somewhere_unpredictable` builds a scratch tree in which the
directory names, the nesting depth, the file name and the line the defect sits on
are all derived from the process id. `contains("/canaries/")` misses it because
the path is not `canaries`; `index > 40` misses it because the defect is on line
200-something and moves every run. **Six** walks are driven over such a tree —
the blanket-allow walk, the §15.1 walk, the `.env` walk, the reference gate, the
section gate, and the emitter's determinism walk.

**And it proves the walk arrives somewhere, not that it arrives everywhere.**
The fixture is unpredictable to a filter keyed on *its* location and invisible
to one keyed on a *real* location, because it is never planted at one: three
one-line filters naming `crates/nc-journal/`, `/nc-journal/tests/` and
`/policy/` left a hand-written code, a blanket allow over the tamper suite and a
committed `.env.local` in the tree, with every number up, every canary reported
and all six of these tests green.

What closes that is a third counter, not a third fixture. The walks in
`corpus.rs`'s `ASKED` keep two, and that constant is the list — 
one where a line or a citation becomes
the gate's to judge, and one on every path out of the judgement. A filter
between them makes them disagree by construction, which is what no position for
a single counter can do. Measured against the three filters above: sixteen
findings, naming the files.

**And the second counter does not close it either, because a filter can pay
it.** The paragraph above claims a filter between the two counters makes them
disagree; it does not, when the filter increments both on its way past. Both
counters are inside the loop the filter is in, so five one-line filters — each
`{ judged += 1; reached += 1; continue; }` keyed on a real subtree — left five
real defects in the tree with every printed number *up* and `check passed`:

```text
ok    76133 source line(s) leave the workspace lints on     (baseline 76096)
ok    29929 source line(s) name no error code as text       (baseline 29911)
ok    965 citation(s) resolve                               (baseline 964)
ok    166 path(s) — with a committed .env.local in one of them
```

The path was the one thing a fixture never varied. `somewhere_unpredictable`
randomises depth, name and line and takes the part *inside* the repository as a
literal from its caller — so the scratch tree has no `crates/nc-journal/` in it
to be skipped, and a filter naming one is invisible. A pid-derived path does not
help: `crates/nc-{pid}/` is still not `crates/nc-journal/`.

**The fixture's directories have to be the repository's own.**
`in_every_real_directory` reads `crates/` from the real tree, plants the same
defect in every crate's `src/` and `tests/` and in `xtask/`'s, and every caller
asserts on **all** of them — so a filter naming any one subtree loses exactly
that finding. Six walks are driven over it. The §15.1 one asks the gate's own
`in_the_error_code_scope` which directories it is allowed to skip, and asserts
both directions: every directory inside the scope is reached, and every
directory the exclusion list names is not judged. That is also what keeps the
exclusion list honest, and why `check.rs`'s `scoped_out()` is derived from
`OUTSIDE_THE_ERROR_CODE_SCOPE` rather than being a second hand-kept copy of it.

## The generated surface, swept by hand

The five mechanisms all watch the *walks*. Nothing watched the code the compiler
**writes**, so each emitted accessor was mutated in turn — `code`, `area`,
`description`, `message_key`, `severity`, `retryable` — regenerated with
`cargo xtask build`, and put through `cargo xtask check`. All six are caught, by
named tests rather than by counts:

```text
retryable -> true for every code   the_clock_codes_say_retry_where_retrying_works
severity  -> Info for every code   check failed: tests
message_key -> one key for all     check failed: tests
area      -> "KERN" for every code check failed: lints, tests
description -> one line for all    check failed: tests
```

The two directions of the generated-directory rule hold as well: a file carrying
the generated header that nothing generates is reported (*"nothing generates it
any more — delete it"*), and a hand-written file inside a generated directory is
reported (*"move it to a crate of its own"*).

### The chain is injective, and that was a claim nothing tested

Criterion 3 says *any* mutation of *any* field breaks verification, which is a
claim about **injectivity** — and the suite proved it by editing fields one at a
time, which shows each edit is noticed and not that two records differing some
other way cannot land on one digest. A collision is the one tamper a chain
cannot see: swap the record for its twin and every hash, every link and every
`verify` still agrees.

`no_two_different_records_hash_the_same` compares every pair of a hand corpus
built around the shapes where a flattening would collapse two records into one —
a realm and a principal exchanged, an actor chain of two against a principal
spelling the same pair, an absent optional against an empty one, `i64::MAX` and
the value one past it, an action carrying the resource's own text. No collision.

It is a real test rather than a restatement: flattening `actor_chain` to joined
text — one plausible way to write that field — produces a collision the moment it
is applied, between *an actor chain of two* and *a principal spelling the same
pair*.

Two smaller results of the same sweep, recorded because a sweep that reports only
what it found reads as one that looked only where it found something: the log
store's retention flags **are** held against the catalog's declaration, and every
published port **is** held to loopback — both by
`xtask/tests/the_stack_enforces_the_declared_bounds.rs`, which an attempt to
break them finds.

### Two things this sweep looks for and does not find

Recorded because a pass that reports only what it found reads as a pass that
looked only where it found something.

* **The self-documentation surfaces publish every field each registry
  declares.** A scan comparing registry keys against `docs/*.md` reported eight
  as unpublished; every one is a false positive, because the pages use prose
  headings — `reversibility` is *"Can it be undone"*, `order_by` is *"Ordered
  by"*, `pack_reason` is *"Why it has no number"*, `args_schema` is a fenced
  JSON block. A key-name scan is the wrong instrument for a page written for a
  person.
* **The workspace suite is deterministic under repetition.** Twelve consecutive
  `cargo test --workspace` runs, zero failures, after the schema-name truncation
  fix. The concurrency case is covered separately by the paired-run measurement
  in that fix's own paragraph.

## A sixth question, and the gate could not answer it

Every mechanism above asks *did the walk read the file*. None asks **is what the
migrations created still there**. `check` corroborated that migrations were
applied — the checksums, the numbering — and never that the objects they made
still stand. `grep -rn pg_trigger xtask/src/` was empty, and so was
`pg_constraint`.

The role this stack ships with owns the tables, and an owner can always disable
its own triggers or drop its own constraints. Two statements, each leaving
`check passed`:

```text
ALTER TABLE journal_entries DISABLE TRIGGER USER;
UPDATE journal_entries SET action='Escalate';        -- accepted, row changed

ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_hash_is_hex;
INSERT … hash = 'not-hex-at-all';                    -- accepted
```

The second is worse than it looks: the malformed row cannot be removed, because
the append-only trigger refuses the `DELETE`, so the constraint can never be put
back. One statement leaves the table permanently in a state its own migrations
forbid.

`check` reads both catalogues and holds every protection `PROTECTIONS` in
`xtask/src/check.rs` names — the journal's and the access records' triggers and
`CHECK` constraints among them — against the state the migrations left them in.
The names are **written out, not read from the migrations**: a list derived from
the files agrees with whatever the files say, and `0008` drops one of the five
constraints on purpose, so the files declare five where the database must hold
four.

Asking the same question at each level finds three more. **Primary keys**:
`journal_entries_pkey` is what stops two rows sharing
a sequence — the chain reads one row per position and `Store::append` relies on
the collision to notice a racing writer — and dropping it left `check passed`.
**Columns**: `ALTER COLUMN hash DROP NOT NULL` left `check passed`, and a
journal row with no hash is one `verify` cannot read and the append-only trigger
will not let anybody remove. The whole column shape is pinned — name, type,
nullability, in order — rather than a few favourites, because picking favourites
is how a list of favourites grows.

**And the state is `tgenabled` *and* `tgtype`.** Presence is not the guarantee
and neither is enablement — found by making the mistake while restoring the
probe: recreating a trigger as `BEFORE INSERT` rather than `BEFORE INSERT OR
UPDATE` leaves one that is present, that reads `A`, and that lets a caller
`UPDATE` the column it exists to own. The gate reported `is A:7 and the
migrations left it A:23`.

## What none of them can see

Five mechanisms precede this section, and all but the last answer the
same question: **did the walk reach the line?** A sixth is below, and it answers
a third one — whether the guard is still there at all. A canary says a known file was read. A floor says the walk did
not stop early. A two-counter identity says nothing was skipped between the
count and the verdict. A fixture at an unpredictable path says no filter was
written around the fixture, and one planted in every real directory says no
filter was written around a real one — **and that sentence was read for more
than it says.** A real directory is not a real name: every file those fixtures
planted was called `f{n}.rs` or `k{n}.{kind}`, which this repository wears
nowhere, so a clause keyed on a name the tree really has, or on a real relative
path, walked past all of them. Measured, each against such fixtures:
`!relative.ends_with("parse.rs")` hid a hand-written
`NC-KERN-4242` in `crates/nc-canon` from §15.1's walk with the transcript
byte-identical, and one clause naming the policy schema by its real path took
eighteen live citations out of the section gate. The fixtures plant under the
names the tree wears now, each in the directory it wears it in, and
`reached_every_one` refuses a fixture that does not. None of them asks whether
the **verdict is the rule somebody wrote**, and that is where the defects below
live. Measured, each
leaving a real defect in the tree with `check passed` and every number at or
above baseline:

* **A narrowed predicate.** `if declared.contains(&code) { continue; }` in the
  §15.1 judgement — against its own comment saying a declared code counts too —
  and `crates/nc-prune` shipped a hand-written `NC-DB-0004`. Both committed
  fixtures used area `KERN`, which the registry declares no code for, so the
  branch was never exercised from the declared side. That specific hole is
  closed: the fixture plants one of each. The *shape* is not.
* **An iterator adapter.** `joined.lines().enumerate().filter(…)` — the skip
  happens before the loop body, so both counters agree and the floor's quantity
  was fixed before the loop opened.
* **A rewrite of the text.** `joined.replace("NC-", "nc-")` for one crate. Every
  line is read, every line reaches a verdict, and the verdict is about different
  text. The count went *up*.
* **A different judgement.** `if holds_an_environment(name) && !path.contains(…)`
  on the `.env` walk is not a skipped path; it is a path that was looked at and
  cleared.
* **A branch no test reaches, found by instrumenting every branch.** A marker
  appended to each of ~90 branches across four source files, the suite run, and
  markers-inserted diffed against markers-fired names, exactly, the arms nothing
  drives. Nine followed, and the two worst were guards a repair had *added*:
  `Pruned::code()`'s refusal arm (put back to `None`, workspace green) and the
  `readable()` hash check standing in front of a permanent, unrepairable row in
  an append-only table (deleted, workspace green). A fix without a test is a
  fix that can be undone by the next person to touch the line, and neither had
  one.

  The generalisation: **a gate that counts what it read cannot see a branch it
  never entered.** Coverage of files, lines and paths is all coverage of the
  walk; this is coverage of the *judgement*, which the section above says none
  of the other five mechanisms reaches.
* **A check name with two call sites is one call site tested.** Three checks in
  the catalog compiler are pushed from two places each, and the cases beside
  them break the first of each. All three second sites could be deleted with the
  gate green — a log event with an empty description, a field whose snake-case
  form is not an identifier, and two locales that camel-case to one variant all
  passed. The shared name is what made it invisible to review: the string is in
  the test file, so the guard looks tested. **Grep for the check name finds the
  test; grep for the *call site* finds the gap.**
* **A list every consumer derives its expectation from cannot be falsified.**
  `OUTSIDE_THE_ERROR_CODE_SCOPE` was added so the walk, its floor and its
  fixture would read one rule — and that made adding a crate to it a change
  nothing could disagree with: the floor subtracted the new crate in the same
  edit, and the fixture filtered its expectation through the same predicate. One
  line switched §15.1 off over the audit crate, with a hand-written code left in
  its tamper suite and every count consistent. Deriving a floor from a list is
  right; it is not a substitute for **a case per member**, written by hand, in
  both directions.
* **A predicate written twice, where only one copy is driven.** The journal's
  `verdict_after_checking` was a near-copy of `chained_and_contiguous` with one
  check dropped — and four of its six branches were reached by no test, so
  deleting them left the workspace green while a removed or relinked entry got
  the wrong diagnosis. Deleting the *whole* body was caught; exactly the copied
  branches were not. The fix is not a test per branch, it is one predicate: the
  copy delegates, and the main path's tests cover all six.
* **The mechanisms themselves are a list, and nothing enumerated it.** Deleting
  the four lines of a fixture test that plant for one walk moves **no test
  count** — removing statements from a body removes no `#[test]` attribute — so
  a blanket allow over acceptance criterion 3's own suite went green with
  `844 test(s)` unchanged. And `corpus.rs`, the fifth mechanism, could be
  emptied entirely: `SUITES` enumerates `tests/*.rs` integration suites, cargo
  announces the xtask bin's unit tests as `Running unittests src/main.rs`, and
  `declared_tests` floors `passed` against `declared` — both of which shrink
  together when a file of tests goes. `mutants.py` is not run by `check` at all,
  so the meta-gate then had nothing to run against.

  Both are written down and checked against the source:
  `REACHED_BY_THE_PLANTED_FIXTURE` names the walks whose fixture call must
  exist, and `the_corpus_still_decides_every_predicate_it_is_written_for` names
  every table and every driver. **Hand lists on purpose** — deriving either from
  the source would make it agree with whatever the source does, which is the
  trap three gates here have already fallen into.

  A caution learned writing the first of them: a test that greps its own file
  for a needle finds *the line that contains the needle* — its own source — so
  it read its own body as the helper's and stayed green with the helper gutted.
  Anchor on the definition, not the name.
* **A gate with none of the five mechanisms at all.** Not a filter defeating a
  mechanism — an absence of every one. `no_crate_compiles_a_canary` and
  `nothing_includes_what_the_gates_do_not_read` had no canary, no fixture, no
  corpus entry and no unit test: only a line floor, paid at the top of the loop.
  One `continue` below it compiled a canary — a file holding a real registry
  code and a blanket allow *by design* — into `libnc_canon.rlib`, with every
  printed number **up** and `check passed`. The list of walks a mechanism covers
  is itself a list, and one that nothing else enumerates hides the walk it
  leaves off. Both are driven by the every-real-directory fixture.
* **A test whose fixture models the wrong call.** The journal's race test read
  `head()` and threw away the hash, passing `None` — so a racer reading late
  built an entry linking to nothing, which the store correctly refused as a
  broken chain while the test called that a defect. It was red about a quarter
  of the time on an untouched tree, in *the* gate, and its own doc claimed it
  did "exactly what `Journal::append` does". A nondeterministic gate is the
  finding that hides every other finding.
* **A helper that ignores the argument it was given.** The release test passed
  two scratch file names and the helper called the one-file wrapper, which reads
  the shipped list — so every assertion about the first file being restored was
  true because nothing had touched it. The multi-file refusal is exactly the
  case the shipped list does not reach, which is what made it worth writing and
  what made the vacuity invisible.
* **A file the gate never looks in.** Not a filter at all — the gate walked the
  repository's ancestors for a cargo configuration, and cargo also reads
  `$CARGO_HOME/config.toml`, which is an ancestor only by coincidence. On this
  machine the home is an XDG data directory, so a `[build] rustflags` of
  `--cap-lints=allow` written there turned off every workspace lint *and*
  `unsafe_code = "forbid"` over six real violations, with every count
  byte-identical and `check passed`. Nothing about the walk was wrong; its
  corpus was. The lesson generalises past cargo: **ask what the tool reads, not
  what the gate walks.**

  The fix has a second half worth naming. Reporting *any* file in cargo's home
  would fail every machine that keeps a registry mirror or a linker there — an
  ordinary arrangement, and a gate that refuses one is a gate somebody switches
  off. It is judged by the tables that reach the compiler, and `CARGO_HOME`
  itself is deliberately **not** in the flags-that-reach-rustc family for the
  same reason: it carries no flag, it names the file that might.

## The fifth mechanism, and it is not another counter

**Mutation coverage of the judgement**, and it is built: `xtask/src/corpus.rs`
holds `(input, expected verdict)` pairs driven straight through the shipped
entry point, and `xtask/mutants.py` applies one single-token weakening at a time
to the predicate and re-runs them. A weakening the corpus leaves green is a
clause the corpus does not decide — a finding about the corpus, not about the
gate.

The predicates, chosen by what ships if the verdict is wrong — the table is the list:

| Predicate | What escapes if it is weakened |
|---|---|
| `holds_an_environment` → the `.env` walk | a committed secret, in a clone |
| `codes_in` + `glued` → the §15.1 walk | a hand-written `NC-…` the registry can never renumber |
| `lints_put_down` + `LINT_GROUPS` → the blanket-allow walk | acceptance criterion 3's suite with the lints off |
| `is_a_test_attribute` + `runs` → the ignored-test walk | criterion 3's suite not running at all — and the census shares the predicate, so both sides of that floor move together |
| `excused` + `is_the_marker` → the unbounded-table gate | criterion 5 and `§15.8`: a table of personal records growing without limit, with a count printed over it |
| `pinned_by_digest` + `is_a_build_key` → the image gate | the stack a developer and CI bring up running bytes anybody outside this repository can change |
| `caret_uses` → the compile-fail gate | criterion 2: a fixture that names the door and fails for something else, with the type wall deleted underneath it |
| `claim` → the rotting-count gate | a stated count that has drifted, in the documents an agent is told to read first |

**The last two are repairs, and a repair is a predicate too.** The caret rule
answers a fixture rewritten to fail for an unrelated reason, and the counting
rule a count written in digits before a countable noun; with no corpus entry,
`mutants.py` reports five weakenings of them SURVIVED, measured. The corpus is
where a predicate is decided.

One driver per predicate — `grep -c 'decides_every_clause'
xtask/src/corpus.rs` answers how many, and `cargo test -p xtask --bin xtask corpus::`
runs them. The number of *cases* is deliberately not written here: the sum of
the fixed arrays counts subjects that have no row in the table above and misses
the image and referent cases, which are built by functions rather than
declared. A count assembled from two different places is one nobody can keep
true. One case per clause and one per element of each name list, because a list
is a clause per element. Each drives the **real** function over a scratch tree rather
than a re-expressed copy of the judgement, so the walk's scope, the attribute
accumulator and the name lists are all inside what is tested.

**The number is not written here.** `python3 xtask/mutants.py` prints it, and
a number written here goes wrong the same way — a transcript pasted into a
document is a measurement of the day it was pasted. What the runner enforces is
the survivor *set*, against the `EQUIVALENT` list at the bottom of `mutants.py`
where each entry carries its argument; a survivor that is not one of them fails
the run, and an entry that stops surviving fails it too.

The arguments live there rather than here for the same reason: a reader
checking whether an equivalence still holds needs the operator beside it.

**An argument that a clause decides nothing is dead code wearing an argument's
clothes.** "The `Option` cannot be `None` past the guard above it" and "the
filter can never shorten a name the three dot guards let through" are both of
that kind. `holds_an_environment` states the shape once —
`matches!(parts.as_slice(), ["env", last] if …)` over unfiltered parts — with no
empty-part filter, dot guards or length test beside it: deleting any of those is
a weakening no input distinguishes, and the filtered and unfiltered forms agree
over 54,240 generated names, measured. **When the argument for an equivalence
is "the code says this twice", the answer is to say it once.**

Five earlier survivors were real, and none of them was a corpus gap of the same
kind. One was an unsatisfiable clause of `is_a_build_key`, deleted rather than
kept — a clause nothing can decide is not a defence, it is a reason to think
there is one. One was a *reachability* defect: the file pass unquoted its keys
before calling that predicate, so the predicate's own unquoting could not be
decided by any input; the normalisation moved back to the predicate, where a
corpus can judge it. Two were ordinary gaps and are closed. That is the mechanism
doing its job rather than an argument that it works.

**The verdict is the survivor *set*, not its size.** `killed != sum(...)` makes a
clean tree exit 1 for ever, because some mutants are known-equivalent and always
survive — so the exit code says the same thing whether those survive or thirty
real ones do, and the only real signal is a person diffing the printed list
against a paragraph. A gate whose verdict cannot change is not a gate.
The equivalents are written into the runner with their arguments, and a survivor
that is not one of them fails the run — as does an entry that stops surviving,
which is how one of them was found to be no equivalence at all.

**`not applicable` is a failure too.** An operator whose text no longer matches
the source is a clause with **no reported coverage**, and a run that still exits
0 lets operators rot unnoticed, with a reader rather than the meta-gate finding
the drift. A weakening that cannot be applied is neither survived nor killed: it
is a hole.

The runner edits every file in its own `SOURCES` — `gates.rs`, `limits.rs`,
`wall.rs`, `counts.rs`, `dev.rs`, `determinism.rs`, `check.rs`, `references.rs`,
`sections.rs`, `workflow.rs`, the catalog compiler's `lib.rs` and `ledger.rs`,
`nc-canon`'s `lib.rs` and `parse.rs`, `nc-ids`'s `base32.rs`,
`snowflake.rs`, `naming.rs`, `flake.rs` and its own `lib.rs`, `nc-log`'s
`value.rs` and `ship.rs`, `nc-prune`'s `lib.rs`, the policy step's `policy.rs`,
`database.rs` — every gate a live database decides — `nc-policy`'s own `lib.rs`
and `problem.rs`, which decides the line every pack refusal names, and its
`manifest.rs`, which decides what a realm pins, the console's
`store.rs`, which reads what the log store is running with, `main.rs`, whose
five lines hand each verb its flags, `build.rs`, which performs this tree's one
irreversible write, `members.rs`, which says what the workspace is, and the six
crate sources with clauses no other operator reaches — the compiler's `problem.rs`,
`messages.rs` and `model.rs`, the policy pack's `cases.rs` and `value.rs`, and
the log's own `lib.rs`,
and
the two stores that need a database — `nc-journal`'s `postgres.rs` and
`nc-prune`'s `postgres.rs` — the release verb's `release.rs`, the history step's
`history.rs`, the console's `ui.rs`, the
documentation step's `docs.rs`, one integration test,
`the_registry_reaches_the_code.rs`, which is the only place a generated limit is
held against the file it came from, the journal's own `lib.rs`, `entry.rs`,
`memory.rs` and `unit.rs`, the compiler's `load.rs`, `emit.rs` and its own
`docs.rs`, and the rule the compiler judges a command's declaration by, and
the kernel a plugin's — `nc-schema`'s `lib.rs`, `selector.rs` and `descriptor.rs`
— and the decision point the kernel embeds, `nc-authz`'s `lib.rs`,
`directory.rs` and `embedded.rs`, and the kernel's own `lib.rs`, `world.rs`,
`bindings.rs`, `engine.rs` and `built.rs`, the grants and bounds a plugin's
manifest is read into — `capability.rs`, `limits.rs` and `manifest.rs` — and
`host.rs` with every interface of the world under it, `log.rs`, `context.rs`,
`store.rs`, `blobs.rs`, `commands.rs`, `events.rs`, `ui.rs`, `net.rs` and
`timers.rs`, the store a plugin's data is kept in, `backend.rs`, where the
bytes of its blobs are, `blobs.rs`, the
registry its commands are judged into, `registry.rs`, the decision point every
command goes through, `dispatch.rs`, the units of work its writes are kept in,
`units.rs`, where its events go, `logging.rs`, what becomes of a plugin
that is stopped, `supervisor.rs`, what a plugin may ask of the network
and what sends it, `net.rs` and `ureq.rs`, a person's own history,
`history.rs`, the commands the catalog gives the kernel to run, `builtin.rs`,
what a plugin announces, is woken by and shows, `events.rs`,
`timers.rs` and `ui.rs`, and what the kernel does between call trees,
`pump.rs`, the reference plugin's `lib.rs`, `behaviour.rs` and `adapter.rs`, `plugins.rs`,
which builds that plugin into what the kernel loads, the local database's
`lib.rs`, `open.rs`, `schema.rs` and `store.rs` and the journal it keeps,
`journal.rs`, what makes that database's
files private on the system it runs on — `nc-platform`'s `lib.rs`, `unix.rs`
and the test doors `testing.rs` — every name an install gives the system,
`nc-channel`'s `lib.rs`, the daemon's `lib.rs`, `main.rs`, `install.rs`,
`profile.rs`, `device.rs` and `mark.rs`, `nc-prune`'s `sqlite.rs`, and
`local.rs`, which builds that database's schema from its migrations, and restores **all of them** before each mutation: its first multi-file version
restored only the one it was editing, so sixteen weakenings were reported killed
by a corpus that had not seen them.

**And the component the kernel's suites read, which no source write puts back.**
An operator over the plugin's sources is judged by building the plugin from the
weakening, and the kernel's suites panic on a component its sources no longer
build — a panic the runner counts as a kill. Measured: a run that restores the
sources and leaves the component built from its last weakening makes the next
run call every operator over the kernel killed with no weakening on disk. So
the runner builds every plugin from the tree as handed before its first
edit, again after each operator whose command built one from a weakening, and
again after its last, and refuses a run in which a suite says its component is
stale, the way it refuses one that cannot reach its database. The middle build
is the one most easily forgotten: the plugin's operators are not the last in the
runner's table, so without it a kernel operator written below them reads their
last component.

**And a suite that fails with no weakening on disk, which a failure counts as a
kill.** Every command an operator is judged by runs on the tree as handed
before the first edit, and again on the tree put back after the last, and a
run in which one fails is refused: measured, the runner driven with one editor
swap file in `xtask/src/`, which a rule of `check::` refuses, reports a
weakening of a comment as `killed`, the run exiting 0. The second control is what
says a suite started failing part-way through, which makes every kill since
the first one worth nothing.

**And what holds it compares base names as multisets, not as a set.**
`the_canaries_name_every_source_the_meta_gate_edits` takes the file name of each
entry in `SOURCES`, and `lib.rs` alone stands for several of them — so compared
as a *set*, a source whose base name is already in it adds nothing at all:
measured, one extra `crates/nc-log/src/lib.rs` in `SOURCES` left the whole tree
green with the runner announcing "over 39 file(s)". As multisets, this sentence
has to name each file as many times as the runner edits one, with no elision
like "`nc-prune`'s" leaving the filename to the reader. The sentence
above names the crate for each, which is what
a reader needs; the test does not, and saying so is better than reading the pass
as more than it is.

A count of source files written here goes stale with the next source, so this
sentence is held to `SOURCES` by
`the_canaries_name_every_source_the_meta_gate_edits`, because the list is the
runner's and a second copy of it in prose is a copy that only agrees on the day
it is written. `counts.rs` cannot reach this class: the noun is answered by a
Python dict rather than by a directory, which is what every entry of `COUNTABLE`
is.

**Where each half runs.** The corpus is in `cargo xtask check`, inside the test
count, and costs a hundredth of a second. The meta-gate is not: it edits its
sources in place and runs `cargo test -q -p xtask --bin xtask corpus::` **once
per operator**, so what it costs is that suite times the number of operators —
minutes, and more on a machine already busy. It is run when a predicate changes,
and its survivor list belongs in this file.

**No duration is written here on purpose.** A duration is a measurement of one
machine on one day recorded as though it were a property of the runner, and two
files stating two of them disagree by an order of magnitude. A measured pass, on
a machine running five other builds, took 271 s. The rule — one corpus run per
operator — is the part that does not rot.

**What it still does not cover.** Six predicates of the dozen this suite has, and
one weakening operator table written by hand — a real mutation engine enumerates
more operators and costs proportionally more. And an operator only covers the
clause it is aimed at: four of them rotted into no-ops when the source they name
was reworded, which is why `not applicable` fails the run.

And two shapes it cannot reach at all:

* **A file kind the walk *and its floor* both exclude.** `include!("x.in")` put a
  hand-written code into a compiled crate with both sides narrowing together and
  no predicate weakened. Closed by a rule instead — nothing may `include!` a
  file the gates do not read.
* **A rule that checks the *form* of an assertion and never its referent.**
  `pinned_by_digest` decides every clause of the rule somebody wrote, and the
  rule asks the wrong question: sixty-four hex characters is a shape, and docker
  resolves `tag@sha256:…` by the digest alone. Measured, one digest swapped for
  another with the tag left alone — `cargo xtask dev` brought up PostgreSQL
  16.14 under the tag `18-alpine`, with every printed line byte-identical to
  baseline. No walk narrowed, no text rewritten, no predicate weakened; the
  human-readable half of the assertion was simply never checked. Closed by
  asking docker what the tag resolves to, which is a different *kind* of
  evidence and not another counter.

* **An accumulator that swallows lines and counts them as judged.** The
  blanket-allow walk holds an inner attribute across lines until its brackets
  balance — and it stripped comments with `split("//")`, which this repository
  documents 1300 lines away as *not a comment stripper*. An inner attribute whose
  string holds a `//` — a URL in a `#![doc]`, which is ordinary Rust — was
  truncated to a fragment that can never balance, and from that line on every
  line in the file was **appended rather than judged**. All five mechanisms are
  blind: the line floor *rises*, because the walk really does read every line;
  the canary lives in another file and is still found; the corpus never mutates
  the comment strip; the planted fixture is balanced and never enters the state.
  And the two-counter identity was satisfied, because `reached_a_verdict` was
  incremented on the accumulate path — a line that was absorbed, counted as one
  that was decided. Measured, in acceptance criterion 3's own suite:

  ```text
  #![doc = "Threat model: <https://example.invalid/tampering>"]
  #![allow(clippy::all, clippy::pedantic, unused)]

  ok    71155 source line(s) leave the workspace lints on    <- up from baseline
  ok    lints
  check passed
  ```

  with three real clippy violations underneath, and `check failed` the moment the
  `#![doc]` line alone was removed. One line of ordinary Rust turned the gate off
  for everything below it. The identity is the lesson: **a counter on the wrong
  side of a branch makes the mechanism agree with itself while it reads nothing.**

* **A rule that judges the shape of an argument rather than what it names.** The
  rule closing the file-kind shape above took the *first string literal* after
  `include!(` and skipped when it held no dot — so `concat!` walked straight
  through, and so did the canonical `include!(concat!(env!("OUT_DIR"), …))`,
  putting every build-script-generated file outside every gate here. This is the
  form-versus-referent shape again, one level down: the fix for a referent
  problem was itself written as a form test. Closed by inverting the default —
  one string literal ending `.rs` is what passes, and everything else is
  reported.

* **A counter kept honest while the judgement is skipped.** This is the
  accumulator shape in its general form, and it defeats *every* counting
  mechanism by construction. Plant `read += 1; if index > 0 { continue; }` in a
  walk and the count, the floor and the two-counter identity all stay exactly
  where they were while the walk judges one line per file. Measured in three
  separate gates — the static-string scan, the written-count gate, and
  the migration-function gate — each time byte-identical to baseline with a real
  defect planted underneath.

  Adding a line floor does not close it: a floor answers *"was the walk
  narrowed"*, and this narrows the judgement instead. The exact paragraph floor
  added to the written-count gate catches `paragraphs(&text).take(10)` — `93 of
  1549` — and cannot catch the same walk reading all 1549 and judging ten.

  What answers it is not a counter at all but **a defect at a position a filter
  cannot be written around** — and a defect at `pid % k + c`, measured in four
  fixtures, is not an unpredictable position but a **window of `k` lines, with
  `k` printed in the test**: 97, 40, 60 and 30. A filter carrying any constant
  above its window is invisible to it and turns the gate off for everything
  else, and `if index > 300` — the same line as `index > 40` with a different
  digit — passes all four with a real blanket allow in criterion 3's own suite
  and every count up.

  The fixtures plant past **the longest file the suite reads**, measured
  rather than guessed, and assert that relationship. A filter that lets any real
  file's tail through lets the fixture's defect through too, so the test fails.
  That is the property "unpredictable" was standing in for, and it is a
  comparison rather than a hope.

* **A counter placed above the rule it guards, inside the loop.** A counter
  moved *into* the loop and kept above the body repairs the shape above only
  halfway, measured in three gates: `asked += 1` as the first line of
  `nc-policy`'s coverage loop, `read += 1` at the head of the leak scan's line
  loop, `judged += 1` before the counts gate's paragraph rule — each leaves a
  `continue` anywhere below it invisible, and each printed an exact number over a
  gate that had examined less. The rule is the same everywhere: **the counter
  is the last statement a verdict reaches**, not the first statement of the body.

* **An exclusion whose widening moves no number, and a count that cannot fix
  it.** The leak scan exempts one directory. Both the line count and the file
  count sit outside that filter, so adding `&& !file.starts_with("crates/nc-journal/")`
  shipped a real `Box::leak` in the journal with all five printed numbers — 15,
  4, 86, 9, 29 — byte-identical. Counting the exemption *at the call site* does
  not close it either: that compares one constant with another and moves for the
  same reason nothing else does. What decides an exclusion list is **a case per
  thing it could grow to include** — one per member crate, so widening to any of
  them fails.

* **A number pinned only against itself.** Five declared bounds were argued for
  in their own paragraphs and read by every test and every consumer *through the
  constant under test* — so each could drift with nothing failing.
  `PLAUSIBLE_DRIFT_MS` raised from a day to sixty-three years left `check passed`
  and turned a corrupt mark a year ahead of the clock back into "wait and retry";
  `MAX_DEPTH` lowered from 64 to 6 narrowed what the journal accepts;
  `OPAQUE_ID_MAX` lowered to 67 left zero headroom above the longest identifier
  the holder exists to keep possible; `MAX_WIDTH` raised to 20 was caught falling
  and not rising. The floor `pinned > 0` on the policy pack's clause count is the
  same shape in another form — widening the one excuse that skips a conjunct took
  it from 28 to 21, and with it the rule that keeps agent-mediated requests under
  `core.fail_closed`.

  A test that reads the constant proves the code agrees with itself. The number
  goes in the test, with the argument restated, so a change to it has to be
  written twice.

* **A gate whose verdict cannot change.** The mutation runner exited 1 on a clean
  tree, because some mutants are known-equivalent and always survive — so the
  exit code said the same thing whether those survived or thirty real ones did,
  and the only real signal was a person diffing a printed list against a
  paragraph here. The runner judges the survivor *set* against the list
  written into it with their arguments; a survivor that is not one of them
  fails, and neither the count nor the members are restated in this file.

* **A fixture whose window is a number in the source, and one planted past the wrong tree.**
  Placing a defect at `pid % k + c` is a *window of `k`*, with `k` printed in the
  test, so a filter carrying a constant above it is invisible. The replacement
  plants past the longest file the suite reads — and measured against the
  repository alone *that* is the wrong tree: the citation and count gates read the workspace
  documents above the repository, and `NOCTUA.md` is 308 lines longer than
  anything in it, so the fixtures planted inside a gap a filter could still be
  written in. Measured: `if index > 5200` with a dangling citation at line 5352
  of the Master Technical Reference, `check passed`.
  
  Two lessons, both general. **Measure the tree the gate actually walks**, not the
  one the fixture happens to live in. And **assert the relationship**, because
  `longest + 1 > longest` is a tautology that passes with the measurement deleted
  — the number has to be fetched a second time for the comparison to mean
  anything.

* **A fixture at one place, guarding a walk over many.** The counts gate's
  fixture planted its claim in the scratch's own `README.md`, so a filter naming a
  *directory* — `if relative.starts_with("policy/") { judged += 1; continue; }`,
  counter kept honest — was invisible to it while a rotten count under `policy/`
  went unreported. A fixture that answers "did the walk reach the line" has to be
  at every *kind of place* the walk goes.

* **A whitelist that reads text where the format has two spellings.** The cargo
  configuration gate found a table by `strip_prefix('[')`, and TOML's dotted-key
  form is the same table: `build.rustflags = […]` **is** `[build] rustflags = […]`,
  and it is the spelling `cargo --config` itself uses. One line above the shipped
  `[alias]` turned off every workspace lint with `check passed`, while the
  identical setting written with a header was caught. This repository already
  argues the general case one file over — *the question is asked of the parser,
  not of the text* — and this gate was still reading lines.

* **A test that runs a copy of the thing it is testing.** The gate comparing the
  compiler's placeholder scanner with the emitted renderer ran a *hand-written
  mirror* of the renderer, guarded by three `contains` assertions over the
  generated file. So a divergence that left those substrings intact was
  invisible: the emitted `fill` changed to refuse an unmatched `{` — which its
  own doc forbids — and `check passed` with the corpus green, while the identical
  edit applied to the mirror failed the corpus at once. The corpus was right and
  the wiring was not. A private function a test must reach gets a
  `#[doc(hidden)]` door, the way `nc-journal` opens one for `unavailable`.

* **A counter above the `continue`s that discard the work.** `inspected` in the
  policy mutation loops was incremented before the variant was parsed and
  substituted, so a mutation this code could not *build* still raised it:
  measured, with every proposed variant made unparseable, the pinned-clause count
  stayed at 28 with **zero** evaluated — and that number is the only thing behind
  the two findings with no negative test. Unbuildable variants are reported
  rather than absorbed, and the count fell to 20 under the same sabotage.

* **A shared counter read as an absolute while another test moves it.** The log
  shipper's accounting test read two process-wide counters across a window in
  which a sibling hands over four thousand lines — 2 failures in 25 runs, and 2
  of 7 full `check` runs, one of them on an unmodified tree. A gate that fails
  8% of the time for no reason is one people learn to re-run, which is the same
  as not having it.

* **Asking the database which database it is.** Four separate places tried to
  work out, at run time, which schema holds the tables they guard — the migration
  gate, the journal's store, the pruner's transaction guard, and the applied-record
  lookup. Every one of them asked through `search_path`: `current_schema()`,
  `(current_schemas(false))[1]`, `to_regclass`, `current_setting`. Those are the
  same answer spelled four ways, and all four begin `"$user"` by default — which
  is not a schema but an instruction to look for one named after whoever connects.
  One `CREATE SCHEMA` named after the role moved all of them at once.

  Measured: the gate reported `ok 3 migrated function(s) search pg_temp last` over
  a journal that had just taken a foreign row; the store's `append` returned `Ok`
  while the audit log received nothing; and the pruner refused *every* bounded
  table for ever behind a code that says retrying will not help.

  **The schema is a value, in all four.** The store and the pruner are told
  (`in_schema`, defaulting to `public`); `xtask` reads it out of the connection
  string. A deployment that puts its tables elsewhere says so once, where it
  already writes everything else about the connection. Nothing infers it.

* **A cleanup helper that guesses what to destroy.** The journal's test teardown
  dropped `current_schema()` — the first *existing* schema on the path, not the
  one the pool asked for — so on a pool whose path resolved elsewhere it dropped
  the developer's `public` schema, twice, taking the journal, the access records
  and the migration record with it. A prefix list grows by one on each run —
  `nc_test`, then `head_undecodable`, then `recorded_is_assigned`, measured. An
  exclusion list keeping a destructive command honest
  is the shape this repository refuses everywhere else; what it uses is the
  one property every schema the run creates has and nothing shared does — the
  process id.

* **A fixture that plants in one tree while the gate reads two.** The counts gate
  and the citation gate both read the workspace documents *above* the repository —
  `ABOVE`'s own doc calls that "the worst place" for a stale count — and every
  planted defect sat under `repos/`. So a branch keyed not on an index but on
  *which tree a file is in* was invisible to both: `if !inside { judged += 1;
  continue; }` and `if !path.starts_with(root) { reached += 1; continue; }` each
  left every count going *up* and `check passed`, with the defect standing in the
  workspace. Widening the plant past the longest file closed the index-shaped
  filter and said nothing about this one. The rule the fixtures already carried —
  *at every kind of place the walk goes* — had been applied across directories and
  not across trees.

* **A test named as a keeper that builds its bound from the thing it keeps.**
  `a_shutdown_stays_inside_its_own_budget` is named in `FLUSH`'s doc as what holds
  the eighteen-second figure, and its bound is `FLUSH + GRACE + GRACE + 6s` — so
  every value satisfies it. Four of the seven timing bounds in that file were
  unpinned this way; `GRACE` at thirty seconds made the documented shutdown
  seventy-two, and `DEADLINE` at two restored the state its own paragraph records
  as tripling delivery. The relation goes in the test as well as the numbers,
  because a pair of values can satisfy each bound separately and break the
  relation between them.

* **A rule whose subject is a different file from the one it guards.** The
  `[alias]` table of `.cargo/config.toml` was gated because "one line there
  decides whether `cargo xtask check` runs the gate at all" — and the same file
  has a second table deciding whether the gate, once run, can fail. Three lines:

  ```text
  [build]
  rustflags = ["--cap-lints=allow"]

  ok    72909 source line(s) compile no canary        <- every count up
  ok    lints
  check passed
  ```

  over eight real clippy violations in criterion 3's own suite, and it demotes
  `unsafe_code = "forbid"` too. No predicate weakened, no walk narrowed, every
  canary found. The gate permits one table and refuses the rest, and reads
  `RUSTFLAGS` from the environment for the same reason.

* **An exemption that protects nothing and hides a class.** `sections.rs` skipped
  a `§` at the start of a heading, on the rule that "a heading is not a citation
  of itself" — and its `continue` sat above both counters, so no floor and no
  identity could see it. Deleting it entirely cost nothing: the count went from
  1518 to 1564 and not one of the forty-six newly judged citations was wrong,
  because a heading that names a section *declares* it and so resolves like any
  other line. What the exemption actually did was put every `§` at the start of a
  `#` line — including a `#` comment inside a fenced block — outside the gate.
  An exemption whose removal changes no verdict was never protecting anything.

All of these are written here because the next one of their kind will not be
found by any of the mechanisms either. Asking what a *counter* counts, rather
than what a predicate decides, is what reaches most of them — and the
general answer to that whole family is the unpredictably-placed fixture, which is
used by four gates rather than one.

One smaller thing, also said rather than assumed: the `.env` walk judges a
*path* and not a line, so its visitor keeps one counter and a planted secret is
what proves it arrived.

The static canaries stay. They read well in a diff, they survive in a tree with
no `git` to compare against, and they are what says the *shipped* run reached a
real file rather than a scratch one. What they cannot do alone is written
here rather than argued away.

## The sixth mechanism: the gate can only say what is written down

The five above ask whether a walk arrived and whether a verdict is the rule. None
of them asks whether the **guard is still there**, and that is the largest hole
measured here.

Measured: every branch body in nine source files instrumented, the whole suite
run plus `check`, `check --offline`, `build`, `docs` and `release --dry-run`, and
the markers inserted diffed against the markers fired. Then, in one edit, the
twenty-six guards that had never fired deleted — sixteen floors and error
reports in `xtask/src/check.rs`, two enumerate-versus-judge floors in
`xtask/src/gates.rs`, and eight branches of the CI-workflow readers — preserving
each condition as `let _ = <cond>;` so nothing else moved:

```text
cargo xtask check
  ok    formatting
  ok    lints
  ok    documentation
  ok    849 test(s)
  ok    advisories, licences, bans and sources

check passed
```

The same five lines, the same test count, the same last line. Every one of those
guards answers a measured incident, and a guard with no test of its own leaves
the mechanism this repository leans on hardest — a floor under a walk — the one
thing none of the five mechanisms above watches. One of them, the edited-migration report, is
the only reader of `_sqlx_migrations` for that case, and no step compares the tree
with what it held before: with it gone, a migration edited after it was applied
passed the gate.

**What answers it is not another counter or another fixture.** It is a list of
every sentence the gate is able to say — `WHAT_THE_GATE_CAN_SAY` in
`xtask/src/check.rs` — read out of the source by a walk over `xtask/src/` and
compared against a written list. Deleting a guard removes a sentence.

Four things make it hold, and each is the easy one to get wrong:

* **Both populations.** A verdict name (`failures.push`) is what `check failed:`
  prints; a finding's `what` is the line a developer reads. Sixteen of the
  twenty-six reported through the first and the rest through the second, so a
  list of only verdicts would have covered less than two thirds of them.
* **A multiset, not a set.** Five names are pushed from two places each. A set
  reports the pair as intact after one of them goes.
* **Both directions.** A one-way comparison lets a new guard arrive unpinned,
  which is how a list stops being a record of what is there, and it fires for
  real.
* **A statement, not a substring.** The reader's own needle is written in its own
  source, so a reader that searches for it finds itself and reports fragments of
  itself as unwritten sentences — the same shape as a test that greps its own file. A
  real push begins a statement: nothing but indentation stands between it and the
  start of its line, or a match arm's `=>`, which is where eight of the seventy
  findings are pushed from and where a rule saying "indentation only" read none of
  them.

**What it does not say.** That a guard is *right*. `xtask/src/corpus.rs` and
`xtask/mutants.py` are where a predicate's verdict is decided; this says the
guard is still there, which is the question a deletion asks and nothing else was
asking.

**And what it still cannot see**, because these were the branches left over after
it: a filter or a parser branch reports nothing, so deleting one changes no
sentence. Those need a case that decides them — tables of `(input, expected)`
driven through the real function.
The one to remember is `matching` in `xtask/src/workflow.rs`, whose guard against
a `*` in the directory half of a pattern is **unobservable** from any plausible
input: `one/*/a.yml` returns nothing either way, because `read_dir` simply fails
on a path that does not exist. The case that decides it needs a directory
literally named `*`. A guard whose removal changes no output for any realistic
input is one a case has to be *built* for, not found.

## The seventh question: what is running, not what is written

`§19.3`'s hazard has an outward form the five mechanisms cannot reach at all: a
gate that certifies a **file** while the claim is about a **running thing**.

Three of these were open at once. The journal's append-only guarantee is enforced
by PostgreSQL triggers and constraints, and was verified by nothing — six
different tampers all passed. The log store's retention is declared in
`catalog/limits.toml`, `docker-compose.yml` is named as its enforcer, and what
was checked was that those two files agree — while an override file compose reads
without being asked, a `-f` on the command line, or a container still running from
before the file was edited each put a third number between them with both green.
And `POSTGRES_INITDB_ARGS` is read by `initdb` **only when the data directory is
empty**, so a volume made before that line existed keeps the host's collation for
the rest of its life while the file goes on saying `--locale=C`.

All three are closed by asking the running thing: `pg_trigger`, `pg_constraint`,
`pg_index`, `pg_rules` and `pg_attribute` for the journal; `GET /flags` for the
store; `pg_database` for the collation. Each reads a **definition** rather than a
name — a journal check comparing trigger *names* is passed by every one of the
six tampers, because each preserves the name.

The generalisation, and the one to carry forward: **a guarantee held outside the
source tree cannot be checked by reading the source tree.** Ask what the platform
depends on that is not a file here, and then ask what would happen if it were
wrong.

**Asked of those queries' own lists, the same question names one defect many
times over.**
The protections query was a hand-kept list of five system catalogs and the shape
query a hand-kept list of three column attributes — while `xtask/src/check.rs`
says, about those very columns, that *"picking favourites is how a list of
favourites grows"*. Applied to the catalogs themselves it names what was
missing: `pg_inherits`, `relpersistence`, `relacl`, `pg_attrdef`, `pg_sequence`,
three of the five `contype` values, column-level `attcollation`,
`pg_db_role_setting`, and `datlocprovider`. Each is an object a migration
creates or a single statement can change, and each was invisible.

The worst of them needs no privilege and no DDL on the protected table:

```sql
CREATE TABLE journal_shadow () INHERITS (journal_entries);
```

A child carries none of the parent's triggers, constraints or primary key, and
its rows *are* parent rows to every reader. A forged entry went in through the
child; `UPDATE journal_entries` and `DELETE FROM journal_entries` — against the
parent — then both succeeded, because a parent's row triggers do not fire for
rows that live in a child. The same child ends the journal permanently: one row
near the top of the sequence range makes every future append violate a `CHECK`,
and there is nothing in this tree that repairs it. `check passed` throughout,
printing `ok 21 protection(s) …`.

`crates/nc-prune` already knew: its own comment records `ALTER TABLE
access_records INHERIT decoy` and says the journal survived that "only because of
the migration's trigger, which this crate says in as many words it does not rely
on". The pruner closed the hole for itself and the journal's gate never learned
it. **A hazard one crate has written down is one every other reader of the same
object is exposed to**, and nothing in this repository looks for that.

**And the same shape one level out, in cargo.** Every gate here is scoped to a
workspace member, and `crate::members::members` keeps only packages under the
workspace root — dropping the rest silently. One manifest line pointing outside
the tree put a crate holding `unsafe { *at }` and `whatever.leak()` into the
build, outside `unsafe_code = "forbid"`, outside the type wall and outside every
line floor, with `ok 9 member(s) take the workspace lints` and `check passed`.
The list of ways a package escapes the member list had four entries and this was
a fifth.

**What to carry forward.** When a gate reads a *set* of things — catalogs,
attributes, constraint types, package sources — ask what decided the set. If the
answer is "the things somebody had been bitten by", the set is a list of
favourites and the next defect is in what it omits.

## What decided this set?

"What decided this set?", asked of everything that holds one, has found defects
of these five *shapes*:

* **A definition is not the fields somebody happened to need.** The protection
  query rendered a trigger as `tgenabled:tgtype proname:md5(prosrc)` — four
  fields, each added when it was found missing — and `pg_trigger` has more.
  `tgqual` holds the `WHEN` clause: a trigger dropped and re-created with the
  same name, events, function and body, plus `WHEN (OLD.realm = '…never…')`,
  renders **byte-identically**, and the journal was then editable and deletable
  with `ok 41 protection(s) … every trigger's body` printed over it.
  `pg_get_triggerdef` is what constraints and indexes were already using two
  clauses below. `pg_index.indisvalid`/`indisready` are the same shape, and
  `pg_get_indexdef` does not render them: one `UPDATE pg_index SET indisready = false`
  and a primary key stops being enforced without its *definition* changing at
  all — measured, two rows under one key.

* **A needle list cannot enumerate a language.** The type wall refused six
  spellings of a leak, and `Box::<str>::leak(text.into())` is a seventh that no
  list will ever have — a type argument is not a path segment. And the
  `OnceLock` promotion has no spelling at all: it never says "leak". What the
  two have in common is not a call, it is a *shape* — a non-`const` function
  handing back a `&'static str` it was not given. The rule is the shape, and
  the twelve functions that legitimately had it became `const fn`.

* **A vocabulary that only grows by being bitten.** `docker-compose.yml` was
  judged by six needles and two forbidden substrings, so `privileged: true`,
  `user: "0:0"`, `cap_add`, a `/etc` bind mount and
  `POSTGRES_HOST_AUTH_METHOD: trust` were each one line and each certified `ok`.
  The answer is a closed key vocabulary — a key is listed with the reason it is
  allowed, or it is a finding — which is what `SKIP`/`COVERED` already does one
  directory over.

* **A gate that skips more than it says.** `--offline` skipped `cargo deny`
  entirely, and three of its four checks read nothing but the lockfile. A flag
  whose description is vaguer than its behaviour is a flag people misjudge; the
  three that need no network run under it.

* **And the census of what a gate can say has to include every way it says it.**
  `WHAT_THE_GATE_CAN_SAY` scanned `failures.push` and `Finding { what: … }` —
  and a guard that calls `ui::failure` and then returns `Err` is neither, which
  is exactly how `finish`'s `--offline` equality is written. It is the only
  reader of `OFFLINE_SKIPS`, the constant four documents quote, and it could be
  deleted with the console identical. `ui::warn` was a fourth way and was free
  in unlimited quantity: a `return` behind a new `skipped:` line turns any step
  off and exits 0. Four populations, and the two guard clauses that were
  meant to skip a type signature were comparing a *trimmed* string against
  needles ending in a space — unconditionally false, so five entries were
  captures of the wrong literal.

**And one about this file's own advice.** Asked to close the counting gate's
verb whitelist, the obvious move is the shape: a number touching its noun is a
count, whatever verb precedes it. The corpus refused it in one run —
`Stage 10 migrations were written for it` touches the noun too, and so do
`page 12`, `Issue 5` and `Round 3`. Closing that class needs a list of every
word that can *identify*, which is the direction this repository has already
recorded failing. So the whitelist is extended word by word instead, and stays the weak
half on purpose: a word it lacks is a miss, and a gate that is wrong the other
way is worse than no gate. **Not every list of favourites can be closed**, and
the ones that cannot should say which failure they have chosen.

## A scanner narrower than the language it scans

The same defect turns up in unrelated files, and it is worth naming as a shape:
**a hand-maintained list whose only tie to reality is a
source scanner that reads less of Rust than Rust writes.** The floor is
satisfied, the count printed by `check` is large and plausible, and the thing
the list exists to catch walks past.

* `fields_of_the_struct` matched `strip_prefix("pub ")`. A
  `pub(crate) source_address: String` on `Record` is stored, is bound by the
  exhaustive destructure as `source_address: _`, is hashed by nothing — and
  rewriting it left the digest unmoved, so `verify` reported a forged history
  clean. That is acceptance criterion 3 turned false by one word of visibility,
  with `check passed` over 872 tests.
* `promotes_to_static` asked whether *one line* starts with `fn` and mentions
  `-> &'static str`. rustfmt wraps every signature past 100 columns, and this
  tree's long signatures are written that way — so the house style the formatter
  produces was the hole. With a `type Keeper = Box<str>;` alias hiding the leak
  needle as well, caller-supplied text became a `&'static str` under
  `88 source file(s) manufacture no static string`. **How many wrapped ones
  there are is not written here**: this bullet said 89, and counting them the way
  `signatures_joined` does — over the 88 files `check` walks, with comments and
  strings stripped first — answers 152.
* `CHANGES` enumerated four `cfg` keys of about twelve. `#[cfg(unix)]` and
  `#[cfg(debug_assertions)]` each put a line into all nine generated files with
  `two generations are byte-identical` printing over it — the second is a
  *profile*, so `cargo build` and `--release` disagree on one machine.
* The compose vocabulary was a predicate on `indent == 0` and `indent == 4`.
  YAML only requires siblings to share a column, so a service written at six
  spaces was never entered at all: `privileged: true`, `user: "0:0"`,
  `cap_add: [SYS_ADMIN]`, and bind mounts of `/` and the docker socket, resolved
  in full by `docker compose config` and reported by nothing.

The fix in each case was the same in kind: read the language rather than a
convenient prefix of it. Every visibility, the whole signature, the attribute
form as well as the macro, and a structural walk by path instead of by column.

**And the same shape one level up, in this file's own sixth mechanism.**
`WHAT_THE_GATE_CAN_SAY` is a census of every sentence the gate can say, and it
was keyed to the *call shapes* somebody had written down — so a guard reporting
through `wrong.push`, `problems.push`, `missing.push`, `unhealthy.push`,
`outside.push`, `caveats.push` or `ui::detail` was outside it, and a
branch-coverage sweep found 395 cold branch bodies and thirty guards deletable
with an identical console. The receiver is a local variable's name; it was never
the rule. Six populations, matched receiver-agnostically, and the list
is `WHAT_THE_GATE_CAN_SAY` in `xtask/src/check.rs` — its length is the count, and
writing that number here is how this paragraph came to be 37 short of it.

**What a census still cannot see, and this is the honest boundary.** A guard
whose whole verdict is control flow — `return false`, `continue`,
`outside.push((name, path))` with no sentence in it — says nothing, so nothing
can notice it stop. Those need a case each, and these have one: the
`$user` element in a frozen search path (where cases a narrower rule already
refuses leave the clause under test deciding nothing),
the writer of the outside-package list, both halves of the rename rule, the
`guard-unprovable` policy check, and the two fixture-quality floors. When a
guard reports nothing, the only mechanism left is a test that makes it fire.

## One reader for one rule

The cheapest lesson to act on. A `code_without_text` re-implementing Rust's
char-literal rule beside `char_literal_at`, which has it right, got one offset
wrong: for `'\''` the search began at the
*escaped* quote, so the literal closed a character early and every subsequent
quote was one out of phase. Two lines then defeated the fixture-coverage guard
outright —

```rust
#[rustfmt::skip]
const _: bool = matches!('a', '\''|'"');
const _: &str = r#"reached_every_one(&findings, &planted, "the §15.1 walk")"#;
```

— after which the §15.1 walk's assertion could be deleted and a one-line filter
hid a hand-written error code with every printed number at its baseline. The
same window missed `'\u{100}'` and every longer escape, which were emitted whole
as *code*.

A rule written twice is a rule with two answers, and the second copy is the one
nobody tests. In `gates.rs`, `code_without_text` asks `char_literal_at` rather
than keeping a reading of its own.

## The output nobody compared: prose

Everything a generated page carries that comes *from* a registry was compared
against that registry — every code, severity, limit, parameter, sentence and
column. What the emitter *authors* was compared by nothing.
`each_page_names_the_registry_it_came_from` reads line two; from line three down
to the first table, `docs/*.md` was free text. Measured, with the whole gate
green:

```text
docs/errors.md   "A code is **recyclable**: once the logs that mention it have
                  aged out, the number is free again and may be repointed at a
                  different error, citing §15.1 for it"
```

— the page whose subject is the append-only rule, citing §15.1 for its
opposite. The same hole ran through `catalog/errors.reserved.toml`, whose
checksum covers its reservations and not its header, and which is deliberately
not drift-checked: rewritten to *"Advisory only: a line here may be edited or
deleted by hand"*, the file that exists to make "never reuse a code"
enforceable shipped instructions to reuse one.

Both are pinned, the way `WHAT_THE_GATE_CAN_SAY` and `PROTECTIONS` are: 91
lines of page prose and every line of ledger header, written down, because there is
nothing in the registry to derive them from. **A generated file is not only the
values it interpolates.**

## An exemption is a gate switched off for a named case

`curve-ends-cheap` refuses a rate-limit curve whose last delay is under a
second, because the last value repeats for ever — and exempted any budget with a
lockout. The exemption was `lockout_after.is_none()`, which is the lockout
*existing*, not the lockout *arriving*:

```text
login: delay_ms = [0, 0, 0, 0, 0, 0, 1], lockout_after = 1_000_000
  cargo xtask check  ->  check passed
```

A million tries at a millisecond each is about seventeen minutes of unthrottled
guessing, on the surface whose own comment in `catalog/limits.toml` says the
escalating delay is what carries it. The exemption requires the lockout to
fire within the curve's own length, and **both halves are tested**: the case the
exemption covers, and the case it must not. An exemption with no test for its
hole is an exclusion list, which this file has a section about already.

`untranslated` was the same shape one level down: `ours == theirs`, byte for
byte, so an English sentence pasted into `pt-BR.toml` with a trailing space, a
different capital or a doubled space passed as a translation and every gate
downstream read it as translated. A rule about sameness has to say what
sameness means.

## One escaper, two languages

`crate::quoted` writes a *Rust* string literal, and everything the compiler
emits is Rust or Markdown — except one file. `catalog/errors.reserved.toml` is
TOML, and Rust spells an escaped control character `\u{7}` where TOML wants four
hex digits and no braces. A BEL in an error's description — prose a developer
writes freely, refused by nothing on the way in — therefore *rendered a ledger
that TOML cannot read*, and rendering it is the one write in this repository
that cannot be taken back:

```text
"NC-DB-0001" = "... a bell\u{7}"
  toml::from_str               ->  too few unicode value digits
  every later build and check  ->  catalog/errors.reserved.toml is not a ledger
```

The round trip is the test — `render` then `read`, over a description holding
every character that broke a different arm — because what was wrong was that one
language's spelling was used for another's, and only the round trip can tell.

## A counter counts what it counts, not what its sentence says

`build`'s ledger counter incremented once per *write of the ledger file* and its
sentence announced `1 appended … which is permanent`. Editing a comment in
`ledger.rs` and rebuilding therefore announced a permanent reservation, twice,
with the reservation set unchanged at 28 both times. A developer told a number
was burned goes and tries to take it back.

The count comes from `newly_reserved`, the same list the lines above it are
printed from, so the count and the names cannot disagree — and a rewrite that
reserves nothing says exactly that.

**And the census cannot see a sentence a helper returns.** `ui::detail(&what_the
_ledger_write_was(&reserved))` has no literal at the call site, so
`WHAT_THE_GATE_CAN_SAY` never sees it — the same blind spot
`each_audit_says_what_it_ran` was written for. A sentence built away from its
call site needs a test that spells it out.

## A rule written twice

The same rule, in a second place. `outside_of` knows that
`/w-vendor` is a sibling of `/w` and not a child of it; `from_metadata`, further
down the same file — the function whose answer is the scope of
every member-keyed gate in the tree — has to know it too, because stripping the
root as a bare textual prefix there admits the package with the relative path `-vendor/nc-vendored`. <!-- allow-reference: `-vendor/nc-vendored` is the nonsense path the defect produced -->

The lesson is not "check for siblings". It is that a fix applied where a defect
was *found* is not applied where the defect *is*: when a rule turns out to be
wrong, the next thing to do is grep for the other places that spell it.

## A counter with a test, and a sentence with none

`a_full_queue_drops_and_counts_rather_than_blocking` reads `shipper.dropped()`
and then leaks the shipper, so the arm that turns that number into a line an
operator can see never ran anywhere in the suite; deleting the `to_stderr_line`
call inside it left everything green. A dropped log line is then a number nobody
is looking at, in the one component whose promise is that a store being down
cannot stop the platform.

Observing it takes a child process — a test cannot redirect the harness's own
stderr without disturbing every other test in the binary — which is the same
apparatus the `/dev/full` pair already needed. **The counter and the sentence are
two guards, and each needs its own case.**

## A number pinned in four places, over a list that rotted anyway

`OFFLINE_SKIPS` exists because the count of steps `--offline` skips was a
literal in one file and prose in three others, and all four had rotted.
`the_documents_agree_about_what_offline_skips` holds `README.md`,
`AGENTS.md`, the `justfile` and `--help` to that one number, and every one of
them says *eight*.

The `justfile` then listed five of them:

```text
cargo xtask check --offline
  warn  skipped: --offline, and the journal's own protections live in the database
  warn  skipped: --offline, and only the running log store states its own bounds
  warn  skipped: --offline, and how a database sorts is a property of that database
```

— three steps that no document but `README.md` and `AGENTS.md` mentioned, and
they are the journal's tamper-evidence, the store's own bounds, and the
collation the database was initialised with. A reader skimming the alias they
actually type is told eight and shown five, and the three missing are the ones
they are most likely to assume were checked.

Held by anchors that are tied to what the gate prints: each anchor must
appear in exactly one `skipped: --offline` sentence of `WHAT_THE_GATE_CAN_SAY`,
every such sentence must be claimed by exactly one anchor, and the number of
those sentences must equal `OFFLINE_SKIPS`. **Pinning a count is not pinning
what it counts.**

## A workflow that could never have gone green

CI declared both halves of the stack as GitHub **service containers**, at the
digests `docker-compose.yml` pins, and a rule held the two files to naming the
same images. Both matched. The workflow still could not pass, and the reason is
that a service container takes an image, an environment, ports and `docker
create` options — and *not a command*, which is how the compose file hands the
log store its retention policy:

```text
docker run … victoria-logs   ->  GET /flags: 200, Content-Length: 0
docker compose up logs       ->  -retentionPeriod="30d" …

error the log store answered `/flags` with nothing
error check failed: the log store reported no flags
```

Two more followed from the same cause. Nothing applied the migrations to CI's
database, so three steps announced a skip and the run passed — a green CI run
was compatible with the journal's forty-four protections never being read. And
the health command copied into the workflow was `psql -h 127.0.0.1`, which
`initdb` writes a `trust` line for; measured inside the container with a
deliberately wrong password it returned a row and exit 0, while the compose
file's `-h "$(hostname -i …)"` was refused — a rule `docker-compose.yml` carries
thirty lines of measurement about, in a file nothing held to it.

**The rule was comparing the wrong thing.** Images are what the two files have in
common; what CI needed was the *stack*, which only `docker compose` can produce.
The `linux` job runs `cargo xtask dev` and declares no services, and the gate
asks exactly that: no image named in the workflow, and `cargo xtask dev` before
`cargo xtask check` — read from the job's *commands*, because the block
explaining why it is there would otherwise satisfy the rule that requires it.

## An empty database is not a request

Three steps skipped when the database had applied no migration, and the run said
`check passed` with exit 0: the migration corroboration, the `pg_temp`
resolution, and all forty-four journal protections. The reasoning was written
down and was about CI — "this sentence is what a *fresh* database produces —
which is every CI run" — so the shape of a broken workflow had become the
justification for a gate that reports nothing.

`--offline` is how a developer asks for those steps not to run. An empty database
is not a way of asking; it is a database to migrate. All three are failures,
and each says `cargo xtask dev`. The states are a named enum rather than two
bools, because *unreachable* and *empty* need different sentences: "this database
has applied no migration" is a claim about a database, and saying it of one that
never answered is two failures reported as one.

**And the record was walked in one direction.** Every applied migration was
checked for its file and its checksum; no file was ever checked for having run,
and the floor beside it was `max(applied, files)`, which `applied < files` cannot
fail. A migration added without `cargo xtask dev` printed `13 migration(s)` and
`12 of them are recorded applied` two lines apart, compared neither, and passed.

## A gate that mandates the configuration it warns about

The journal's protection list pinned `ownedbyme=true` and `grants=default` —
that the connecting role *owns* the journal, and that no grant has ever been made
on it. Both are wrong, in opposite directions.

`AGENTS.md` says of `ALTER TABLE … USING`, `DROP TRIGGER` and the rest: "All of
them need table ownership or more, and the answer to them is **a role that owns
nothing**". The gate required the opposite, so it could never be pointed at a
hardened database to confirm the protections are in force there — which is the
whole argument for reading a live database at all.

And `relacl` never returns to `NULL`:

```text
GRANT INSERT, SELECT ON journal_entries TO app;   relacl -> {noctua=…,app=ar/noctua}
REVOKE ALL … FROM app;                            relacl -> {noctua=…}
DROP ROLE app;                                    relacl -> {noctua=…}
```

so the first time anyone granted the application role `INSERT`, every run
afterwards reported an audit-log breach that had not happened, permanently.

The field says `writers=`: how many roles other than the owner hold `UPDATE`,
`DELETE` or `TRUNCATE`, over `aclexplode(COALESCE(relacl, acldefault(…)))`.
Measured: `GRANT INSERT, SELECT` passes, `GRANT UPDATE` fails, `REVOKE` passes
again. **Pin the property, not the configuration that happens to have it.**

## A property of the data is not a protection

The same list carried `agrees=`: that `access_records`'s own insertion order
agrees with the *text* order of `id`, a client-generated Snowflake. Two ordinary
rows break it — two nodes writing in the same millisecond with different machine
flakes, the higher-flake one committing first — and the operator is told their
audit log is editable:

```text
error `access_records.recorded` is `ordering distinct=true agrees=false` …
error check failed: 4 of the journal's protections are not in force
```

`catalog/limits.toml` refuses `occurred_at` as an ordering column in as many
words — "it holds whatever wrote the row" — and `id` carries the same writer's
clock. It also expires on its own: identifiers grow a character in 2034, after
which an alphabetical sort stops matching a chronological one and the assertion
is false unconditionally.

The neighbouring defect is smaller and the same shape: the sequence branch
filtered on the schema and not on the two tables, so any `BIGSERIAL` anywhere in
`public` was reported as an unexplained object on the journal, under a paragraph
about `UPDATE`, `DELETE` and `TRUNCATE` on the audit log.

## A scope written as a path

`Literal`'s tuple field is private to `nc-log`, so "no public function hands one
out" is a rule about **the crate**. It was written as `file.contains("nc-log/src/")`,
and Rust does not agree with that spelling — a private field is in scope for
every module of the crate, wherever its file lives:

```text
#[path = "../doors.rs"] pub mod doors;                   one directory up
#[cfg_attr(all(), path = "../../nc-canon/src/x.rs")] …   another crate's directory

  pub fn remembered(text: String) -> Literal {
      Literal(HELD.get_or_init(|| text).as_str())        // a OnceLock, no `leak`
  }
```

Both compiled a run-time `String` into `Origin.service`, which is on every line
the log emits, with `check passed`. And the reader that decides which files are
in scope at all matched `#[path` at the start of a line, so the second one was
invisible to it — the same `cfg_attr` lesson `gates.rs` records for the canary
walk, fixed there and never carried across.

Scope comes from the module tree the caller already resolves. **A rule about
a crate cannot be spelled as a directory.**

## The generated file has names of its own

An event named `field` generated `pub struct Field` into a file that opens
`use crate::{Event, Field, LogValue as _};`. `cargo xtask build` reported
`15 generated file(s), 3 rewritten` and the next `cargo build` failed inside
generated source, whose header forbids editing it. `event` and `level` collide
the same way; `vec` and `option` shadow the prelude instead, which surfaces as an
`E0107` on a method written for every event.

The keyword list beside it had the same shape from the other end: six entries, of
which five are not Rust keywords, compared case-sensitively — and a second caller
hands it *snake*-cased field names, so `type`, `ref`, `match` and `in` all passed
and the build died in rustfmt at an offset into a stream.

Two lists, one per cause: what Rust reserves and the prelude names, and what
this particular generated file imports. **A name check that only compares the
registry against itself has not met the file it writes into.**

## A rule the document already decided

`NOCTUA.md §15.1` names the areas: "`AUTH`, `ACCT`, `APPR`, `KERN`, `PLUG`,
`BLOB`, `P2P`, `AI`, `GW`, `DB`, `VAL`, `LOG`, …". `catalog/errors.toml` declares
`P2P` and `docs/errors.md` publishes it — and `area_of` required every byte to be
`is_ascii_uppercase`, so the first peer-to-peer code was refused as *malformed*,
in a message about the code's shape, while the cause sat thirty lines above and
was never mentioned. Nothing validated the keys of `[areas]` at all.

The fix is not a judgement call: the document had already made it. Where the code
and `NOCTUA.md` disagree about a rule the document states, the document is right
— and both halves of the rule read the same alphabet, so they cannot disagree
in silence again.

## The class a gate that checks a pack against itself cannot report

A sweep of the policy pack has found defects that are each a **rule that is
absent**, not a rule that is wrong. The gate around `policy/` is unusually
strong — a conjunct-and-branch mutation engine, a written `PINNED` count, a
denial case per policy, and a sweep that deletes each policy and requires its own
case to go red. Measured with the written rule lists deliberately switched off,
every one of the policies is still caught by its own case. Nothing in that
machinery can notice a request nobody thought to ask about.

The two that mattered:

```text
ana Share Content::"vault"   -> Deny  by family.sharing_restricted_…
ana Grant Content::"vault"   -> Allow by family.a_guardian_administers_the_realm
ana Read  Content::"recipe"  -> Deny  by []          <- carla's private content
ana Grant Content::"recipe"  -> Allow                <- the same content
```

A guardian could hand a third party the key to content the guardian herself is
refused, with no quorum and no rule naming it — and the policy's own comment,
four lines above the rule, records that granting had been measured permitted and
then covered `Share` alone. The second: `Grant.valid_from` appeared in the schema,
in the reference's provenance list, and in **no `.cedar` expression at all**, so a
grant that ran out last January was refused and the identical grant that starts
next June authorized today.

What finds this class is asking, of every attribute the schema declares, *which
policy reads it* — and of every verb, *which rule stops it*. Both are greps, and
neither is a gate.

## A write that can be interrupted is a state somebody has to be told about

`catalog/errors.reserved.toml` is the one permanent file here, and it was written
with `fs::write`, which truncates before it writes. Interrupted — measured under
`ulimit -f`, the shape a full disk produces — it left 4096 bytes of 5090, 22
reservations of 28, and **no error line at all**. Then the composition:

```text
cargo xtask check
  error catalog/errors.reserved.toml:17 — does not match its own checksum, so it
        was edited outside the compiler
        … clear the checksum and run `cargo xtask build --reserve`

cargo xtask build --reserve
  ok  reserved NC-VAL-0001 — permanently
```

`reconcile` re-derives every truncated-away line from `catalog/errors.toml` *as it
reads today*, so a meaning change the previous build had refused was ratified in
silence — and the build that gets interrupted is normally the one run *because*
the catalog was just edited. The ledger is written to a sibling and renamed,
so the state cannot arise from an interruption; the diagnostic no longer blames a
person for it, and says what `--reserve` actually does before offering it.

`release` has the same shape and no fix inside the tool: interrupted between the
manifest rewrite and `cargo update`, it leaves `Cargo.toml` ahead of `Cargo.lock`,
and `.cargo/config.toml` puts `--locked` in the alias — so **all five verbs refuse
at once** and the tool cannot be run to undo its own write. A guard inside `xtask`
is unreachable there, by construction. What exists instead is one line printed
*before* the window, and the recovery written in `AGENTS.md`. Where no gate can
stand, say so and write the sentence.

## A claim printed before the thing it claims

`build` announced `reserved NC-VAL-0002 — permanently` from a loop that ran before
the write loop. Measured with one generated file made read-only: the line printed,
the build failed, and `grep -c NC-VAL-0002 catalog/errors.reserved.toml` answered
`0`. `AGENTS.md` states the contract in as many words — "announces each code it
reserves, **on the line it reserves it**" — and a developer told a number was
burned goes looking for the undo, whose first step is blanking the ledger's
checksum.

The same failure said only which file it could not write. It says how many were
already rewritten now, because a tree part-way through a build is a state the
person in front of it is in.

## A word that names a stage and a state at once

`docker compose ps` leaves the Health column empty for a container that declares
no healthcheck. `dev` read empty as `"starting"` — a transient — and waited the
full 120 s for a permanent state to change, then printed a diagnosis about
PostgreSQL data directories. Measured twice, `2:00.48 total` each time. It is a
failure at the first poll, naming the service and what it lacks.

Its neighbour: `--down` printed `the stack is down and its volumes are gone` over
compose's own `Warning: No resource found to remove`. That matters when it is run
from the wrong checkout — a compose project is named for its directory — and the
developer is told the volumes are gone while another project's stack still holds
the ports.

## Prose describes a behaviour, and the behaviour changed

An empty database is three failures, not three skips, so a document describing
the skips as the intended shape, transcript
and all — "**five** announcements and a pass" — is a document telling a
reader to restore the hole. A reader following that reproduction gets a red run
and concludes their copy is broken.

That is the cost of a measurement written into prose: it is a claim with a date on
it and no gate. A count of protections, of sentences, of what `--offline` skips
— each written into prose drifts from the list it counts, and one sat in a file
the number's own gate already opens, in a sentence shape it did not match. That
shape is in the list. **Where a number can be read from the tree, say the command and not the
value.**

## A row's address is not the row

The pruner deleted by `(tableoid, ctid)` — where a row physically sits. Another
transaction's `UPDATE` moves it, PostgreSQL re-checks the new tuple against the
list, does not find its address there, and **skips it**. Measured with no
injected delay at all, an ordinary single-row `UPDATE` loop beside a batch of
5000:

```text
removed 4991 of 5000, rows left 15009 where 15000 was expected
```

The half that makes it permanent: `access_records.recorded` is preserved across
an `UPDATE` by `0006`'s trigger — written so that "a row that can renumber itself
can make itself immortal" — so the skipped row keeps its place at the head of
every later batch and is skipped again. Five rounds, `removed 3 of 4` each time,
the same row surviving, every run reporting `Ok`. `§15.8`'s only enforced bound
stops converging in silence. The `UPDATE` is not hypothetical: `§15.6` fills in a
city on an access record after the fact.

`SELECT … FOR UPDATE` is worse than the defect: it needs the `UPDATE`
privilege — measured, the suite's probe role holding
`SELECT, DELETE` was refused outright — and granting the pruner `UPDATE` on
`journal_entries` is the one privilege the append-only design exists to withhold.
A **primary key** does not move: the re-check compares the new tuple's key
against the same list and deletes it, in one statement, with no privilege the
pruner did not already have.

**A guard with one side.** `over_deleted` refused taking *more* than the batch and
there was no mirror, so taking fewer read as an ordinary run. A bound that stops
converging is exactly the failure a bound exists to prevent.

## The gate denies more than rustc does

`printable` stops a control character and a bidi override from reaching a `///`
in generated code, and its own paragraph explains why: prose from a registry
lands in a file whose header forbids editing it. The repair was made as wide as
*rustc's* denials. The gate runs `cargo doc` with `RUSTDOCFLAGS=-D warnings` and
`cargo clippy -- -D warnings`, and both deny more:

```text
description = "Migrations were applied, from <version> to <version>"
  cargo xtask build  ->  ok  15 generated file(s), 3 rewritten
  cargo doc          ->  error: unclosed HTML tag `version`

{ name = "Applied" }
  cargo clippy       ->  structure field `Applied` should have a snake case name

{ name = "max-rows" } beside { name = "max_rows" }
  cargo build        ->  error[E0124]: field `max_rows` is already declared
```

Each is an ordinary thing to write in a registry. The last is the sharpest: the
field loop deduped on the name as *written*, while `snake` turns `.` and `-` into
`_` — and `Names::claim`, forty lines up, does exactly this for every top-level
registry name and says so in its own diagnostic. **A name check that only
compares a registry against itself has not met the file it writes into.**

## An escaper knows one output language

`cell()` escapes `\`, `|` and the line separators — Markdown *table* syntax — and
a page is Markdown *with HTML in it*. A sentence from `catalog/i18n/en.toml`:

```text
"The activity history is not answering. Ask <your administrator> to look."
  accepted, no diagnostic, published verbatim
  pandoc -f gfm -t html  ->  <td>Ask  to look.</td>
```

The reader of `docs/errors.md` sees "Ask  to look." while the application shows
the whole sentence — on the page that exists so a reader can compare the two
language columns against what a user is told. This repository already splits its
escapers by output language (`quoted` for Rust, `as_toml` for the ledger); the
page's had been split by *syntax layer* instead.

## A value that says nothing

Five accepted with no diagnostic, each on a field whose own documentation says
what it is for: a description of three spaces ("the only English an operator
reading the log gets"), a command with no name (how a command is invoked, how its
page titles the section, the key `nc_catalog::command()` takes), an area that
says nothing about what it covers, a locale direction that is neither `ltr` nor
`rtl`, and a `bcp47` that is not an identifier — the one generated name
`load.rs`'s header promises to check and did not. Two rules in that same file
already trimmed before testing; the two beside them did not.

## One clock, the crates that read it, one guard

`nc-journal::now_ms` is `nc-ids`' `SystemClock::now_ms` with the guards removed:
its `map_or(0, …)` turns a clock set before 1970 into `recorded_at = 0`, hashed
into the chain and stored in a `BIGINT NOT NULL` with no `CHECK`, where `nc-ids`
follows the identical expression with `within_range` and answers
`MintError::BeforeEpoch`. A journal entry's timestamp is inside its digest, so an
entry claiming 1970 is an entry that verifies for ever. It is a refusal, with
a code and a sentence. **Copied code keeps the shape and loses the reason.**

## A gap left open on purpose: two `xtask` runs

Nothing in `xtask` takes a cross-process lock. Cargo's own build lock is released
before a verb starts, so two `cargo xtask build` runs can write the same
generated files, and a `check` can compile from a `target/` a `build` is
rewriting. The ledger's temp file is named for the process rather than one
shared path.

**Not closed, and this is the argument.** An attempt to produce a corrupted
tree this way fails, measured: two builds render identical bytes, so the overlap
has no visible effect unless `catalog/errors.toml` changes between them. A lock file would trade
that for a failure mode that is worse for the person in front of it — a stale
lock after a crash, in a repository whose whole command surface is five verbs and
whose recovery instructions cannot assume a sixth. If a build ever does interleave
visibly, the fix is a lock and this paragraph is where the decision was made.

## The eighth mechanism: instrument the tests, then run them once

Every mechanism above judges the *subject*. This one judges the **tests**, and
does it mechanically rather than by suspicion: a marker in the body
of all 398 `for` loops in test code, and one inside all 214 `.all(`/`.any(`
closures, then one run of the suite.

```text
393 of 398 loops provably executed
202 of 214 iterator closures saw at least one element
```

The five loops and twelve closures that did not are the whole finding, and they
are the kind nothing else here can see. An assertion over an empty collection is
*true*, however the subject behaves — `!xs.any(p)` cannot tell "reported nothing"
from "reported something else" — and the sharpest instance was in a `§15.8` test:
`Rows::with("journal_entries", …)` leaves every other table at zero and the
journal is `PolicyPack`, so the "end to end" half was satisfied by a `run()` that
pruned nothing. It seeds a second table, and asserts that something *was*
pruned before asserting what was not.

**What the sweep costs is one run.** What it replaces is a person guessing which
assertions might be hollow.

## A test that names a field and edits another

`changing_any_field_of_any_entry_breaks_verification` is criterion 3's proof, and
`every_field_of_a_record_has_an_edit` is what keeps its list honest. The list was
compared by *name* — `FIELDS` against the struct's fields, `FIELDS` against the
labels `edits()` carries — and nothing tied a label to what its closure did:

```text
the edit labelled "grant_used" edits record.action instead
  cargo test --workspace  ->  green
```

Every test that loops over `edits()` asks only whether *something* changed, so
"any mutation of any field" was proved as "some mutation of some field". Each
edit is applied to a fresh record and the field that moved in the canonical
form has to be the one the label names. (The property itself survived — dropping
three fields from `hashed_form` alongside the relabel went red on the golden
vector — so this was a false claim in a test name rather than a hole in the
chain. Both are worth fixing; only one is worth being frightened of.)

## Where a constructor is asserted by where the row did *not* go

`Postgres::new` is the constructor a deployment uses, and both crates' versions
could be aimed at a schema that does not exist with the whole gate green.
`nc-journal`'s only caller bound `let landed = …` and never asked whether it was
`Ok`; `nc-prune`'s had no caller at all — a systematic scan found it the only
public function in the crates with none.

The journal's test cannot simply assert `is_ok()`: it runs as a role with no
grant on the real table, and making the write succeed would put a row in the
append-only journal for ever. What distinguishes the two schemas is the *reason*
it was refused — `permission denied` reaches a table that exists, and a schema
that is not there answers `relation … does not exist`. **When you cannot assert
the outcome, assert what only the right path can produce.**

## Two clauses that no test can reach, and what to do instead

`ONLY` in the pruning `DELETE`, and the classification of an unreadable head
hash: both are guarded by something upstream — the reach guards refuse every
inherited relation, and `0009`'s constraint refuses a hash that is not 64 hex —
so neither arm runs, and both mutations survived. That is not a reason to leave
them undecided: the repository already answers this with a *source* assertion
(`the_over_deletion_guard_runs_before_the_commit`), and both have one.

A source assertion is weaker than a behavioural one and stronger than nothing. It
says: the clause is still written where the decision is made.

## A gate whose only floor is "more than zero"

`EXTENSIONS` decides whether a token in a document reads as a path. Its two
sibling lists each have a membership test anchored to the tree; it had none.

```text
"yml" dropped from EXTENSIONS:
  ok  1342 citation(s) resolve  ->  ok  1261 citation(s) resolve
  check passed                  ->  check passed
```

81 citations, 6% of the gate, and nothing between them: `inspected_something`
rejects zero and `read > 20` was measured against 1342. **A floor that a
catastrophe passes is not a floor** — and the sibling lists were given theirs
after exactly this defect, in a change that did not think to look one constant
over.

## The consumer nobody had been

Read as its author reads it, this repository misses what a consumer is *told*.
Read as the kernel — a crate that takes `nc-catalog` as a dependency and writes
against what it offers — it has shown two defects, both about that.

```rust
/// Changes state the kernel can reverse.
Write,
```

That is `crates/nc-catalog/src/commands.rs`, generated. The page generated from
the same enum says *"not automatically — it changes state and nothing here
reverses it"*, and `docs.rs`'s own comment calls the first sentence "a safety
claim nothing supports": `limits.prune` carries `write` and deletes with no
tombstone and no copy. The guard written for exactly this read **the page**, and
its needle did not match the crate's wording anyway.

The second: `render` was the one public item the crate root did not re-export,
and it is the one that turns `{field} must be between {min} and {max}.` into a
sentence. A consumer writing `use nc_catalog::*;` gets `message`, which returns
the placeholders.

**A generated crate is an API before it is an artefact.** What it says in a doc
comment is what its first consumer will believe, and the page beside it is read
by nobody who is writing code.

## An error you can build is an error that can lie

Four types in this tree could be constructed to say something untrue, because
their fields were public and their constructors were the only thing that upheld
an invariant:

```text
FlakeOutOfRange { value: 5 }   "a machine flake of 5 is outside 0..1024"   NC-VAL-0001
ParseError { code: JournalChainBroken, … }   from the canonical parser, critical
Entry { sequence: 9_999, previous: Some([7; 32]), hash: [0; 32] }
Command { action: "Read", mutating: false }  over a Delete command
```

The third is the sharpest: `entry.rs`'s own header says a `Record` "cannot carry
a sequence number or a hash, so no caller can place an entry anywhere but the end
or claim a link it did not compute" — and `Entry`, which `Store::append` takes,
carries both. All four are `#[non_exhaustive]`, with accessors where a
consumer legitimately reads them and one `forged_for_tests` door named for what
it does.

`TableLimit` deliberately keeps its literal, and the reason is written where the
decision was made: twelve fixtures in `nc-prune` build one, and those fixtures
are the tests that keep `the_catalog_disagrees` — the runtime guard against
emptying `access_records` through that very door — honest.

## "Did not answer" and "answered, and refused" are different systems

A revoked `GRANT`, a dropped table and a stopped container all reached a caller
as one code:

```text
INSERT revoked (42501)   permission denied for table journal_entries
table dropped (42P01)    relation "journal_entries" does not exist
the store stopped        pool timed out while waiting for a connection

all three -> NC-JRNL-0003 / critical / retryable
          -> "The activity history is not answering right now. Try again in a moment."
```

`§14.9` makes the journal fail closed, so the platform stops on any of them —
and the only instruction anybody gets is *try again*, for ever. `nc-prune` had
drawn this line since it was written (`NC-DB-0005`: "refused, and retrying will
not change it"); the journal had not, while its own file states the rule three
times.

`NC-JRNL-0009` exists, reserved deliberately and permanently, for the
SQLSTATE classes that mean the store answered: `42` access rule, `28`
authorization, `3D` catalog, `3F` schema. **The code is what the log is queried
by, so two conditions sharing one code are one condition to everybody who reads
it afterwards.**

## A message that names a command nobody can run

`dev`'s failure messages said `docker compose logs`. `project_for` names this
stack `noctua-central-<digest of the path>` on purpose, and a bare
`docker compose` resolves to the *directory basename* — which is by construction
never that name:

```text
$ docker compose ps -a
NAME   IMAGE   COMMAND   SERVICE   CREATED   STATUS   PORTS
$ docker compose -p noctua-central-44f26d14 ps -a
noctua-central-44f26d14-postgres-1   …   Up (unhealthy)
```

The developer reads "no containers at all" and goes to look at the daemon. The
correct incantation was already in the file — on the *success* path, where nobody
needs it.

Three more of the same family, all measured: a migration that failed half-way
named neither the migrations that had landed permanently nor the stack it left
running; a partial ledger write left a truncated `…toml.writing.<pid>` beside the
real ledger with nothing to remove it and a message naming the temp path rather
than the ledger; and two limits findings pointed at line 1 of `catalog/limits.toml`,
which is the file's header comment, while the migration-side findings in the same
run carried real numbers. **A diagnostic is a set of instructions, and every part
of it is either true where the reader stands or worse than silence.**

## The line that is one short

`str::lines()` does not count a trailing empty segment, so a byte offset landing
exactly on a line boundary answers one line early. In a policy case file that
meant a duplicate key was reported against **the original**:

```text
error policy/core/tests/chain_depth.toml:15 — the case file does not parse: duplicate key
   line 16 is the duplicate; line 15 is the one that was already right
```

The sibling reader in `nc-catalog-compiler` spells this exact hazard out in its
own doc — "for a duplicate, the *first* occurrence is the innocent one" — and
counts newlines. Two implementations of one arithmetic, and only one of them had
been taught.


## The measuring instrument, measured

`mutants.py` weakens one clause at a time and reports how many the suite stops.
Pointed at the file that answers "is the verdict the rule" —
`xtask/src/corpus.rs` — it reports what that file kills, and every kill it
reports there comes from exactly one line:

**The counts are not written here.** A number written here is wrong by the time
anyone reads it: `python3 xtask/mutants.py --list` answers how many there are
and the run's last line answers how many died. The survivors are *required* to
survive — `EQUIVALENT` at the foot of `mutants.py` holds them by name and fails
a run if the set that survived is not that set, in either direction — so
"every one comes back killed" is false by construction as well as by
arithmetic, and the comparison is between sets rather than between a pair.
Above, this same file says why: *a transcript pasted into a document is a
measurement of the day it was pasted*. `COUNTABLE` does not hold "clauses", so
`counts.rs` would not judge such a number — and it is not written here to rot.
`OPERATORS`
in `xtask/src/check.rs` names every weakening by its source, its predicate and
its label, and `the_meta_gate` holds `WEAKENINGS` against that list on every run
of the gate. A floor such as `operators.len() >= 70` is satisfied by any
seventy of them: the rest could be deleted with the gate green and the run
reporting fewer killed.

```rust
assert!(wrong.is_empty(), "…decided differently from what it is written to decide…");
```

Widen it to `wrong.len() < 100`, change nothing else, and:

```text
  ok    914 test(s)
  ok    advisories, licences, bans and sources

check passed
```

Most drivers rest on that function, and two more — `claim` and `caret_uses` —
carry a verbatim copy of the same assertion, so a weakening of it is invisible
in every corpus at once unless something hands the verdict a case that decides
wrongly.

`the_corpus_reports_a_case_that_decided_wrongly` does, to all three, in the shape
the gates themselves are held to. **A tally is worth what its assertion is worth,
and the assertion that produces every other test's verdict is the one nobody
thinks to test.**

## A function whose answer is thrown away is dead in a way `-D dead-code` cannot see

```rust
let _ = of_file(&findings, named);
```

One definition, one call, the answer discarded. `dead_code` sees a call and says
nothing; inverting the function's predicate — `==` to `!=` — changed no test
anywhere. What it was written for was the second half of a sentence the comment
above it already made: the finding is about *this* file, and the resolved pass
labels every finding it makes `docker-compose.yml`. It compares, and the
inversion fails.

## One rule, written four times, with two answers

Measured with the question "is this HTTP status line a 2xx" asked in four places
— twice in `xtask/src/store.rs`, once in `dev.rs`, once in `nc-log`'s shipper —
each under a comment saying it was "the same shape" as the others, three of them
were:

```rust
status.starts_with("HTTP/")
    && code.len() == 3
    && code.starts_with('2')
```

and only the shipper also required `code.chars().all(is_ascii_digit)`. So
`HTTP/1.1 2ab Invented` was a refusal to the crate that ships the platform's logs
and `ok the log store answers /health` to `cargo xtask dev`, on the same answer
from the same store.

The copies are one function per crate, and both read
`xtask/canaries/status-lines.tsv` — status lines with their verdicts, in a
file rather than in a sentence. A case added to it is answered by both suites.
`the_two_xx_rule_is_written_twice_and_no_more` walks the tree for a third copy,
assembling its needle at run time so the scanner is not one.

**A comment claiming two things are the same is not a mechanism.** Written above
four different pieces of code, that exact sentence has been measured false of each:

```text
members.rs   "the same invocation `members` makes, lifted out so the two readers
              cannot ask cargo two different questions"   — members kept its own copy
workflow.rs   the deleted-job loop, written twice and both copies running, so a
              deleted job produced two identical findings and `.any(…)` saw one
gates.rs      COMPOSE_FILES as a const, and the same eight names again as a
              literal in the pass that decides which files are read at all
store.rs      the 2xx rule, above
```

## Coverage and proof are different questions

`policy/core`'s denial-coverage gate asks whether a policy has a case.
`guards_fail_closed` needs a case whose `because` names *that* policy — and when
it found none it returned in silence, on the written argument that "reporting it
twice sends a reader to two places". Retype one `because` from
`core.grants_expire` to its sibling and coverage stays green, every guard in the
policy goes unproven, and the only trace is `inspected` getting smaller: a filter
between the subject and the counter, in a crate whose whole job is proving that
rules still refuse.

## A port you released is a port somebody else can take

```rust
let closed = TcpListener::bind("127.0.0.1:0").expect("a free port");
let port = closed.local_addr().expect("bound").port();
drop(closed);                       // <- the window
assert!(the_log_store_is_serving(&port).is_err());
```

A dozen other tests in the same binary bind ephemeral ports, and `cargo test`
runs them in parallel, so this failed for a reason that had nothing to do with
the log store — the flake this fleet has spent time on before. Port zero is the
wildcard: the kernel refuses a connection to it every time, with nothing to race
against.

## An attribute in front of a door

```rust
#[doc(hidden)]
pub type Text = Literal;

impl Text {
    pub fn made(text: &'static str) -> Self { Literal(text) }
}
```

Ordinary Rust, `cargo fmt`- and clippy-clean, in `crates/nc-log/src/value.rs`.
Every count went **up** and the type wall was byte-identical:

```text
ok    92443 source line(s) leave the workspace lints on      (up from 92432)
ok    0 crate(s) outside nc-log may log; 16 file(s) hold 16 logging call site(s)
check passed
```

Delete the one attribute and it is a finding. `is_public` strips the run of
attributes in front of a declaration — it exists for that, and its own doc
carries the measurement — and `no_other_public_door`, three lines below the call
to it, tested the **raw** text with `starts_with("pub type ")`. So the item was
public to one half of the rule and not a door to the other. The alias is what
makes it worth closing twice over: the constructor rule resolves an `impl` header
by name, and `Text` is not a sealed name, so nothing else in the wall sees the
`impl` underneath.

**Two halves of one rule read the same text, or they are two rules.**

## A test that passed because of the alphabet

```rust
// The counter is process-wide and other tests print, so what is
// asserted is the *difference* a `warn` makes, not an absolute.
…
let before = ui::warnings() - ui::skips();
assert_eq!(before, 1, "the announced skip was counted as an unexplained warning");
```

Three lines under its own comment. `release::run(…, dry_run)` prints three
non-skip warnings and two release tests drive it, so this held only because
libtest starts tests in name order and `check::` sorts before `release::`:

```text
$ xtask-<hash> --test-threads=2 --exact \
    release::tests::the_verb_refuses_a_release_that_does_not_move_forward \
    check::tests::the_verdict_counts_the_skips_it_announced
  left: 3
 right: 1
failures=200 of 200
```

The fix is not a lock every printing test has to remember to take — that is a
rule the next test breaks. `ok`, `failure`, `warn` and `detail` take a reentrant
hold themselves in test builds, and `every_printer_aligns_what_it_is_given`
requires each of them to.

## The census read characters and sliced bytes

`the_literal_at` walked `text.chars().collect()` and returned the index into
*that* vector; the caller added it to a byte offset and sliced. These files are
full of `§`, `—` and `…`, so the two units differ by hundreds of bytes over one
file. It is latent until a needle reaches further into the file:

```text
start byte index 165653 is not a char boundary; it is inside '§'
```

A census whose reader can restart inside the literal it just read is a census
that can report a fragment as a sentence. With that fixed and the needle widened
from `return Err(` to `Err(` — a tail-position `Err` is the same guard travelling
the same way — **seven verdicts appeared that had never been in the list**,
including the database-unreachable message and both log-store health messages.

## One rule, five doc comments, six orphans

A doc comment attaches to the next item. Insert an item without one and the
paragraph above it changes owner, silently: `missing_docs` is a lint about
*public* items, and almost nothing in `xtask` is public.

It has happened six times here, twice in the edit that closed it. `walk` — the
one walk every gate in this crate uses — lost its paragraph to a function
inserted above it, and `inspected_all` lost its `# Errors` section to a function
whose own doc links to it.

`UNDOCUMENTED` is the ratchet: the items that carry no doc today,
compared for **equality**, so it cannot grow without a reader seeing the line
and documenting one means deleting its entry in the same edit. Planted:

```text
fn a_probe_that_takes_the_doc_above_it() {}
pub(crate) fn walk(…)

the set of undocumented items moved …  ["gates.rs:walk", …]
```

## Whitespace is not a space

```text
CREATE OR REPLACE
FUNCTION planted() …
```

Both halves of the unbounded-function gate required a literal single space
between the keywords, and SQL does not. So the parser and the independent count
that floors it missed the **same** statement, and `found != creates` — the one
thing standing between a partial parse and a green run — was satisfied by their
agreeing about nothing. `CREATE  FUNCTION` with two spaces *was* caught, by the
mismatch rather than by either reader, which is what made this look tested.

Two counters over the same ground catch a filter in one of them. They cannot
catch a blind spot both share, and a shared blind spot is what a shared *reader*
gives them.

## Four documents, one workflow, and the word "only"

`xtask --help` — the surface a developer types — said:

```text
check    THE gate — CI runs this and nothing else
```

`.github/workflows/ci.yml` runs `cargo install --locked cargo-deny`,
`cargo xtask dev`, `cargo xtask check`, and `cargo xtask build` twice. The three
prose copies had each been corrected once and all three still omitted
`cargo xtask dev`, whose own step comment spends a paragraph on why it is
load-bearing: everything the gate asks a running database and a running log store
is there or is not checked at all. A reader acts on that sentence when deciding
whether a local `check` is what CI ran.

`WHAT_CI_RUNS` is the set, and `the_documents_agree_about_what_ci_runs` holds the
workflow and all five sentences against it in both directions — so a fifth step
added to CI fails here rather than quietly making four documents wrong.

## The floor that could only pass on the machine that wrote it

```text
ok    7 document(s) state no count that can rot, in 365 paragraph(s)
error the second counter read 365 paragraph(s), so either it stopped early or it
      is being asked about the wrong tree
error check failed: tests
```

No plant. That is `cargo xtask check` on a clone of this repository — which is
what the checkout step gives CI, and what `README.md` promises the gate passes
on. `how_many_paragraphs` reads every `AGENTS.md`, `README.md` and `CLAUDE.md` in
this repository that `counts.rs`'s `NOT_READ` does not exclude — nothing falls in
that hole today, and `docs/` is where one would — **and the five in the workspace
above**, so the number was 1643 for a developer and 365 for CI, and the floor
`> 500` was written in the first state and asserted in the second.

How many documents that is comes off the run's own `ok … document(s)` line and
is not written here: taking a file out of the `NOT_READ` hole changes that
number without touching this sentence.

Four sibling tests that need the workspace carry the guard; two of them are in
the same module. This one did not.

**A floor is calibrated against what the run in front of it can see.** The number
is "at least ten blocks per document opened" — true of every document in this
tree, false of a counter that stopped, and independent of which tree it is.

## The counter that was the first statement of the body

This document already says the counter is the last statement a verdict reaches.
It was the first statement in two loops, and one plain `continue` under either:

```text
unbounded tables   seen=3, no finding, 316 passed         criterion 5, off
nc-policy guards   32 constraint(s) … pinned, identical   §14.6, unasked
```

The second is the sharper one: `core.an_agent_never_impersonates` flipped to its
fail-open form is `guard-fails-open` and two red tests, and with the filter added
the policy step is byte-identical to a clean run. The rule is written for this
crate's *coverage* loop, and a rule written for one loop does not reach the
proof loop beside it.

**A rule stated in a document is not a rule applied.** What applies it is a case
that plants the filter.

## A manifest read as text

```toml
[package]
description = """
[lints]
workspace = true
"""
```

`every_member_takes_the_workspace_lints` asked a line scanner whether the file
held `[lints]` and `workspace = true`. It does — inside a string. Measured with
the real stanza deleted: `9 member(s) take the workspace lints`, and a public
undocumented function with an `unsafe` block compiling clean in that crate.
`unsafe_code = "forbid"` cannot be overridden at crate level, so the compile
succeeding *is* the proof the crate was outside the lints.

`is_a_build_directory` two hundred lines away already asked `toml`. The line
scanner is gone, because nothing else called it.

**A manifest is TOML.** Reading it as text is writing a second parser that
disagrees with cargo's, and the disagreement is silent.

## A bidi override on a published page

`printable`'s doc says it writes "every `///` line the compiler emits **and every
Markdown cell**". `doc()` was its only caller. One locale sentence carrying
`U+202E`, `U+202C` and a BEL byte:

```text
crates/nc-catalog/src/i18n.rs   "…data \u{202e}right now\u{202c}. Try\u{7} again…"
docs/errors.md                  the same bytes, raw
  ok    15 generated file(s) match the catalog
```

The generated *code* was escaped correctly and the generated *page* was not — on
the page that exists so a reader can check what a user is told.

## Two columns compared against the emitter that wrote them

Swapping the two arms of `Raising::written` published the inverted sentence for
all 29 error codes, green, under the page's own pinned paragraph saying *"the
column is the count"*. Every other word-mapping in that test file is written out
under a header explaining why; this one was not, and the reason was incidental —
`Strategy::written` and `Level::written` are `pub(crate)` so a test *cannot* call
them, while `Raising::written` is `pub`.

`docs/limits.md` was worse: nothing walked it page to registry at all. One
literal row appended to the emitter published a row-bounded table that
`catalog/limits.toml` does not declare and `migrations/` does not create.

## A repair that made a shared blind spot structural

Teaching both halves of the unbounded-function gate that a newline separates two
keywords was right. Putting the *same reader* in both was not:

```text
+ if rest.starts_with("access_") { continue; }   inside routine_at
  creates=2, found=2, mismatch=false, findings=0
```

`found != creates` is the one thing standing between a partial parse and a green
run, and it could no longer disagree about anything that reader decides. The
floor walks tokens — different code, same rule — and shares only the comment
stripper, deliberately: a shared normalisation of the input is not a shared
judgement of it.

**Two counters over the same ground catch a filter in one of them. They cannot
catch a blind spot both share, and a shared reader is how they get one.**

## `#[doc(hidden)]` is outside the gate that reads documentation

```rust
/// The journal is refused whatever the authority says — see
/// [`the_journal_is_never_prunable_by_row_count`].
#[doc(hidden)]
pub async fn prune_these_declared_by(…)
```

`grep` finds that name once: on that line. `RUSTDOCFLAGS=-D warnings` denies
`broken_intra_doc_links`, and rustdoc does not document a hidden item unless
asked — `--document-hidden-items` is behind `-Z unstable-options` and this
repository is pinned to stable. Measured: the same broken link is
`error: unresolved link` on a public function and exit 0 on a hidden one.

This workspace uses that attribute as its idiom for "a door a test must reach",
so the items whose prose explains *why they exist* were the ones outside the
gate. `no_hidden_item_carries_an_intra_doc_link` holds the class: say the name in
backticks, because a citation nothing can check is worse than none.

## An `# Errors` section that names the wrong refusal

| Site | Said | Returns |
|---|---|---|
| `Store::append` | `Unavailable`, `OutOfOrder`, `TooDeep` | every implementation calls `readable` first, so `Unstorable` is the commonest |
| `Store::from` | `Unavailable` | `Postgres::from`'s first statement returns `Unstorable` |
| `readable` | `TooDeep` | plus `Unstorable` for `hash` and for `sequence` |
| `Store::head` | "three places" | four |

The variants that go missing are the ones that invert `retryable`. A caller
written against the section retries a defect it caused, for ever — which is the
pair `NC-JRNL-0005` exists to keep apart.
`every_errors_section_names_what_its_body_builds` holds the half a scan can hold:
a variant a function builds in its own body is named in its own section.

## Two deliverables a clean pass does not settle

Each of `nc-ids` and `nc-prune` can pass a full audit clean while holding a defect
that would ship:

**A clock that dips and comes back.** One memo, two conclusions:

```text
first  = ClockMovedBackwards { drift: 1ms }   after 1002 read(s)  NC-IDS-0001, error
second = ClockStuck { at: … }                 after 1 read(s)     NC-IDS-0007, critical
```

Both exits from the bounded wait wrote `clock_stopped_at`, and the early return
that reads it raised `ClockStuck` whatever had been concluded. `NC-IDS-0007` says
*"the clock has stopped, so no more identifiers can be minted on this machine"*
about a clock that had moved. Every shipped fixture scripts a clock that stays
down, so the memo they exercise is only ever the stopped one — the fixture that
distinguishes the repair from the defect is one whose clock **returns**.

**A repair that closed a measured exploit, with no test.** `nc-prune`'s
schema-qualified-alias clause:

```rust
.any(|known| known.name == table.name || known.name == bare)
```

Delete `|| known.name == bare` and `public.access_records` — a legal name for the
table the catalog declares — is the caller's own to describe again: twenty of
twenty rows of the access log gone, ordered by a column the catalog documents as
*not* chronological, reported as an ordinary success, with the whole gate green
at the baseline test count.
`the_catalog_decides_the_whole_entry_and_not_one_field_of_it` covers the journal
under a qualified name and the other table under its bare one, and the cross of
the two is a case of its own.

**A repair is a claim, and a claim needs a case.** Both of these close a measured
defect, and a repair written without the test that would notice it being undone
is the shape an audit finds most.

## A gate that slices by bytes what it counted in chars

`without_string_contents` replaces each *char* inside a literal with one *byte*
of space — char-count-preserving, not byte-count-preserving, and `gates.rs` says
so about its own use. The counter-reader rule took `opens` and `brace` from
`text` and sliced `code` with them:

```text
check.rs   432312 bytes of code against 432045 of text
gates.rs   299802 against 299670
363 of 382 extracted bodies do not end at their function's closing brace; 13 are empty
```

Measured with one probe — a `#[test]` reading `warnings() - skips()` with no hold
— reported in `database.rs` and invisible in `gates.rs`. Every offset comes from
one string.

**Two strings that differ in length are two coordinate systems.** Blanking,
stripping and normalising all produce one; mixing offsets between them is a bug
that looks like a working scan.

## The claim a gate makes about itself

```sql
AND a.grantee <> 0
```

In `aclexplode`, grantee `0` is `PUBLIC`. That clause sat in the `writers=` count
on the journal, so the one grant shape that reaches every role — present and
future — was the one the gate filtered out:

```text
GRANT UPDATE, DELETE, TRUNCATE ON journal_entries TO PUBLIC;
  relacl = {noctua=arwdDxtm/noctua,=wdD/noctua}
  journal_entries writers=0
  ok    44 protection(s) on the journal and the access records …
```

`AGENTS.md` names `GRANT ALL … TO PUBLIC` as one of the shapes this reads, and
states the property as "no role other than the owner holds `UPDATE`, `DELETE` or
`TRUNCATE`". Both were false for exactly the shape the first one names in its own
parenthesis.

## `[workspace.lints]` has four tables

`cargo`, `rust`, `clippy` and `rustdoc`. The pin read two, so two lines in the root
manifest —

```toml
[workspace.lints.rustdoc]
broken_intra_doc_links = "allow"
```

— turned off the workspace's only reader of documentation with a real broken link
standing, and every printed count went *up*. A pin over a set enumerates the set.

## Thirty-eight thousand directories

A test calling `remove_dir_all` before writing its scratch tree and never after
fills `/tmp`: measured, 38,176 of them. Tests here remove at the end.

A test that cleans up on the way in is a test that leaves one behind every run,
and nothing counts them.

## Shapes an audit of every deliverable finds

Six sections, because each of these is a shape rather than an incident.

### A ratchet that reads column zero reads a tenth of the tree

`every_item_in_this_crate_is_documented`, matching `openers` against `*line`
**untrimmed**, sees only column-zero items: no method inside an `impl`, and no
test inside a `mod tests`, is visible to it. Walking `src/` alone, it sees no
integration suite either. What a ratchet of that shape lets past, measured:
`only_for`'s documentation — the `ONLY `/`INHERIT` argument, which is the reason
the inheritance exploit is closed — sitting on `primary_key_of` with `only_for`
carrying none; `nc-ids`' epoch memo on a test whose body asserts two widths are
equal; and one block in `the_registry_reaches_the_code.rs` holding the
documentation of **four** separate tests, each rendering with none.

Pointing it at indented items makes the list several times longer, which is
bookkeeping nobody reads. So the list stays a list of `src/`, and the second
ratchet is a **count**: doc blocks per file, over `src/` and `tests/`, compared for
equality. A merge turns two blocks into one, so the number falls and the file is
named — one line to update when a documented item is genuinely added, unmoved by
the 675 undocumented helpers a list would have been swamped by.

Both readers share one definition of "documented", because two definitions
disagree: a plain `//` line between a block and its item is not a detachment in
Rust, and a reader that treats it as one lists items as undocumented while
rustdoc renders their prose.

### A whitelist of files cannot hold a rule

`the_two_xx_rule_is_written_twice_and_no_more` has fourteen needles for the
spelling of a 2xx test and allows two files to hold one. A third classifier,
`code.as_bytes().first().copied() == Some(50)` — `50` is ASCII `2` — matches
none of them: measured, both scans reported the same two files, and
`cargo xtask dev` printed `the log store answers /health` for
`HTTP/1.1 2ab Invented`. The needle list is an arms race the scanner loses.

`the_health_check_answers_the_shared_table` stands a listener up for every row of
`status-lines.tsv` and requires `the_log_store_is_serving` to agree with the
table. Measured: the same third classifier fails on `HTTP/1.1 2 whatever`.

### The counter outside the loop, in criterion 2

A `compile_fail` returning `fixtures.len()`, computed from the *directory
listing* before anything is compiled, hands `inspected_all`, the printed line and
`the_wall_holds_and_the_proof_actually_ran` one number to read. Measured, three
lines in the loop body — `if name.contains("string") { continue; }` — turned the
whole compile-fail gate off with `11 fixture(s) refuse to compile`
byte-identical to a clean run, while `impl LogValue for String` shipped.

`corpus.rs`'s `ASKED` names the walks that carry a two-counter identity, and
this walk is one of them.

### A function body can be emptied, and the assertion over it is true

`verdict(&wrong, …)` is `assert!(wrong.is_empty())`. `image_cases()` and
`referent_cases()` are built by functions, which the corpus meta-gate does not
read as tables — so `referent_cases()` returning `Vec::new()`, with
`one_of_them_is` weakened to a substring test that lets PostgreSQL 16.14 run
under the tag `18-alpine`, gave `check passed`, measured. Deleting statements
deletes no `#[test]` attribute.

Each returns a fixed-size array, so emptying it is a compile error, and the
constant it is sized by is pinned with its value.

### Four doors, and the measurement watched one

`writers=` counts `pg_class.relacl`. A column-level `GRANT UPDATE (outcome,
masked) … TO PUBLIC` leaves `relacl` NULL and lands in `pg_attribute.attacl`;
`GRANT pg_write_all_data TO <role>` is a membership. Both were measured on a live
stack: a non-owner changed a row's `outcome` from `deny` to `allow`, and another
emptied the table, with `writers=0` and `check passed`. `access_records` has no
append-only trigger, so this is its only guard.

And the passing value is zero, so a clause nothing moves is watched by nothing —
an `AND a.grantee <> 0` hiding every grant to `PUBLIC` leaves every other test
green. A test grants each shape in a scratch schema of its own and requires the
number to move.

### A repair with no test

`--dry-run` promises to change nothing, and without a test of the write half of
`release`, deleting the `if dry_run { … } else { … }` arm left `11 passed` and
then wrote the manifest under the line saying nothing was written, measured. A
printer check that floors the *files* it reads and not the bodies every finding
it can make is read out of floors the wrong thing. And a CI determinism job
resting on one `cd -` rests on one line.

## A repair is where the next defect hides

Each shape below is a defect in a repair rather than in the code it repaired.

### The excuse and the danger are the same thing

The `Entry`-outside-the-hash defect is closed with an exhaustive
`let Self { … }` and a list — `NOT_HASHED_DIRECTLY` — of the fields the
destructure may discard. Adding a third name to that list is one token. The same
`pub decided_by`, threaded through `chain`, `forged_for_tests` and the store's
row reader and then excused, gave `check passed` with **the golden digest vectors
still matching**, which is itself the proof the field was outside the hash.

What makes a field dangerous is that the store persists it, so the store's
`INSERT` is the second reader. A column it names has to be under the digest,
and the excuse list cannot buy one.

### A door is watched by moving it

`writeall=` and `superusers=` are in the write-door census, and the row has a
positive control. A control that grants a column privilege and a table privilege
and asserts `writeall=0` *before* any grant, never granting that one, lets
`pg_write_all_data` spelled `pg_write_all_dat` — one character — leave the whole
gate byte-identical to baseline, measured.

And a census without `owners=` is a door short. `GRANT <owner> TO <an ordinary login>`
moves no number in that row, and an ordinary `UPDATE` as that role is still
refused by the trigger — so the row looks right and reads right. Three statements
later, with `ALTER TABLE … DISABLE TRIGGER USER` between them, the audit log is
rewritten and `check passed`. `owners=` counts it, and every door in the row is
granted and required to move.

### The control that connects under `--offline`

A control reading `url()`, which falls back to
`postgres://…@127.0.0.1:<this checkout's port>/noctua` and probes the port,
connects anyway under the environment `cargo xtask check --offline` hands the
test binary, and issues `CREATE SCHEMA`, two `GRANT`s and
`DROP SCHEMA … CASCADE` against whatever is listening, which by default is the
developer's own stack. `NC_NO_STACK` is the flag, and its sibling suites read it
too.

### Two tallies are not a pairing

Both operands of CI's determinism diffs are anchored, and a gate "counting
anchored operands against `diff` invocations" — `diff ` lines against
`$GITHUB_WORKSPACE` anywhere in the job body, the second at least the first —
lets a *comment* pay the budget while three of the four diffs compare the second
checkout with itself, with every workflow test green, measured.

### A repair with no test, measured by undoing it

`without_string_contents`'s char-literal clause (revert it: `327 passed`);
`release`'s write half, which `a_dry_run_writes_nothing` does not reach because
it passes `dry_run = true` (replace the write with an empty vector: `12 passed`,
`check passed`, and a real release then prints `1 file(s) carry 0.2.0` having
written nothing); `ci_supplies_what_the_gate_needs`, whose every finding was
unreachable because each scratch fixture in its module hits its early return
(replace the body with a return: all 30 suites green, and CI can then be
reordered to bring the stack up *after* the gate); and `not-a-permit`, the arm
that stops a `forbid` being credited with a denial test.

### The ratchet that pinned a defect

A doc-block count baselined by reading the tree as it stands pins whatever is
wrong in it: one of its numbers measured a file where a helper's documentation
sat on the function above it, so repairing the tree made the ratchet fail. And
the count is blind to half its subject: a merge takes two blocks to one and the
number falls, but an *insertion* conserves it. Each file pins two numbers —
blocks, and items carrying none.

