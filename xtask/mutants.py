#!/usr/bin/env python3
"""The meta-gate: does a suite notice a clause being weakened?

For each single-token weakening in `WEAKENINGS`: apply it to its source, run the
suite that decides that source, revert, and say whether the suite noticed. A
weakening nothing notices is a clause nothing decides — a hole in what this
script covers rather than in the tree, which is the direction the order of work
in this file exists to keep straight.

**This docstring states no inventory, for the reason the two comment blocks
below state none.** It used to: it said the run applies the corpus tests only,
that a survivor is a finding about `corpus.rs`, and that this is the fifth of
four mechanisms. Measured on 2026-09-15: the default filter is two suites, most
sources are judged by something else entirely, and the canaries file names eight
mechanisms. The clause about running only the corpus was the very one reported
forty lines below, in the block repaired the round before — repaired there and
not here, because a reviewer reads what they are pointed at.

What the run covers is computed and printed: `--summary` reads `SOURCES`,
`SUITES` and `ELSEWHERE` and says how many files, how many operators, and which
suite judges each. A sentence here can only disagree with it.

Usage:  python3 xtask/mutants.py            (from anywhere in the repository)
        python3 xtask/mutants.py --list     (print the weakenings and stop)
        python3 xtask/mutants.py --summary  (print what a run would cover)
"""

import os
import shutil
import signal
import socket
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# **More than one file, because the judgement is in more than one.** The
# unbounded-table verdict lives in `limits.rs` and the rest in `gates.rs`; a
# meta-gate that can only edit one file cannot weaken the predicate that decides
# acceptance criterion 5, and a predicate no operator can reach is a predicate
# this reports nothing about.
#
# **And what it does not reach is not written here, on purpose.** Inventories
# of it have been measured wrong in every clause: a written list of what was
# outside named covered files as uncovered — so a reader trusting it skips them
# while the real gaps go unstated — and a shortened list was false about the
# filters, the sources, the packages and the crates alike.
#
# **So the rule is that this paragraph states no inventory at all.** Nothing
# pins prose — no test in the tree reads this comment, and the operator gate in
# `check.rs` reads `WEAKENINGS` and never the header — and a list nothing can
# read is a list that rots at the speed the file changes. What a reader wants is
# `python3 xtask/mutants.py --summary`, which is computed from `SOURCES`,
# `SUITES` and `ELSEWHERE` and prints how many files, how many operators, and
# the suite each file is judged by. A sentence here can only disagree with it.
#
# Widening this is not a matter of adding a path. An operator is a *named*
# single-token weakening anchored to a line, and it is only meaningful if a
# `corpus::` case decides that clause — a new source with no case makes every
# operator on it SURVIVE, which reports a hole in this script rather than in the
# tree. So the order is: write the corpus case, then the operator, then add the
# file here. What must not happen is this list growing quietly while the number
# the run prints stays where it was — which is why that number is computed and
# not written down anywhere, here least of all.
SOURCES = {
    "gates": os.path.join(ROOT, "xtask/src/gates.rs"),
    "limits": os.path.join(ROOT, "xtask/src/limits.rs"),
    # **The file that judges the others.** A repair in `check.rs` uncovered by
    # mutation has to carry its own corpus row, and without an operator here
    # nothing measures whether it does. The rows exist —
    # `a_plain_count_is_told_from_arithmetic`,
    # `a_plain_expectation_is_told_from_a_pin`,
    # `no_floor_is_handed_a_count_that_cannot_be_zero`,
    # `everything_the_gate_can_say_is_written_down` — so the file is here in the
    # order this script prescribes: the corpus case first.
    "check": os.path.join(ROOT, "xtask/src/check.rs"),
    # **The hatch reader.** `references.rs` decides whether
    # a citation is excused, which is a gate switched off for one line — the
    # class `EXCUSED` counts and `AGENTS.md` invariant 15 is written about. Its
    # own suite decides these clauses; none of them is reachable from the
    # corpus, which is why the suite is named beside the file rather than
    # assumed.
    "references": os.path.join(ROOT, "xtask/src/references.rs"),
    # **The one no checkout ever runs.** `sections.rs` reads the
    # workspace documents above this repository, so in CI it announces a skip
    # and judges nothing — which makes it the gate most in need of a runner
    # locally, and the least likely to be noticed rotting. Its own suite decides
    # these clauses.
    "sections": os.path.join(ROOT, "xtask/src/sections.rs"),
    # **It judges the thing that judges everything.**
    # `workflow.rs` decides whether CI runs the gate at all and whether the gate,
    # once run, can fail — a `rustflags` entry in the cargo config was measured
    # turning eight real clippy violations into `ok lints`. A reader that passes
    # every workflow looks, from CI, exactly like one that works, so its own
    # suite is what decides its clauses.
    "workflow": os.path.join(ROOT, "xtask/src/workflow.rs"),
    # **Here in the order this script prescribes.**
    # A corpus case first — `the_absolute_path_predicate_decides_every_clause`
    # — then the operators over it, then the file here. Added the other way
    # round, every operator on a new source SURVIVES and reports a hole in
    # this script rather than one in the tree.
    "determinism": os.path.join(ROOT, "xtask/src/determinism.rs"),
    # **Where the rule about a reason actually lives.** The word-boundary rule
    # `is_the_marker` and the compiler's `pack_reason` share has one
    # implementation, here, so this is where a weakening of it has to be
    # applied for the corpus to see one.
    "compiler": os.path.join(ROOT, "crates/nc-catalog-compiler/src/lib.rs"),
    # **A judge in another crate.** A crate's `lib.rs` in this list says
    # nothing about its other files, and `ledger.rs` decides acceptance
    # criterion 7 — that an error code cannot be renumbered. The guard that a
    # ledger one line short is refused needs an operator to say the clause is
    # exercised at all.
    #
    # Its clauses are decided by that crate's own `ledger::` suite, which is why
    # `ELSEWHERE` exists: `-p xtask` cannot reach them.
    "ledger": os.path.join(ROOT, "crates/nc-catalog-compiler/src/ledger.rs"),
    # **Two predicates that decide whether a gate fires at all.** A repair is
    # where a plausible-but-wrong fix hides: `caret_uses` says whether a compile-fail fixture's
    # refusal came from the door it is named for, and `COUNTS_THEM` says whether
    # a numeral in a document is a count or a label.
    "wall": os.path.join(ROOT, "xtask/src/wall.rs"),
    "counts": os.path.join(ROOT, "xtask/src/counts.rs"),
    # **The forward-only migration rule.** Written inline in `migrate`, which no
    # test runs, and outside this list, the clause deciding whether a backdated
    # migration is refused is decided from neither direction: measured that way,
    # the condition replaced by `false` left `cargo test --workspace --locked` at
    # exit 0 with every target `ok`, while `cargo xtask dev` applied version 0
    # after version 12 and printed `ok 1 migration(s) applied`. So it is
    # `dev::is_backdated`, driven by
    # `corpus::the_forward_only_predicate_decides_every_clause` and weakened here.
    "dev": os.path.join(ROOT, "xtask/src/dev.rs"),
    # **Outside `xtask` and the compiler: the tree itself.** Every source above
    # judges *the tree*; these two are the tree. `nc-canon` writes the bytes a
    # journal entry is hashed over, so acceptance criterion 3 rests on the two
    # of them more directly than on anything else here — and defects of the
    # kind found in them by hand (`{:04x}` written `{:04X}`, and `hash`
    # normalising its domain) are invisible to the whole gate until a test holds
    # each, as `a_control_character_is_escaped_in_the_case_the_specification_writes`
    # holds the first; an operator here is what says that test still decides it.
    #
    # Their clauses are decided by that crate's own suite, which is why
    # `ELSEWHERE` carries them: `-p xtask` cannot reach one of them.
    "canon": os.path.join(ROOT, "crates/nc-canon/src/lib.rs"),
    "reader": os.path.join(ROOT, "crates/nc-canon/src/parse.rs"),
    # **Criteria 8 and 9 are theirs.** Read by hand, `nc-ids` is the best-held
    # crate in the tree — every bound written out and checked at both ends, the
    # Crockford alphabet pinned against an independently written constant, the
    # layout read one field at a time with its neighbours zeroed. That is
    # exactly the tree where a *runner* is worth having: what it measures is
    # whether the holding is still there after the next edit.
    "base32": os.path.join(ROOT, "crates/nc-ids/src/base32.rs"),
    "snowflake": os.path.join(ROOT, "crates/nc-ids/src/snowflake.rs"),
    # **Acceptance criterion 2 is theirs.**
    # `value.rs` is the wall — the newtypes that keep a run-time string out of a
    # log line — and `ship.rs` is what puts a line on the wire. The wall's
    # *types* are held by compile-fail fixtures and by `xtask/src/wall.rs`,
    # both in this list; what these operators touch is the code either side of
    # them, where this crate's own comments record four clauses each held by a
    # line in a test: the arm past what a number holds, the `snowflake` impl,
    # `Field::Id`'s rendering, and the distinction between a store that closed
    # and one that is merely silent. Each of those lines is something a run of
    # this script can be wrong about.
    #
    # Their clauses are decided by that crate's own suite, which is why
    # `ELSEWHERE` carries them: `-p xtask` cannot reach either.
    "value": os.path.join(ROOT, "crates/nc-log/src/value.rs"),
    "ship": os.path.join(ROOT, "crates/nc-log/src/ship.rs"),
    # **The ones that need a database.** Measured with no test holding the four
    # clauses named here, forty-three weakenings planted in `nc-journal`'s store
    # by hand: four survived a whole `cargo xtask check` — `from()`'s
    # `WHERE sequence >= $1` and its `ORDER BY sequence` each removed, `head()`
    # ordered by the caller's column, and the arm that says *nothing is wrong*
    # answering the error its own comment records as the defect it guards
    # against. Three of those are a one-line edit inside a SQL string, and each
    # of the four is an operator here.
    #
    # Their suites need the stack, so a run of this script needs
    # `cargo xtask dev` up. That is stated rather than worked around — the
    # alternative is a runner that reports `killed` for the half of criterion 3
    # and criterion 5 that a database decides.
    #
    # **The migrations themselves stay out, and the reason is measurable.**
    # `sqlx`'s migrator refuses an applied migration whose text changed, so every
    # operator over a `.sql` file costs a `dev --down` and a full rebuild of the
    # stack — minutes each, against seconds for everything above. What holds them
    # instead is `limits::every_condition_a_migration_tests_is_the_one_written_down`,
    # which pins the text of every `IF … THEN` in every migration, and the
    # `database.rs` census over each trigger's `tgenabled` and `md5(prosrc)`.
    # **The crate root of acceptance criterion 3.**
    # The crate root holding `verify`, `chained_and_contiguous`, `readable`,
    # `readable_record`, `Journal::append` and both `code()` maps — the whole of
    # what criterion 3 rests on. Swept by hand with 38 operators, its suite
    # killed 37; the three below are the clauses a reader would reach for first.
    "journal": os.path.join(ROOT, "crates/nc-journal/src/lib.rs"),
    "journal_store": os.path.join(ROOT, "crates/nc-journal/src/postgres.rs"),
    "prune_store": os.path.join(ROOT, "crates/nc-prune/src/postgres.rs"),
    # **Acceptance criterion 11's step.** Measured with no test driving the four
    # comparisons below, twelve one-line weakenings of it, each alone, each
    # against the whole `-p xtask` suite: **nine survived**. Three of the twelve
    # died and only one of those died in a test that exercises code — the
    # other two were caught by
    # `every_verdict_is_pinned_to_where_it_is_said`, which pins the *source text*
    # of the arm a sentence sits in and noticed the text rather than the
    # behaviour.
    #
    # These are the four comparisons the step performs — the resource
    # comparison, the written case count its own doc calls "the one artefact of
    # this step no written number held", the per-policy pinned table, and both
    # directions of the schema against the reference that outranks it — and
    # `policy::tests` is what decides them, which is why `SUITES` runs it for
    # this source.
    "policy": os.path.join(ROOT, "xtask/src/policy.rs"),
    # **Both crates' roots, beside their `postgres.rs` and their siblings.**
    # `nc-ids`'s `lib.rs` is where `OpaqueId` lives — the type the realm column
    # is, which `Journal::append` scopes every write by. `nc-prune`'s `lib.rs`
    # holds `refusal()` and `Pruned::code()`, the writer and the reader of the
    # sentence a scheduler reads a failure out of; measured with a test that
    # asked only that the code appear somewhere in the sentence, a prefix as small
    # as `prune: ` left them disagreeing with the whole workspace green — which is
    # why `the_code_reaches_whoever_reads_the_result` reads it off the front.
    #
    # A crate root left out while its siblings are in is left out on no
    # argument. Measured, ten one-line weakenings: nine died and one survived,
    # and the survivor was a test comparing a reader against a writer that both
    # go through the mutated function.
    "ids": os.path.join(ROOT, "crates/nc-ids/src/lib.rs"),
    "pruner": os.path.join(ROOT, "crates/nc-prune/src/lib.rs"),
    # **Every gate a live database decides, and what criterion 11 rests on.**
    # `xtask/src/database.rs` holds the applied-migration record and its
    # checksum, the forty-four protections, the migrated functions' paths, the
    # collation — and a statement written there twice, with only the copy that
    # has a test right, is the shape an operator catches.
    # `crates/nc-policy/src/lib.rs` holds `coverage`, `run`, `widenings` and the
    # guard predicates acceptance criterion 11 rests on, beside the *step* that
    # calls them.
    #
    # Measured, five one-line weakenings: all five died.
    "database": os.path.join(ROOT, "xtask/src/database.rs"),
    "pack": os.path.join(ROOT, "crates/nc-policy/src/lib.rs"),
    # **And the file that says *where* a problem is.** `line_of` decides the
    # line number every pack refusal carries, and a comment quoting a policy is
    # not where that policy is declared. A repair in a file no operator reaches
    # is a repair nothing measures — the same reason `check.rs` is here.
    "problem": os.path.join(ROOT, "crates/nc-policy/src/problem.rs"),
    # **And the log store's reader.** Its clauses are not "held by fixtures,
    # not by mutation": measured without
    # `a_flags_answer_is_read_only_when_it_was_served_and_only_its_body`, three of
    # them were held by nothing at all — the status check, the header boundary,
    # and the accessor a floor reads.
    "store": os.path.join(ROOT, "xtask/src/store.rs"),
    # **And the five lines that hand each verb its flags**, which
    # `check.rs`'s `each_verb_is_handed_what_its_flags_say` pins as text.
    # Measured without that pin:
    # discarding `--reserve` there left `check passed` and a byte-identical
    # transcript, and `--reserve` is the one irreversible write: a code it
    # reserves is permanent.
    "main": os.path.join(ROOT, "xtask/src/main.rs"),
    # **The generator's own writer, whose live clauses need an operator.**
    # `build.rs` performs this tree's only irreversible write — reserving an
    # error code — and, without `a_run_writes_the_files_that_are_not_already_on_disk`,
    # inverting one condition in its write loop made `cargo xtask build` write
    # nothing at all with a full `check` green, measured. A file that writes
    # permanently is the last one to leave uncovered.
    "generator": os.path.join(ROOT, "xtask/src/build.rs"),
    # **What the workspace is, which every floor in this tree is a set against.**
    # Outside this runner, whether five of its clauses are decided can be found
    # out only by planting them by hand.
    "members": os.path.join(ROOT, "xtask/src/members.rs"),
    # **And the crate sources with clauses no other operator reaches.** Each
    # carries its own suite: the compiler's message rules and its model, the
    # policy pack's cases and its value translation, and the log's events and
    # crate root.
    "compiler_problem": os.path.join(ROOT, "crates/nc-catalog-compiler/src/problem.rs"),
    "compiler_messages": os.path.join(ROOT, "crates/nc-catalog-compiler/src/messages.rs"),
    "pack_cases": os.path.join(ROOT, "crates/nc-policy/src/cases.rs"),
    "pack_value": os.path.join(ROOT, "crates/nc-policy/src/value.rs"),
    # **The log's crate root, which is more than the type wall.** "The type
    # wall, enforced by rustc" is no reason to leave it out: three of its
    # clauses are ordinary runtime rules its own crate's suite decides — the
    # reserved field name, the origin-ordering loop, and the shipper's lost
    # counter, measured.
    "log_root": os.path.join(ROOT, "crates/nc-log/src/lib.rs"),
    # **And the compiler's model, which is not "declarations only".**
    # It holds eight `const fn` predicates and their spellings are *published*:
    # `Raising::written` is a column of `docs/errors.md`. So what decides a
    # weakening here is the gate that rebuilds the pages and compares them with
    # disk — `a_second_generation_that_differs_is_a_failure`, in the xtask
    # binary's own tests.
    #
    # **Measured, because the obvious decider is not one.** With
    # `Self::Reserved`'s sentence rewritten to `"yes"`:
    # `cargo test -q -p xtask --bin xtask` fails on that gate, and
    # `cargo test -q -p xtask --test the_pages_reach_the_catalog` returns
    # `ok. 17 passed` — every word-mapping in that target is written out a second
    # time by hand and compared against the same page nobody rebuilt, so both
    # sides of its comparison are untouched by an edit to the emitter. So the
    # command below is not the wider `-p xtask`, which would spend a whole
    # integration suite per operator to run a test that cannot fail.
    "compiler_model": os.path.join(ROOT, "crates/nc-catalog-compiler/src/model.rs"),
    # **The other two files of `nc-ids`, beside `base32.rs` and `snowflake.rs`**
    # — and acceptance criterion 8 is theirs: `naming.rs` is Denote's scheme
    # transcribed, and `flake.rs` is the ten bits that say which machine minted
    # an identifier.
    #
    # Measured without a test for either clause below, thirty-eight one-line
    # weakenings, each alone, against that crate's own suite: **thirty-six died
    # and two survived.** One was `with_label` ignoring the label its caller
    # handed in — with every test passing the same two words, the function could
    # interpolate the placeholder's own name and the sentence still rendered
    # with no brace left in it and its bounds in the right order. The other was a title
    # shortened to nothing kept as `Some("")` rather than dropped, which writes a
    # `--` into the name with nothing behind it and breaks the law `§16.3`
    # calls this module's highest-value test. Each has a test, and each is an
    # operator here.
    #
    # Their clauses are decided by that crate's own suite, which is why
    # `ELSEWHERE` carries them: `-p xtask` cannot reach either.
    "naming": os.path.join(ROOT, "crates/nc-ids/src/naming.rs"),
    "flake": os.path.join(ROOT, "crates/nc-ids/src/flake.rs"),
    # **Four files whose rules no other operator drives.** `release.rs` is the verb nobody runs,
    # so its rules are the least driven in the tree; `ui.rs` holds the one-word
    # rule acceptance criterion 1 rests on — what counts as an announced skip;
    # `docs.rs` decides whether a page is rewritten; and
    # `the_registry_reaches_the_code.rs` is where each generated limit is held
    # against the file it came from.
    "release": os.path.join(ROOT, "xtask/src/release.rs"),
    # **The history step**, whose rules decide what every commit and every pull
    # request may say, and which no operator elsewhere reaches.
    "history": os.path.join(ROOT, "xtask/src/history.rs"),
    "ui": os.path.join(ROOT, "xtask/src/ui.rs"),
    "docs": os.path.join(ROOT, "xtask/src/docs.rs"),
    "registry": os.path.join(ROOT, "xtask/tests/the_registry_reaches_the_code.rs"),
    # **Files where a sweep by hand has found defects.** The struct reader's
    # raw-literal branch, a command's resource selector, a pack-bounded table's
    # second number, and a locale file whose extension is written in capitals
    # each held one, found by a person running mutations one at a time — which
    # is the work this script exists to stop repeating.
    "entry": os.path.join(ROOT, "crates/nc-journal/src/entry.rs"),
    "memory": os.path.join(ROOT, "crates/nc-journal/src/memory.rs"),
    "unit": os.path.join(ROOT, "crates/nc-journal/src/unit.rs"),
    "loader": os.path.join(ROOT, "crates/nc-catalog-compiler/src/load.rs"),
    "emitter": os.path.join(ROOT, "crates/nc-catalog-compiler/src/emit.rs"),
    "pages": os.path.join(ROOT, "crates/nc-catalog-compiler/src/docs.rs"),
    # **The rule a command's declaration answers to, in a crate of its own.**
    # `nc-schema` is what the catalog compiler judges every shipped command
    # with, and what `NOCTUA.md §11.7` has the kernel judge every plugin's with
    # at `activate` — so a clause weakened here is weakened for both callers at
    # once, and neither caller's suite is the one that decides it. The crate's
    # own suite is: the refusals one test each, the selector's grammar and the
    # judge's order.
    "schema": os.path.join(ROOT, "crates/nc-schema/src/lib.rs"),
    "selector": os.path.join(ROOT, "crates/nc-schema/src/selector.rs"),
    "descriptor": os.path.join(ROOT, "crates/nc-schema/src/descriptor.rs"),
    # **What a pack's version is, and what a realm pins against it.** The
    # manifest gate decides whether a realm running a pack it was not reviewed
    # against is refused, and a decision point reads the same `Manifest` — so a
    # clause weakened here is weakened for the gate and the kernel at once.
    # `nc-policy`'s own suite decides it, from both ends of each rule.
    "pack_manifest": os.path.join(ROOT, "crates/nc-policy/src/manifest.rs"),
    # **The decision point the kernel embeds.** Each clause here is one of the
    # ways it denies rather than allows — an evaluation error, a request the
    # schema refuses, an entity the directory does not hold, a realm pinning a
    # pack this build does not carry — so a weakening that survives is a way a
    # dispatch is allowed that should not be. `nc-authz`'s own suite decides it,
    # against the embedded pack and the world every policy case runs in.
    "authz": os.path.join(ROOT, "crates/nc-authz/src/lib.rs"),
    "authz_directory": os.path.join(ROOT, "crates/nc-authz/src/directory.rs"),
    "authz_embedded": os.path.join(ROOT, "crates/nc-authz/src/embedded.rs"),
    # **The kernel's ground, and the plugin it is shown with.** The world and its
    # pinned digest, the bindings' link to the host, the engine's two counters,
    # the reader that refuses a stale component, and the two doors a plugin is
    # run through — each decided by `nc-kernel`'s own suite, which loads the
    # component `cargo xtask build --plugins` wrote.
    "kernel": os.path.join(ROOT, "crates/nc-kernel/src/lib.rs"),
    "kernel_world": os.path.join(ROOT, "crates/nc-kernel/src/world.rs"),
    "kernel_bindings": os.path.join(ROOT, "crates/nc-kernel/src/bindings.rs"),
    "kernel_engine": os.path.join(ROOT, "crates/nc-kernel/src/engine.rs"),
    "kernel_built": os.path.join(ROOT, "crates/nc-kernel/src/built.rs"),
    # **What a plugin may call, and every host function that asks.** The
    # grants a manifest names, the bounds it may only lower, and the one door
    # each function of the world enters through — decided by `nc-kernel`'s unit
    # tests beside each rule and by `crates/nc-kernel/tests/capabilities.rs`, which runs the
    # reference plugin's probe through every function of the world.
    "kernel_capability": os.path.join(ROOT, "crates/nc-kernel/src/capability.rs"),
    "kernel_limits": os.path.join(ROOT, "crates/nc-kernel/src/limits.rs"),
    "kernel_manifest": os.path.join(ROOT, "crates/nc-kernel/src/manifest.rs"),
    "kernel_host": os.path.join(ROOT, "crates/nc-kernel/src/host.rs"),
    "kernel_host_log": os.path.join(ROOT, "crates/nc-kernel/src/host/log.rs"),
    "kernel_host_context": os.path.join(ROOT, "crates/nc-kernel/src/host/context.rs"),
    "kernel_host_store": os.path.join(ROOT, "crates/nc-kernel/src/host/store.rs"),
    "kernel_host_blobs": os.path.join(ROOT, "crates/nc-kernel/src/host/blobs.rs"),
    "kernel_host_commands": os.path.join(ROOT, "crates/nc-kernel/src/host/commands.rs"),
    "kernel_host_events": os.path.join(ROOT, "crates/nc-kernel/src/host/events.rs"),
    "kernel_host_ui": os.path.join(ROOT, "crates/nc-kernel/src/host/ui.rs"),
    "kernel_host_net": os.path.join(ROOT, "crates/nc-kernel/src/host/net.rs"),
    "kernel_host_timers": os.path.join(ROOT, "crates/nc-kernel/src/host/timers.rs"),
    # **Where a plugin's data is kept, and what its commands are held to.**
    # The store held in memory is decided by
    # `crates/nc-kernel/tests/both_stores_answer_alike.rs`, which holds it to
    # the local database answer for answer; the registry by its own unit tests
    # and by `crates/nc-kernel/tests/activation.rs`, which loads the reference
    # plugin as every way its activation can be refused.
    "kernel_backend": os.path.join(ROOT, "crates/nc-kernel/src/backend.rs"),
    # The blobs' bytes, kept apart from the store that says which unit made
    # each, are decided by the unit tests beside each rule and by
    # `crates/nc-kernel/tests/where_a_blob_is.rs`, which runs the reference
    # plugin over a blob directory on disk.
    "kernel_blobs": os.path.join(ROOT, "crates/nc-kernel/src/blobs.rs"),
    "kernel_registry": os.path.join(ROOT, "crates/nc-kernel/src/registry.rs"),
    # **The decision point, the units of work and the log**, judged by the
    # kernel's suites: `crates/nc-kernel/tests/the_decision_point.rs` dispatches
    # through every step of the door, journal and store included, and each file
    # holds unit tests of what that suite cannot reach.
    "kernel_dispatch": os.path.join(ROOT, "crates/nc-kernel/src/dispatch.rs"),
    "kernel_units": os.path.join(ROOT, "crates/nc-kernel/src/units.rs"),
    "kernel_logging": os.path.join(ROOT, "crates/nc-kernel/src/logging.rs"),
    # **What becomes of a plugin that is stopped**: the cause each kill is told
    # apart by, the budget it is restarted within, the release, the host call's
    # deadline and the epoch counted as the plugin's own running. Judged by the
    # unit tests beside each rule and by `crates/nc-kernel/tests/isolation.rs`,
    # which kills the reference plugin each way beside its peer.
    "kernel_supervisor": os.path.join(ROOT, "crates/nc-kernel/src/supervisor.rs"),
    # **What a plugin may ask of the network, and what sends it.** The request
    # policy is decided clause by clause by the unit tests beside it and by
    # `crates/nc-kernel/tests/net.rs`, which sends the reference plugin's
    # requests through the decision point to a transport that keeps them; the
    # transport by its own tests against sockets on this machine, a child
    # process given the environment's proxy among them.
    "kernel_net": os.path.join(ROOT, "crates/nc-kernel/src/net.rs"),
    "kernel_net_ureq": os.path.join(ROOT, "crates/nc-kernel/src/net/ureq.rs"),
    # **A person's own history.** Undo and redo — which step, the barrier, the
    # journal's word, the actions authorized again, the keys held to what was
    # left, the pins, the compensations — are decided by the unit tests beside
    # them and by `crates/nc-kernel/tests/units_and_history.rs`, which steps the
    # reference plugin's units back and forward through the decision point.
    "kernel_history": os.path.join(ROOT, "crates/nc-kernel/src/history.rs"),
    # **The kernel's own commands**: the handler of each command the catalog
    # gives the kernel, the chain verified and the local tables pruned. Decided
    # by the unit tests beside them and by
    # `crates/nc-kernel/tests/the_decision_point.rs`, which dispatches each as
    # a guardian and as people the pack does not let.
    "kernel_builtin": os.path.join(ROOT, "crates/nc-kernel/src/builtin.rs"),
    # **The rest of the world, and what runs it between call trees.** The
    # topics and the event queue's bounds, the timers' bounds and their table,
    # the closed set of nodes a surface shows and the pump that fires and
    # delivers — each decided by the unit tests beside it and by
    # `crates/nc-kernel/tests/the_rest_of_the_world.rs`, which publishes, sets,
    # shows and pumps the reference plugin beside its peer.
    "kernel_events": os.path.join(ROOT, "crates/nc-kernel/src/events.rs"),
    "kernel_timers": os.path.join(ROOT, "crates/nc-kernel/src/timers.rs"),
    "kernel_ui": os.path.join(ROOT, "crates/nc-kernel/src/ui.rs"),
    "kernel_pump": os.path.join(ROOT, "crates/nc-kernel/src/pump.rs"),
    # **What becomes the component, which no host test compiles.** The
    # plugin's root and its adapter are built for `wasm32` only by `cargo
    # xtask build --plugins`, so that is the first command and the kernel's
    # suite, which loads what it wrote, is the second. The plugin's behaviour
    # is plain Rust and is decided by its own host tests.
    #
    # **The guest bindings are not here, and `NOT_MUTATED` in `check.rs` says
    # why.** `crates/nc-guest/src/lib.rs` is one `generate!` call, and every
    # argument of it is decided when a plugin is compiled against it. An
    # operator turning `pub_export_macro` off makes the plugin's `export!`
    # private: the build fails and this run says `does not compile`, which is
    # right, because a weakening that does not build decides nothing. No
    # weakening of that call both builds and changes what the kernel loads.
    "reference": os.path.join(ROOT, "plugins/reference/src/lib.rs"),
    "reference_behaviour": os.path.join(ROOT, "plugins/reference/src/behaviour.rs"),
    "reference_adapter": os.path.join(ROOT, "plugins/reference/src/adapter.rs"),
    "plugins": os.path.join(ROOT, "xtask/src/plugins.rs"),
    # **The local database, and every refusal it makes before the kernel
    # trusts it**: the directory and the file private to their user, the
    # schema held to its list, a unit written whole or not at all. Decided by
    # that crate's own suites, against real directories, a real file, a model
    # and a child process killed mid-commit.
    "local_lib": os.path.join(ROOT, "crates/nc-local/src/lib.rs"),
    "local_open": os.path.join(ROOT, "crates/nc-local/src/open.rs"),
    # **What makes those files private, on the system this runs on**: the
    # owner, the mode and the link judged, and the directory and the file
    # created, in `nc-platform`'s Unix half and the probe beside it. Decided by
    # that crate's own tests and by the local database's suites, which open
    # real directories with each refusal staged; the test doors are decided by
    # the suites that stage through them. The Windows half is not here:
    # `NOT_MUTATED` in `check.rs` says why.
    "platform": os.path.join(ROOT, "crates/nc-platform/src/lib.rs"),
    "platform_unix": os.path.join(ROOT, "crates/nc-platform/src/unix.rs"),
    "platform_testing": os.path.join(ROOT, "crates/nc-platform/src/testing.rs"),
    "local_schema": os.path.join(ROOT, "crates/nc-local/src/schema.rs"),
    "local_store": os.path.join(ROOT, "crates/nc-local/src/store.rs"),
    # **The journal's store on the person's machine**: the link and a unit's
    # brackets inside one insert, the count every append is believed for, a
    # row read back only as this code writes it, and which units are open
    # asked of four columns. Decided by the crate's own suites, the local
    # journal's held line for line to the store in memory.
    "local_journal": os.path.join(ROOT, "crates/nc-local/src/journal.rs"),
    # The pruner's second target, over the same file: decided by `nc-prune`'s
    # suite against a scratch SQLite file, which needs no service.
    "prune_sqlite": os.path.join(ROOT, "crates/nc-prune/src/sqlite.rs"),
    # The gates over the local database — its schema built from the files on
    # disk, and the SQL its crates may write — decided by their own module.
    "local_gates": os.path.join(ROOT, "xtask/src/local.rs"),
    # **Every name an install gives the system**, and the manifest an install
    # names itself by: the frozen digest and its rendering, the port block, the
    # leaf, the user's spelling and every refusal of a manifest. Decided by
    # that crate's own tests, which derive the names of every channel and touch
    # no disk but to canonicalize a path.
    "channel": os.path.join(ROOT, "crates/nc-channel/src/lib.rs"),
}

# (source, predicate, label, from, to). `from` must appear exactly once in it.
WEAKENINGS = [
    # -- base32: the text a name is written in ------------------------------
    ("base32", "decode", "an empty text is not a value",
     "    if length == 0 {\n        return Err(Base32Error::Empty);\n    }",
     "    if false {\n        return Err(Base32Error::Empty);\n    }"),
    ("base32", "decode", "the width bound is a bound",
     "    if length > MAX_WIDTH {",
     "    if length > MAX_WIDTH + 1 {"),
    ("base32", "decode", "`O` is read as the digit it resembles",
     "            'O' => 0,\n            'I' | 'L' => 1,",
     "            'I' | 'L' => 1,"),
    ("base32", "decode", "`L` is a confusable as well as `I`",
     "            'I' | 'L' => 1,",
     "            'I' => 1,"),
    ("base32", "decode", "the accumulation is checked",
     "            .checked_mul(32)",
     "            .checked_mul(1)"),
    ("base32", "encode", "a value is padded to the minimum width",
     "    while digits.len() < MIN_WIDTH {",
     "    while digits.len() < MIN_WIDTH - 1 {"),

    # -- snowflake: the layout criterion 9 rests on -------------------------
    ("snowflake", "assemble", "the timestamp is masked into its field",
     "            ((millis & TIMESTAMP_MASK) << (FLAKE_BITS + SEQUENCE_BITS))",
     "            ((millis) << (FLAKE_BITS + SEQUENCE_BITS))"),
    ("snowflake", "assemble", "the node is masked into its field",
     "                | ((flake as u64 & FLAKE_MASK) << SEQUENCE_BITS)",
     "                | ((flake as u64) << SEQUENCE_BITS)"),
    ("snowflake", "assemble", "the sequence is masked into its field",
     "                | (sequence as u64 & SEQUENCE_MASK),",
     "                | (sequence as u64),"),
    ("snowflake", "from_crockford", "a value past the layout is refused",
     "        if value >> (TIMESTAMP_BITS + FLAKE_BITS + SEQUENCE_BITS) != 0 {",
     "        if value >> (TIMESTAMP_BITS + FLAKE_BITS + SEQUENCE_BITS) > 1 {"),
    ("snowflake", "resuming_at", "the drift bound is a bound",
     "            if millis - now > PLAUSIBLE_DRIFT_MS {",
     "            if millis - now > PLAUSIBLE_DRIFT_MS + 1 {"),
    ("snowflake", "wait_past_with", "a wait on a stopped clock spends its yields",
     "        for _ in 0..YIELDS_PER_WAIT {\n",
     "        for _ in 0..YIELDS_PER_WAIT / 10_000 {\n"),
    # -- flake: the ten bits that say which machine minted an identifier --
    ("flake", "from_value", "the range bound is a bound",
     "        if value >= RANGE {",
     "        if value > RANGE {"),
    ("flake", "candidate", "the domain is length-prefixed",
     "        hasher.update(&(DOMAIN.len() as u64).to_le_bytes());\n",
     ""),
    ("flake", "candidate", "the hash is domain-separated",
     "        hasher.update(DOMAIN.as_bytes());\n",
     ""),
    ("flake", "candidate", "the device key is length-prefixed",
     "        hasher.update(&(device_key.len() as u64).to_le_bytes());\n",
     ""),
    ("flake", "candidate", "the attempt salts the hash",
     "        hasher.update(&attempt.to_le_bytes());\n",
     ""),
    ("flake", "candidate", "the digest is reduced into the layout",
     "        Self(raw % RANGE)",
     "        Self(raw % (RANGE + 1))"),
    ("flake", "candidate", "the digest is read from two whole bytes",
     "        let raw = u16::from_le_bytes([bytes[0], bytes[1]]);",
     "        let raw = u16::from_le_bytes([bytes[0], bytes[0]]);"),
    ("flake", "candidates", "the sequence is the whole budget",
     "        (0..CANDIDATES).map(move |attempt| Self::candidate(device_key, attempt))",
     "        (0..CANDIDATES - 1).map(move |attempt| Self::candidate(device_key, attempt))"),
    ("flake", "bounds", "the low bound is the low bound",
     "        [(\"min\", \"0\"), (\"max\", HIGHEST)]",
     "        [(\"min\", \"1\"), (\"max\", HIGHEST)]"),
    ("flake", "with_label", "the label is the caller's, not the placeholder's name",
     "        [(\"field\", field), (\"min\", \"0\"), (\"max\", HIGHEST)]",
     "        [(\"field\", \"field\"), (\"min\", \"0\"), (\"max\", HIGHEST)]"),
    ("flake", "HIGHEST", "the spelled maximum is the layout's",
     "const HIGHEST: &str = \"1023\";",
     "const HIGHEST: &str = \"1024\";"),
    ("flake", "code", "the code is the one the catalog declares for a bound",
     "    pub const fn code(self) -> nc_catalog::ErrorCode {\n        nc_catalog::ErrorCode::ValidationOutOfRange\n    }",
     "    pub const fn code(self) -> nc_catalog::ErrorCode {\n        nc_catalog::ErrorCode::IdsNotAnIdentifier\n    }"),

    # -- naming: Denote's scheme, which criterion 8 is about -------------
    ("naming", "MAX_BYTES", "the byte limit is the number its paragraph argues for",
     "pub const MAX_BYTES: usize = 255;",
     "pub const MAX_BYTES: usize = 256;"),
    ("naming", "DELIMITER", "the title's delimiter is two bytes wide",
     "const DELIMITER: usize = 2;",
     "const DELIMITER: usize = 1;"),
    ("naming", "build", "the refusal is at the byte after the last that fits",
     "        if full.len() <= MAX_BYTES {",
     "        if full.len() <= MAX_BYTES + 1 {"),
    ("naming", "shorten", "shortening begins at the byte after the last that fits",
     "        if self.render(self.title.as_deref()).len() <= MAX_BYTES {",
     "        if self.render(self.title.as_deref()).len() <= MAX_BYTES + 1 {"),
    ("naming", "shorten", "the delimiter is counted with the title",
     "        let Some(room) = MAX_BYTES.checked_sub(without.len() + DELIMITER) else {",
     "        let Some(room) = MAX_BYTES.checked_sub(without.len()) else {"),
    ("naming", "shorten", "a name that fits without its title drops the title",
     "            if without.len() <= MAX_BYTES {",
     "            if false {"),
    ("naming", "shorten", "a truncated title loses its trailing hyphen",
     "        let shortened = title[..end].trim_end_matches('-').to_owned();",
     "        let shortened = title[..end].to_owned();"),
    ("naming", "shorten", "a title cut to nothing is no title",
     "        self.title = if shortened.is_empty() {",
     "        self.title = if false {"),
    ("naming", "terminators", "the title's terminators are its own list",
     "    if marker.as_bytes()[0] == b'-' {",
     "    if false {"),
    ("naming", "field", "a marker followed by a dot yields nothing there",
     "        if rest.starts_with('.') || rest.is_empty() {",
     "        if rest.is_empty() {"),
    ("naming", "field", "a marker at the end of the name yields nothing",
     "        if rest.starts_with('.') || rest.is_empty() {",
     "        if rest.starts_with('.') {"),
    ("naming", "field", "a dot ends a component",
     "            if character == '.' || ends.iter().any(|end| remainder.starts_with(end)) {",
     "            if ends.iter().any(|end| remainder.starts_with(end)) {"),
    ("naming", "field", "the content is a non-empty run",
     "        for (offset, character) in rest.char_indices().skip(1) {",
     "        for (offset, character) in rest.char_indices() {"),
    ("naming", "unusable", "the C0 range ends at its last character",
     "    (character as u32) <= 0x1f",
     "    (character as u32) < 0x1f"),
    ("naming", "slug", "what the component removes is removed",
     "            || also_removed.contains(&character)\n",
     ""),
    ("naming", "slug", "what a filesystem forbids is removed",
     "            || FORBIDDEN.contains(&character)\n",
     ""),
    ("naming", "slug", "Denote's punctuation is removed",
     "            || REMOVED.contains(&character)\n",
     ""),
    ("naming", "slug", "an illegal character is removed rather than kept",
     "        if unusable(character)\n",
     "        if false\n"),
    ("naming", "slug", "an underscore is a word boundary",
     "        let kept: String = lowered.chars().filter(|c| *c != ' ' && *c != '_').collect();",
     "        let kept: String = lowered.chars().filter(|c| *c != ' ').collect();"),
    ("naming", "slug", "a boundary before anything is not a boundary",
     "            pending = joiner != '\\u{0}' && !out.is_empty();",
     "            pending = joiner != '\\u{0}';"),
    ("naming", "collapse", "a doubled signature delimiter is collapsed",
     "        let doubled = matches!(character, '-' | '_' | '=' | '@') && previous == Some(character);",
     "        let doubled = matches!(character, '-' | '_' | '@') && previous == Some(character);"),
    ("naming", "slug_extension", "an extension is collapsed like every other component",
     "    let trimmed = collapse(cleaned.trim_matches('.'));",
     "    let trimmed = cleaned.trim_matches('.').to_owned();"),
    ("naming", "slug_extension", "an extension cannot carry the identifier's delimiter",
     "        .filter(|c| !c.is_whitespace() && !unusable(*c) && !FORBIDDEN.contains(c) && *c != '@')",
     "        .filter(|c| !c.is_whitespace() && !unusable(*c) && !FORBIDDEN.contains(c))"),
    ("naming", "slug_extension", "an extension cannot carry a control character",
     "        .filter(|c| !c.is_whitespace() && !unusable(*c) && !FORBIDDEN.contains(c) && *c != '@')",
     "        .filter(|c| !c.is_whitespace() && !FORBIDDEN.contains(c) && *c != '@')"),
    ("naming", "slug_signature", "a signature removes the hyphen it joins on",
     "    slug(text, &['-'], '=')",
     "    slug(text, &[], '=')"),
    ("naming", "slug_keyword", "a keyword joins its words with nothing",
     "    slug(text, &['-', '='], '\\u{0}')",
     "    slug(text, &['-', '='], '-')"),
    # `canonical_name` answers "already spelled right" with `Ok(None)`, so a
    # refusal swallowed on either of its two steps becomes that answer — and a
    # caller renaming files by it leaves the pair it asked about alone.
    ("naming", "canonical_name", "a name that cannot be built says so",
     "        let canonical = Self::parse(name)?.build()?;",
     "        let canonical = Self::parse(name)?.build().unwrap_or(name.to_owned());"),
    ("naming", "canonical_name", "a name that cannot be parsed says so",
     "        let canonical = Self::parse(name)?.build()?;",
     "        let Ok(p) = Self::parse(name) else { return Ok(None) };\n        let canonical = p.build()?;"),

    # -- release: the verb nobody runs ---------------------------------------
    ("release", "every_file_listed_as_carrying_the_version_actually_carries_one",
     "a version closes with the quote it opened with",
     "                let closes = if spelling.ends_with('\\'') { '\\'' } else { '\"' };",
     "                let closes = '\"';"),
    ("release", "written_as_the_reader_reads_it", "the rewrite keeps the file's own quotes",
     "    let Some((at, quote)) = ['\"', '\\'']",
     "    let Some((at, quote)) = ['\"', '\"']"),
    ("release", "bump_of", "a perf is a patch",
     "        \"fix\" | \"perf\" => Bump::Patch,",
     "        \"fix\" => Bump::Patch,"),
    ("release", "bumped", "a major is a minor while the major is 0",
     "        Bump::Major if major == 0 => Some([0, minor + 1, 0]),",
     "        Bump::Major if major == 1 => Some([0, minor + 1, 0]),"),
    ("release", "on_the_channel", "a candidate is one past the highest that exists",
     "                rc: Some(highest + 1),",
     "                rc: Some(1),"),
    ("release", "the_channel_above_is_merged", "staging waits for main's latest release",
     "        Some(tag) if !tag.merged => Err(format!(",
     "        Some(tag) if tag.merged => Err(format!("),
    ("release", "goes_forward", "a release behind the manifest's numbers is refused",
     "    if version.core < manifest.core {",
     "    if version.core > manifest.core {"),
    ("release", "the_points", "a candidate below the last release folds into it",
     "    candidates.retain(|point| latest.is_none_or(|latest| point.version > latest));",
     "    candidates.retain(|_| true);"),
    ("release", "the_part_of", "a breaking change is listed as changed",
     "    if change.breaking {\n        return Some(Part::Changed);",
     "    if false {\n        return Some(Part::Changed);"),
    ("release", "the_breaking_note", "the footer is read with a hyphen too",
     "        line.starts_with(\"BREAKING CHANGE:\") || line.starts_with(\"BREAKING-CHANGE:\")",
     "        line.starts_with(\"BREAKING CHANGE:\")"),
    ("release", "a_release_commit_of", "a merge is not a release commit",
     "    if head.parents != 1 {",
     "    if head.parents == 0 {"),
    ("release", "carried_on", "a release commit's changelog is the history's",
     "    if written != the_changelog(&sections).into_bytes() {",
     "    if written.is_empty() {"),
    ("release", "what_github_holds_is_this_releases", "a tag is never moved",
     "        (Some(at), Some(tagged)) if tagged != at => {",
     "        (Some(at), Some(tagged)) if false => {"),
    ("release", "what_github_holds", "an open pull request carries the release whatever its title",
     "        if read.state == Carried::Open {",
     "        if read.state == Carried::Merged {"),
    ("release", "of", "gh is handed none of its own variables but the token",
     "            .filter(|(key, _)| !ghs_own(key))",
     "            .filter(|_| true)"),
    # **What a run on a release commit, and a release from a tree, are held
    # to.** A release commit at another version, a tree holding what no commit
    # has, a fork's pull request taken as the one carrying the release back,
    # and auto-merge asked of a pull request GitHub would merge now each
    # publish what nobody wrote or reviewed, or stop a release for good, and
    # only the test written for each says so.
    ("release", "carried_on", "a release commit carries its own version",
     "    if manifest != version {",
     "    if false {"),
    ("release", "the_tree_is_clean", "a tree holding what no commit has is not released",
     "    if status.is_empty() {",
     "    if status.len() < usize::MAX {"),
    ("release", "from_this_repository", "a fork's pull request never carries a release",
     "        Some(true) => Ok(false),",
     "        Some(true) => Ok(true),"),
    ("release", "from_this_repository", "this repository's own pull request has its head here",
     "            if head.is_some_and(|head| head.eq_ignore_ascii_case(repository)) {",
     "            if head.is_some() {"),
    ("release", "published", "a pull request whose checks have passed is merged, not queued",
     "        if carried.mergeable {",
     "        if false {"),
    # **What GitHub holds is judged before a release commit is made, and what
    # it answers is held to the shape asked.** A refusal judged after the
    # commit leaves a release commit with no tag, which every run after it
    # carries into the same refusal; a merge asked on a head this run moved
    # lands a release before its checks; and a step answered in another shape
    # taken as done publishes what was never made. Only the test written for
    # each says so.
    ("release", "released", "what GitHub holds is judged before the release is written",
     "            what_github_holds_is_this_releases(&holds, setting.channel, version, None)?;\n"
     "            let written = written(setting, version, &changelog)?;\n",
     "            let written = written(setting, version, &changelog)?;\n"
     "            what_github_holds_is_this_releases(&holds, setting.channel, version, None)?;\n"),
    ("release", "what_github_holds_is_this_releases", "a version GitHub holds is not made again",
     "        (None, tagged) if tagged.is_some() || holds.release => {",
     "        (None, tagged) if tagged.is_some() && holds.release => {"),
    ("release", "what_github_holds_is_this_releases",
     "a pull request closed without merging stops the release",
     "        .filter(|carried| carried.state == Carried::Closed)",
     "        .filter(|_| false)"),
    ("release", "once_the_branch_moved",
     "a pull request whose head this run moved is queued, never merged now",
     "            mergeable: false,",
     "            mergeable: carried.mergeable,"),
    ("release", "once_the_branch_moved",
     "a pull request in conflict before the release commit moved its head is refused after",
     "            conflicting: false,",
     "            conflicting: carried.conflicting,"),
    ("release", "once_the_branch_moved",
     "a pull request merged before the release commit does not carry it",
     "        .filter(|carried| carried.state != Carried::Merged)",
     "        .filter(|_| true)"),
    ("release", "a_carried_back",
     "a pull request passing with hooks or beside checks no rule requires is merged now",
     "        Some(\"CLEAN\" | \"HAS_HOOKS\" | \"UNSTABLE\")",
     "        Some(\"CLEAN\")"),
    ("release", "auto_merged", "auto-merge is on by merge commit, or refused",
     "        != Some(\"MERGE\")",
     "        == Some(\"SQUASH\")"),
    ("release", "merged_now", "a merge GitHub does not say happened is refused",
     "        != Some(true)",
     "        == Some(false)"),
    # **Who is handed the App's token, what a gate that refused stops, what a
    # run carried on finds below, and how a pull request's node is read.** A
    # child of the release handed `gh`'s variables holds a token that may push
    # past `staging`'s and `main`'s rules; a gate that refused, read as passed
    # or its refusal dropped on the way out of the verb, lets a red tree be
    # committed, tagged and released; a release the branch
    # below already holds, carried back again, fails that run and every run
    # after it, and one a branch below that moved on since is read as holding
    # is never carried back; and a node read from REST's numeric `id` refuses
    # every release made fresh. Only the test written for each says so.
    ("release", "run_at", "the gate runs in a child of its own",
     "        the_gate(itself, root, inherited)?;",
     "        crate::check::run(root, false)?;"),
    ("release", "the_gate", "a gate that refused stops the release",
     "    if ran.success() {",
     "    if true {"),
    ("release", "run_at", "the verb hands back a gate's refusal",
     "        the_gate(itself, root, inherited)?;",
     "        let _ = the_gate(itself, root, inherited);"),
    ("release", "run", "the verb hands back what the release refused",
     "    run_at(\n        &itself,\n        root,\n        channel,\n        dry_run,\n        publish,\n        after_the_gate_job,\n        &inherited,\n    )\n",
     "    drop(run_at(\n        &itself,\n        root,\n        channel,\n        dry_run,\n        publish,\n        after_the_gate_job,\n        &inherited,\n    ));\n    Ok(())\n"),
    ("release", "without_ghs_own", "no child but gh is handed gh's variables",
     "        .filter(|(name, _)| !ghs_own(name))",
     "        .filter(|_| true)"),
    ("release", "a_gate", "the gate is handed nothing this process holds that its list leaves out",
     "        .arg(\"check\")\n        .env_clear()\n",
     "        .arg(\"check\")\n"),
    ("release", "relock", "the relock is handed none of gh's variables",
     "        .env_clear()\n        .envs(without_ghs_own(inherited))\n        .current_dir(root)\n        .output()",
     "        .env_clear()\n        .envs(inherited.iter().map(|(name, value)| (name, value)))\n        .current_dir(root)\n        .output()"),
    ("release", "carried_on", "the branch below is asked about a release no pull request carries",
     "let below = holds.carried.is_none() && held_below(",
     "let below = false && held_below("),
    ("release", "published", "a release the branch below holds is not carried back again",
     "        None if holds.below => None,",
     "        None if false => None,"),
    ("release", "held_below", "a branch below gone on past the release commit holds it",
     "        Some(\"BEHIND\" | \"IDENTICAL\") => Ok(true),",
     "        Some(\"IDENTICAL\") => Ok(true),"),
    ("release", "held_below", "a branch below that moved on since does not hold the release",
     "        Some(\"BEHIND\" | \"IDENTICAL\") => Ok(true),\n        Some(\"AHEAD\" | \"DIVERGED\") => Ok(false),",
     "        Some(\"BEHIND\" | \"IDENTICAL\" | \"DIVERGED\") => Ok(true),\n        Some(\"AHEAD\") => Ok(false),"),
    ("release", "a_carried_back", "a pull request's node is its node_id where its id is a number",
     "        .get(\"node_id\")\n        .and_then(serde_json::Value::as_str)\n        .or_else(|| node.get(\"id\").and_then(serde_json::Value::as_str))",
     "        .get(\"id\")\n        .or_else(|| node.get(\"node_id\"))\n        .and_then(serde_json::Value::as_str)"),
    # **What a production release is.** Published as a pre-release, or not as
    # the latest, a release on `main` is not the one GitHub shows as current;
    # carried back to `dev`, it reaches `staging` only when `dev` is next
    # promoted, and until then `staging` refuses every candidate. Only the test
    # written for each says so.
    ("release", "the_github_release", "a production release is not a pre-release",
     "            \"prerelease\": channel == Channel::Staging,",
     "            \"prerelease\": true,"),
    ("release", "the_github_release", "a production release is the latest",
     "            \"make_latest\": if channel == Channel::Main { \"true\" } else { \"false\" },",
     "            \"make_latest\": \"false\","),
    ("release", "below", "a production release is carried back to staging",
     "            Self::Main => \"staging\",",
     "            Self::Main => \"dev\","),
    # **What a release that failed part way puts back.** A listed file whose
    # read or write refused, a changelog that could not be written, and a relock
    # or a read-back that refused each leave the tree as the release found it,
    # or the refusal says nothing was released over a tree it changed — a
    # manifest ahead of its lockfile, which cargo refuses for every verb, or a
    # changelog truncated, partial or new. A changelog that is there and cannot
    # be read, taken as absent, is one a failure removes. Only the test written
    # for each says so.
    ("release", "rewrite_listed", "a refused rewrite puts back every file it read",
     "            let not_back = put_back_listed(root, &before);",
     "            let not_back: Vec<String> = Vec::new();"),
    ("release", "rewrite_each", "every file a rewrite reads is one its refusal puts back",
     "        before.push((name, text.clone()));",
     "        // nothing recorded"),
    ("release", "written", "a changelog that cannot be read is not taken as absent",
     "        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,",
     "        Err(_) => None,"),
    ("release", "written", "a release refused after the version is written puts back what it wrote",
     "            let not_back = put_back(root, &found);",
     "            let not_back: Vec<String> = Vec::new();"),
    ("release", "put_back", "a refused release puts the listed manifests back",
     "    put_back_listed(root, &found.manifests)\n        .into_iter()",
     "    Vec::<String>::new()\n        .into_iter()"),
    ("release", "put_back", "a refused release puts the lockfile back",
     "    let lockfile = put_the_lockfile_back(&root.join(\"Cargo.lock\"), &found.locked)",
     "    let lockfile = std::io::Result::<()>::Ok(())"),
    ("release", "put_back", "a refused release puts the changelog back",
     "    let changelog = put_the_changelog_back(&root.join(CHANGELOG), found.changelog.as_deref())",
     "    let changelog = std::io::Result::<()>::Ok(())"),
    ("release", "put_the_changelog_back", "a changelog the release found is put back as it was",
     "        Some(bytes) => put_back_as(changelog, bytes),",
     "        Some(_) => Ok(()),"),
    ("release", "put_the_changelog_back", "a changelog the tree had none of is removed",
     "        None => match std::fs::remove_file(changelog) {\n            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),\n            removed => removed,\n        },",
     "        None => Ok(()),"),
    ("release", "put_the_changelog_back", "a changelog never created is no failure to put it back",
     "            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),",
     "            Err(error) if error.kind() == std::io::ErrorKind::Other => Ok(()),"),
    # **Where the gate stands aside, what a file the release writes must be,
    # what a refusal on GitHub leaves, and what a pull request back that cannot
    # merge says.** The verb's gate stood aside without `--publish`, outside
    # GitHub Actions, in another job, workflow, branch or repository releases a
    # tree no gate judged; a link written through moves a file outside the
    # tree, or creates one; a refusal at the release commit or after it that
    # puts nothing back leaves the runner's tree written; and a conflict queued,
    # or auto-merge refused with nothing said, stalls a release with nobody told
    # how to carry it back. Only the test written for each says so.
    ("release", "the_gate_ran_in_the_job_before", "the gate stands aside without --publish",
     "    if !publish {",
     "    if false {"),
    ("release", "the_gate_ran_in_the_job_before", "the gate stands aside outside GitHub Actions",
     "    if actions.as_deref() == Some(\"true\")",
     "    if true"),
    ("release", "the_gate_ran_in_the_job_before", "the gate stands aside in any job",
     "        && job.as_deref() == Some(THE_RELEASE_JOB)\n",
     "\n"),
    ("release", "the_gate_ran_in_the_job_before", "the gate stands aside in any workflow on any branch",
     "        && in_this_workflow\n",
     "\n"),
    ("release", "the_gate_ran_in_the_job_before", "the gate stands aside in another repository's release job",
     "named.eq_ignore_ascii_case(&repository)",
     "true"),
    ("release", "run_at", "the gate stands aside wherever the flag is passed",
     "        the_gate_ran_in_the_job_before(channel, publish, inherited)?;",
     "        let _ = (channel, publish);"),
    ("release", "written", "the files a release writes are not judged before the first write",
     "    each_is_a_file_of_this_trees(root)?;",
     "    let _ = each_is_a_file_of_this_trees;"),
    ("release", "each_is_a_file_of_this_trees", "a link is a file of this tree's",
     "        if found.is_file() {",
     "        if true {"),
    ("release", "released", "a refusal on GitHub leaves the tree it wrote",
     "                .map_err(|problem| with_the_tree_put_back(setting.root, &written.found, &problem))",
     "                .map_err(|problem| problem)"),
    ("release", "with_the_tree_put_back", "a refusal on GitHub puts nothing back",
     "    let not_back = put_back(root, found);",
     "    let not_back: Vec<String> = Vec::new();"),
    ("release", "published", "a pull request back in conflict is queued",
     "        if carried.conflicting {",
     "        if false {"),
    ("release", "published", "auto-merge refused says nothing of carrying the release back",
     "                auto_merged(github, carried).map_err(|problem| {\n                    carried_back_by_hand(channel, version, carried.number, &problem)\n                })?;",
     "                auto_merged(github, carried)?;"),
    ("release", "a_carried_back", "a pull request REST says conflicts is not in conflict",
     "node.get(\"mergeable\") == Some(&serde_json::Value::Bool(false))",
     "false"),
    ("release", "a_carried_back", "a pull request GraphQL says is dirty is not in conflict",
     "            == Some(\"DIRTY\");",
     "            == Some(\"NEVER\");"),
    # -- history: what a commit and a pull request may say -------------------
    ("history", "subject", "a subject opens with one of the types",
     "    if !TYPES.contains(&kind) {",
     "    if kind.is_empty() {"),
    ("history", "subject", "a description does not end with a period",
     "    if description.ends_with('.') {",
     "    if description.ends_with(\"..\") {"),
    ("history", "subject", "a character nobody sees is refused, not only a control",
     "    if line.chars().any(nc_schema::unseen) {",
     "    if line.chars().any(char::is_control) {"),
    ("history", "in_a_scope", "a scope holds no space",
     "        || character.is_ascii_digit()\n        || matches!(character, '-' | '_' | '.' | '/')",
     "        || character.is_ascii_digit()\n        || matches!(character, '-' | '_' | '.' | '/' | ' ')"),
    ("history", "without_the_pull_request", "only digits are GitHub's number",
     "    if number.is_empty() || !number.chars().all(|character| character.is_ascii_digit()) {",
     "    if number.is_empty() {"),
    ("history", "without_the_pull_request", "and an empty one is not",
     "    if number.is_empty() || !number.chars().all(|character| character.is_ascii_digit()) {",
     "    if !number.chars().all(|character| character.is_ascii_digit()) {"),
    ("history", "names_an_agent", "a bot is named by its bracket",
     '    value.contains("[bot]")',
     '    value.contains("[bot]]")'),
    ("history", "a_trailer", "a trailer ending `-with` is one",
     "    (shaped && (key.ends_with(\"-by\") || key.ends_with(\"-with\"))).then_some((key, value))",
     "    (shaped && key.ends_with(\"-by\")).then_some((key, value))"),
    ("history", "a_linked_site", "a link written without its scheme is read",
     "        let linked = gap.ends_with(\"://\") || after.starts_with('/');",
     "        let linked = gap.ends_with(\"://\");"),
    ("history", "judge_a_commit", "the line after the subject is blank",
     "    if lines.next().is_some_and(|second| !second.trim().is_empty()) {",
     "    if lines.next().is_some_and(|second| second.starts_with(\"never\")) {"),
    ("history", "not_shallow", "a shallow clone is refused",
     "        \"true\" => Err(",
     "        \"true\" => Ok(()),\n        \"never\" => Err("),
    ("history", "the_objects", "an answer longer than the question is refused",
     "    if !rest.is_empty() {",
     "    if rest.len() > answer.len() {"),
    ("history", "the_blobs", "a blob above the limit is judged",
     "        if size <= LIMIT {",
     "        if size <= LIMIT * 4 {"),
    ("history", "judge_the_places", "a listed path is held to its ceiling",
     "                Some((_, ceiling, _)) if size <= *ceiling => {}",
     "                Some(_) => {}"),
    ("history", "the_blobs", "every object named is measured",
     "    if answered != asked.len() {",
     "    if answered > asked.len() {"),
    ("history", "names_an_agent", "an address under an agent's site names it",
     "        || value.split('@').skip(1).any(|after| {",
     "        || value.split('@').skip(usize::MAX).any(|after| {"),
    ("history", "names_an_agent", "an address at an agent vendor's domain names it",
     "                .or_else(|| the_site_of(host, &AGENT_VENDORS))\n",
     ""),
    ("history", "an_agent_opens", "an article may stand before the agent's name",
     "        Some(first) if ARTICLES.contains(&first.to_lowercase().as_str()) => &written[1..],",
     "        Some(first) if first.is_empty() => &written[1..],"),
    ("history", "an_agent_opens", "the agent is named right after the words, not later",
     "        [first, ..] if an_agent(first) => true,",
     "        _ if named.iter().any(|word| an_agent(word)) => true,"),
    ("history", "an_agent_opens", "a maker's name may stand before its agent's",
     "if MAKERS.contains(&maker.to_lowercase().as_str()) => an_agent(then),",
     "if MAKERS.contains(&\"never\") => an_agent(then),"),
    ("history", "an_agent", "a name that is an ordinary word is not read in prose in lower case",
     "    AGENT_NAMES.contains(&word.as_str())\n        && (!ALSO_A_WORD.contains(&word.as_str()) || written.starts_with(char::is_uppercase))",
     "    AGENT_NAMES.contains(&word.as_str())"),
    ("history", "an_agent", "a name that is an ordinary word is read in prose written as a name",
     "|| written.starts_with(char::is_uppercase))",
     "|| written.is_empty())"),
    ("history", "a_trailer", "spaces and a tab before a trailer's colon are its key's",
     "    let key = key.trim_end_matches([' ', '\\t']);\n",
     ""),
    ("history", "a_credit", "a trailer's value folded onto the lines below is read",
     "            .take_while(|next| next.starts_with([' ', '\\t']))",
     "            .take_while(|next| next.starts_with(\"never\"))"),
    ("history", "judge_a_commit", "a message declared in another encoding is refused",
     "        && !declared.eq_ignore_ascii_case(\"utf-8\")\n        && !declared.eq_ignore_ascii_case(\"utf8\")",
     "        && declared.is_empty()"),
    ("history", "judge_a_commit", "a message that is not UTF-8 is refused",
     "    let Ok(message) = std::str::from_utf8(message) else {",
     "    let Ok(message) = std::str::from_utf8(message).or(Ok::<_, ()>(\"fix: a thing\")) else {"),
    ("history", "measure_the_blobs", "a partial clone is refused",
     "find(|line| line.first() == Some(&b'?'))",
     "find(|line| line.first() == Some(&b'!'))"),
    ("history", "measure_the_blobs", "a root commit's paths are read",
     "            \"--root\",\n",
     ""),
    ("history", "measure_the_blobs", "a merge's own paths are read",
     "            \"--diff-merges=first-parent\",",
     "            \"--diff-merges=off\","),
    ("history", "placed", "a blob no commit places is refused",
     "            if sits.is_empty() {",
     "            if sits.is_empty() && *size == 0 {"),
    ("history", "judge_the_places", "a listed path is matched byte for byte",
     "named.as_bytes() == path.as_slice())",
     "named.as_bytes() == path.trim_ascii())"),
    ("history", "judge_the_places", "every path a blob sits at is judged",
     "        for path in sits {",
     "        for path in sits.iter().take(1) {"),
    ("history", "judge_the_places", "the path a finding names is quoted",
     "            let at = quoted(&String::from_utf8_lossy(path));",
     "            let at = format!(\"`{}`\", String::from_utf8_lossy(path));"),
    ("history", "a_git", "a replacement ref shows git no other commit",
     "        .arg(reading.root)\n        .arg(\"--no-replace-objects\");",
     "        .arg(reading.root);"),
    ("history", "a_git", "git is handed the tree's own repository and searches for none",
     "        .env(\"GIT_DIR\", \".git\")",
     "        .env(\"NC_NEVER\", \".git\")"),
    ("history", "a_git", "a missing object is never fetched",
     "        .env(\"GIT_NO_LAZY_FETCH\", \"1\")",
     "        .env(\"NC_NEVER\", \"1\")"),
    ("history", "inheriting", "no variable of git's own reaches it",
     ".is_some_and(|start| start.eq_ignore_ascii_case(b\"GIT_\"))",
     ".is_some_and(|start| start.is_empty())"),
    ("history", "inheriting", "a variable of git's own is removed in any case",
     "start.eq_ignore_ascii_case(b\"GIT_\")",
     "start == b\"GIT_\""),
    ("history", "inheriting", "no variable of gh's own reaches git",
     "            || crate::release::ghs_own(name)\n",
     "            || false\n"),
    ("history", "judged_against", "git's variables are removed from this process's own environment",
     "std::env::vars_os().collect();\n    judged_inheriting(",
     "Vec::new();\n    judged_inheriting("),
    ("history", "not_grafted", "a grafted clone is refused",
     "    if grafted {",
     "    if grafted && named.is_empty() {"),
    ("history", "judge_the_pull_request", "a pull request's title is read for a credit",
     "        (\"the pull request's title\", pull_request.title.as_str()),\n",
     ""),
    ("history", "judged_inheriting", "the merge made for the run is measured",
     "starts.iter().copied().chain(merge.as_deref()).collect();",
     "starts.iter().copied().chain(None).collect();"),
    ("history", "judged_inheriting", "the merge made for the run is read for its paths",
     "the_commits_reached(&reading, &measured)?;",
     "the_commits_reached(&reading, &starts)?;"),
    ("history", "judged_inheriting", "every commit the merge made for the run reaches is read for its paths",
     "let placing = the_commits_reached(&reading, &measured)?;",
     "let placing = [commits.clone(), merge.clone().into_iter().collect()].concat();"),
    # -- ui: what counts as an announced skip --------------------------------
    ("ui", "warn", "a skip is declared at the start of its line",
     '    if text.starts_with("skipped:") {',
     '    if text.contains("skipped:") {'),
    ("ui", "warn", "and the colon is what declares it",
     '    if text.starts_with("skipped:") {',
     '    if text.starts_with("skipped") {'),
    # -- docs: whether a page is rewritten -----------------------------------
    ("docs", "already_there", "a page that differs is written",
     "    std::fs::read_to_string(path).is_ok_and(|on_disk| on_disk == contents)",
     "    std::fs::read_to_string(path).is_ok()"),
    # -- registry: the only place a generated limit meets its file -----------
    ("registry", "every_limit_carries_its_own_numbers", "every declared table is compared",
     "    for (name, entry) in &catalog.limits.tables {",
     "    for (name, entry) in catalog.limits.tables.iter().take(1) {"),
    ("registry", "every_limit_carries_its_own_numbers", "every declared surface is compared",
     "    for entry in &catalog.limits.rate_limits {",
     "    for entry in catalog.limits.rate_limits.iter().take(1) {"),

    # -- entry: what a struct body is, and what only looks like its end -------
    ("entry", "past_a_literal", "a raw literal is one",
     "    let raw = characters.get(at) == Some(&'r');",
     "    let raw = false;"),
    ("entry", "past_a_literal", "a raw literal's hashes are counted",
     "        while characters.get(cursor) == Some(&'#') {",
     "        if characters.get(cursor) == Some(&'#') {"),
    ("entry", "past_a_literal", "an escape covers the character after it",
     "            '\\\\' if !raw => cursor += 2,",
     "            '\\\\' if !raw => cursor += 1,"),
    ("entry", "past_a_literal", "and a raw literal has no escapes",
     "            '\\\\' if !raw => cursor += 2,",
     "            '\\\\' => cursor += 2,"),
    # -- loader: the emptiness one question decides --------------------------
    ("loader", "chord", "a binding that shows a reader nothing is empty",
     "    if says_nothing(binding) {",
     "    if binding.trim().is_empty() {"),
    ("loader", "what_a_dispatcher_needs", "a command names a resource selector",
     "        resource_selector: as_shown(&entry.resource_selector),",
     "        resource_selector: &entry.resource_selector,"),
    # Every other fixture breaks one field, so a loop that kept the judge's
    # first problem and dropped the rest passed all of them.
    ("loader", "what_a_dispatcher_needs", "every problem the judge finds is reported",
     "    for problem in found {",
     "    for problem in found.into_iter().take(1) {"),
    # The judge's problems reach the catalog under checks of their own, and a
    # test asserts on each; these two are the ones a remedy differs between.
    ("loader", "the_check_for", "a malformed selector is a check of its own",
     "            \"selector-malformed\",",
     "            \"selector-names-no-entity\","),
    ("loader", "the_check_for_a_schema", "an unknown keyword is a check of its own",
     "            \"schema-unknown-keyword\",",
     "            \"schema-outside-the-subset\","),
    # And the three the judge gained: each has a remedy of its own, and a test
    # of the gates asks for each check by name.
    ("loader", "the_check_for", "a realm reached through an argument is a check of its own",
     "            \"selector-realm-through-an-argument\",",
     "            \"selector-names-no-entity\","),
    ("loader", "the_check_for_a_schema", "an unseen character in a schema is a check of its own",
     "            \"schema-invisible-character\",",
     "            \"schema-outside-the-subset\","),
    ("loader", "the_check_for", "an unseen character in a title is a check of its own",
     "            \"command-title-invisible-character\",",
     "            \"command-title-too-long\","),
    ("loader", "locale_files", "a locale's extension is read in any case",
     '            .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))',
     '            .is_some_and(|extension| extension == "toml")'),
    # -- pages: what a published cell may not carry --------------------------
    ("pages", "cell", "an angle bracket is escaped",
     """    text.replace('\\\\', "\\\\\\\\")
        .replace('|', "\\\\|")
        .replace('<', "&lt;")""",
     """    text.replace('\\\\', "\\\\\\\\").replace('|', "\\\\|")"""),
    ("pages", "cell", "and so is the bar that would end the cell",
     """    text.replace('\\\\', "\\\\\\\\")
        .replace('|', "\\\\|")
        .replace('<', "&lt;")""",
     """    text.replace('\\\\', "\\\\\\\\").replace('<', "&lt;")"""),

    # -- declared_in: whether a fixture wrote its own sentence ---------------
    # **The clause acceptance criterion 2 rests on.** Measured without its
    # subject side: all four string fixtures forged by declaring `struct String;`
    # and `struct str;` — every `//! EXPECT:` line byte-identical — and then the
    # wall itself deleted, with `ok 11 fixture(s) refuse to compile` and
    # `check passed`. The subject side of the rule closes that; `corpus.rs`
    # decides both sides case by case.
    ("wall", "declared_in", "the subject side is asked at all",
     "        .filter(|part| only.is_empty() || only.contains(part))",
     "        .filter(|part| only.is_empty())"),
    ("wall", "declared_in", "a keyword list of seven, not six",
     '                "trait ", "struct ", "enum ", "mod ", "type ", "union ", "fn ",',
     '                "trait ", "enum ", "mod ", "type ", "union ", "fn ", "fn ",'),
    ("wall", "declared_in", "a declaration anywhere on the line",
     "            .find(|needle| source.contains(needle.as_str()));",
     "            .find(|needle| source.starts_with(needle.as_str()));"),

    # -- canon: the bytes a journal entry is hashed over --------------------
    # **A crate rather than a judge of one.** These are the clauses that defects
    # found here by hand, and their neighbours, turn on.
    ("canon", "write_string", "the escape's hex is lower case",
     'let _ = write!(out, "\\\\u{:04x}", control as u32);',
     'let _ = write!(out, "\\\\u{:04X}", control as u32);'),
    ("canon", "write", "an object's keys are ordered",
     "                        keys.sort_by(|left, right| utf16_cmp(left, right));",
     "                        keys.sort_by(|left, right| utf16_cmp(right, left));"),
    ("canon", "fmt", "the pairs are ordered as they are built",
     "                    ordered.sort_by(|(left, _), (right, _)| utf16_cmp(left, right));",
     "                    ordered.sort_by(|(left, _), (right, _)| utf16_cmp(right, left));"),
    ("canon", "hash", "the domain's length is in the digest",
     "    hasher.update(&(domain.len() as u64).to_le_bytes());",
     "    hasher.update(&(0_u64).to_le_bytes());"),
    ("canon", "hash", "the payload's length is in the digest",
     "    hasher.update(&(canonical.len() as u64).to_le_bytes());",
     "    hasher.update(&(0_u64).to_le_bytes());"),
    ("canon", "hash", "the domain reaches the hasher as it was given",
     "    hasher.update(domain.as_bytes());",
     "    hasher.update(domain.to_lowercase().as_bytes());"),
    ("canon", "utf16_cmp", "the comparison is over UTF-16 code units",
     "fn utf16_cmp(left: &str, right: &str) -> Ordering {",
     "fn utf16_cmp(left: &str, right: &str) -> Ordering {\n    return left.cmp(right);"),

    # -- reader: what a hostile document may say ----------------------------
    ("reader", "object", "a key written twice is shown escaped",
     "                    .map(crate::shown)\n",
     "                    .map(String::from)\n"),
    ("reader", "object", "a key written twice is shown no longer than its bound",
     "                    .take(KEY_SHOWN_AT_MOST)\n",
     "                    .take(usize::MAX)\n"),
    ("reader", "unicode_escape", "the high-surrogate range reaches its last value",
     "        if (0xd800..0xdc00).contains(&high) {",
     "        if (0xd800..0xdbff).contains(&high) {"),
    ("reader", "unicode_escape", "the low half is checked against the whole range",
     "            if !(0xdc00..0xe000).contains(&low) {",
     "            if !(0xdc00..0xdfff).contains(&low) {"),
    ("reader", "unicode_escape", "a lone low surrogate is refused",
     "        if (0xdc00..0xe000).contains(&high) {",
     "        if (0xdc00..0xdfff).contains(&high) {"),
    ("reader", "string", "every C0 character is refused raw",
     "                control if control < 0x20 => {",
     "                control if control < 0x1f => {"),
    ("reader", "hex4", "four hex digits are four",
     "        if !text.chars().all(|character| character.is_ascii_hexdigit()) {",
     "        if false {"),
    # `width_of` decides how many bytes the reader steps by, and both ends of
    # every class it names need a case of their own: the generator draws the
    # interior of each arm and never an end, so without one a rendering this
    # crate has just written can fail to be read back with the whole gate
    # byte-identical.
    ("reader", "width_of", "the three-byte class reaches its top",
     "            0xe0..=0xef => 3,",
     "            0xe0..=0xee => 3,"),
    ("reader", "width_of", "the three-byte class starts where it starts",
     "            0xe0..=0xef => 3,",
     "            0xe1..=0xef => 3,"),
    ("reader", "width_of", "the two-byte class reaches its top",
     "            0xc0..=0xdf => 2,",
     "            0xc0..=0xde => 2,"),
    ("reader", "width_of", "the one-byte class reaches its top",
     "            0x00..=0x7f => 1,",
     "            0x00..=0x7e => 1,"),
    # -- caret_uses: the fixture's refusal is the door's --------------------
    # Measured without `caret_uses`, two fixtures rewritten to fail for reasons
    # that have nothing to do with the wall gave `check passed` in both cases,
    # with `Origin`'s fields put back to `pub &'static str` under one of them.
    # What answers is where rustc put the caret, which a fixture does not choose.
    ("wall", "caret_uses", "the caret's own line",
     "    if names(caret) {\n        return true;\n    }", "    if false {\n        return true;\n    }"),
    ("wall", "caret_uses", "the enclosing blocks, by indentation",
     "        if indent(above) >= deepest {\n            continue;\n        }",
     "        if false {\n            continue;\n        }"),
    # `caret_uses` reads its text with every comment spent — a comment of
    # either shape is not code — and accepts only the qualified path, so what
    # is left to weaken is the door clause.
    # **A comment and a string literal are both spent, in one copy.** Read
    # through `without_comment` alone, a door inside a *literal* on a
    # block-opening line is code — `let held = vec!["nc_log::LogValue", {`
    # names the door in a value the fixture chose. Both questions read one
    # blanked copy, so this operator is anchored to that call.
    ("wall", "caret_uses", "a comment is not code",
     "crate::gates::blanked(text).contains(door)",
     "text.contains(door)"),
    # **The half of the caret rule that would read prose.** The door half above
    # spends the comment, and so must this one, three lines apart in the same
    # function — without it, `// version` on a caret line lets
    # `Origin::version` be demoted from `Literal` to `&'static str` with
    # `check passed`, measured.
    # A comment and a string literal are not the fixture's subject, and neither
    # is the door: both halves read the file through `readable`, so planting the
    # stripping away is planting both rules away at once.
    ("wall", "the_refusal_is_the_doors", "a fixture is read as code",
     "    let stripped = readable(whole_fixture);",
     "    let stripped = whole_fixture.to_owned();"),

    # -- claim: a numeral is a count, or a label ----------------------------
    # Every digit form of every countable noun falls through a rule written on
    # the grounds that "nothing in these documents does" it — a measurement of
    # one day's prose rather than a property of the rule.
    ("counts", "claim", "a counting word before the digits",
     "        let counted = preceding", "        let counted = false && preceding"),
    ("counts", "claim", "a digit mid-clause is still judged",
     "let names_something_else = (in_digits && !opens_a_clause && !counted)",
     "let names_something_else = (in_digits && !opens_a_clause)"),

    # -- one_of_them_is: the digest a tag actually resolves to ---------------
    # **A corpus block needs operators too.** `corpus.rs` records that
    # replacing this whole function with `Ok(true)` left the suite green, and
    # without operators here the clauses below are decided by the corpus and
    # weakened by nothing. It is the predicate that says whether
    # a pinned digest is the one its own tag names: wrong, and the stack a
    # developer brings up runs bytes anybody outside this repository can change.
    ("gates", "one_of_them_is", "the tag's repository, not the whole tag",
     '        .map_or(tag, |(before, _)| before);', '        .map_or(tag, |(_, after)| after);'),
    ("gates", "one_of_them_is", "a port is not a tag separator",
     '        .filter(|(_, after)| !after.contains(\'/\'))', '        .filter(|_| true)'),
    ("gates", "one_of_them_is", "the repository has to match",
     'named == repository && found == digest', 'found == digest'),
    ("gates", "one_of_them_is", "the digest has to match",
     'named == repository && found == digest', 'named == repository'),
    ("gates", "one_of_them_is", "the list is split on every delimiter",
     'split([\'"\', \',\', \'[\', \']\'])', 'split(\',\')'),

    # -- holds_an_environment: a committed secret ---------------------------
    ("gates", "holds_an_environment", "the env part is required",
     'if !parts.contains(&"env") && !name.starts_with(".env") {', 'if false {'),
    ("gates", "holds_an_environment", "the `.env` prefix clause",
     '&& !name.starts_with(".env")', '&& false'),
    ("gates", "holds_an_environment", "the format exemption is exactly two parts",
     'matches!(parts.as_slice(), ["env", last]', 'matches!(parts.as_slice(), [.., "env", last]'),
    ("gates", "holds_an_environment", "the first part is `env`",
     '["env", last]\n        if KNOWN_FORMATS.contains(last)',
     '[_, last]\n        if KNOWN_FORMATS.contains(last)'),
    ("gates", "holds_an_environment", "the suffix is a kind this tree holds",
     'KNOWN_FORMATS.contains(last) &&', 'true &&'),
    ("gates", "holds_an_environment", "an environment format is not a module",
     '&& !AN_ENVIRONMENT_IS_WRITTEN_IN.contains(last)', '&& true'),
    ("gates", "holds_an_environment", "the template words: any -> all",
     '.any(|part| matches!(*part, "example"', '.all(|part| matches!(*part, "example"'),
    ("gates", "holds_an_environment", 'the template words lose "dist"',
     'matches!(*part, "example" | "sample" | "template" | "dist")',
     'matches!(*part, "example" | "sample" | "template")'),
    ("gates", "holds_an_environment", "the parts are in written order",
     "let parts: Vec<&str> = name.split('.').collect();",
     "let parts: Vec<&str> = name.rsplit('.').collect();"),

    # -- codes_in + glued: a hand-written error code ------------------------
    ("gates", "codes_in", "four digits -> at least four",
     'if digits.len() == 4 {', 'if digits.len() >= 4 {'),
    ("gates", "codes_in", "four digits -> three",
     'if digits.len() == 4 {', 'if digits.len() == 3 {'),
    ("gates", "codes_in", "the area must be upper case",
     'is_some_and(char::is_ascii_uppercase)', 'is_some_and(|c| c.is_alphabetic())'),
    ("gates", "codes_in", "an empty area is not an area",
     "if area.is_empty() || characters.get(cursor) != Some(&'-') {",
     "if characters.get(cursor) != Some(&'-') {"),
    ("gates", "codes_in", "the separator is a hyphen",
     "if area.is_empty() || characters.get(cursor) != Some(&'-') {",
     "if area.is_empty() || characters.get(cursor) != Some(&'_') {"),
    ("gates", "codes_in", "the digits are digits",
     'is_some_and(char::is_ascii_digit)', 'is_some_and(|c| c.is_alphanumeric())'),
    ("gates", "codes_in", 'the prefix is "NC-"',
     'line.match_indices("NC-")', 'line.match_indices("NC")'),
    ("gates", "glued", "the comma is glue",
     "            '\"' | ',' => {", "            '\"' => {"),
    ("gates", "glued", "whitespace before the glue",
     "                while out.ends_with(' ') || out.ends_with('\\t') {\n                    out.pop();\n                }\n", ""),
    ("gates", "glued", "\\u as well as \\x",
     "matches!(characters.peek(), Some('x' | 'u'))",
     "matches!(characters.peek(), Some('x'))"),
    ("gates", "no_code_is_written_at_a_call_site", "run-time upper-casing: to_uppercase",
     'line.contains("to_ascii_uppercase") || line.contains("to_uppercase")',
     'line.contains("to_ascii_uppercase")'),

    # -- the rule's scope ---------------------------------------------------
    ("gates", "in_the_error_code_scope", "migrations/ too",
     '        || relative.starts_with("migrations/")\n        || relative.starts_with("local-migrations/")',
     '        || relative.starts_with("local-migrations/")'),
    # `plugins/` is in the scope because the plugin tree is a member tree:
    # measured without it, a hand-written code in the reference plugin passed
    # the step.
    ("gates", "in_the_error_code_scope", "plugins/ is inside the §15.1 scope",
     '        || relative.starts_with("local-migrations/")\n        || relative.starts_with("plugins/"))',
     '        || relative.starts_with("local-migrations/"))'),
    ("gates", "codes_something_raises", "the raising walk opens plugins/",
     '    if plugins.is_dir() {\n        walk(&plugins, &mut visit)?;',
     '    if false {\n        walk(&plugins, &mut visit)?;'),
    # The scope is `in_the_error_code_scope` and its named list, so the
    # walk, its floor and its fixture read one rule rather than three copies.
    # Both halves are weakened here: dropping the compiler from the exclusion
    # list, and dropping the list's use so nothing is excluded at all.
    # A duplicate rather than a deletion: the list is a fixed-length array, so
    # removing an entry is a compile error and would be reported "does not
    # compile" rather than measured.
    ("gates", "OUTSIDE_THE_ERROR_CODE_SCOPE", "the compiler is excluded",
     '    "crates/nc-catalog-compiler/",',
     '    "crates/nc-catalog/",'),
    ("gates", "in_the_error_code_scope", "the exclusion list is consulted",
     '    !OUTSIDE_THE_ERROR_CODE_SCOPE\n        .iter()\n        .any(|excluded| relative.starts_with(excluded))',
     '    true'),
    ("gates", "no_code_is_written_at_a_call_site", ".sql as well as .rs",
     '.is_none_or(|kind| kind != "rs" && kind != "sql")',
     '.is_none_or(|kind| kind != "rs")'),

    # -- lints_put_down + LINT_GROUPS: a blanket allow ----------------------
    ("gates", "lints_put_down", "expect( as well as allow(",
     'for opener in ["allow(", "expect("] {', 'for opener in ["allow("] {'),
    ("gates", "lints_put_down", "a word ending in allow( is not one",
     ".is_some_and(|before| before.is_alphanumeric() || before == '_')",
     ".is_some_and(|before| before.is_alphanumeric())"),
    ("gates", "lints_put_down", "..and an underscore is a word character",
     ".is_some_and(|before| before.is_alphanumeric() || before == '_')",
     ".is_some_and(|before| before == '_')"),
    ("gates", "lints_put_down", "nesting: depth == 0 -> depth <= 0",
     "')' if depth == 0 => break,", "')' if depth <= 0 => break,"),
    ("gates", "lints_put_down", "whitespace inside a lint name",
     "                        named\n                            .chars()\n                            .filter(|c| !c.is_whitespace())\n                            .collect::<String>()",
     "                        named.to_owned()"),
    ("gates", "LINT_GROUPS", 'loses "clippy::perf"',
     '    "clippy::perf",\n    "clippy::suspicious",', '    "clippy::suspicious",\n    "clippy::suspicious",'),
    ("gates", "LINT_GROUPS", 'loses "unused"',
     '    "warnings",\n    "unused",', '    "warnings",\n    "warnings",'),
    ("gates", "no_source_puts_the_lints_down", "the wrapped-attribute accumulator",
     'if attribute.len() < 4_000 {', 'if attribute.len() < 0 {'),
    ("gates", "no_source_puts_the_lints_down", "an inner attribute, not an outer one",
     'None if code.starts_with("#![") => (index, code.to_owned(), 1, false),',
     'None if code.starts_with("#!") => (index, code.to_owned(), 1, false),'),
    ("gates", "no_source_puts_the_lints_down", "an outer attribute is judged by its item",
     'if a_module_declaration(code) {',
     'if !a_module_declaration(code) {'),
    ("gates", "a_module_declaration", "a module, not a name beginning with one",
     'code.starts_with("mod ")',
     'code.starts_with("mod")'),

    # -- every_hatch_carries_its_reason: a lint put down says why ------------
    ("gates", "every_hatch_carries_its_reason", "a doc comment is not a reason",
     '&& !line.starts_with("///")', '&& true'),
    ("gates", "every_hatch_carries_its_reason", "..nor an inner one",
     '&& !line.starts_with("//!")', '&& true'),
    ("gates", "every_hatch_carries_its_reason", "an attribute is code, not a string",
     'let code = bare.get(index).map_or("", |line| line.trim());',
     'let code = lines.get(index).map_or("", |line| line.trim());'),
    # The plugin tree, in both halves of the walk: the directories it opens and
    # the predicate every file is asked. Either alone left a reasonless allow in
    # `plugins/reference/src/` unjudged.
    ("gates", "every_hatch_carries_its_reason", "the hatch walk opens plugins/",
     'for tree in ["crates", "plugins", "xtask"] {',
     'for tree in ["crates", "xtask"] {'),
    ("gates", "in_the_hatch_scope", "plugins/ is inside the hatch scope",
     '        || relative.starts_with("plugins/")\n        || relative.starts_with("xtask/"))',
     '        || relative.starts_with("xtask/"))'),

    # -- is_a_test_attribute + runs + no_ignored_test: the whole suite --------
    ("gates", "is_a_test_attribute", "the last path segment, not a prefix",
     'path.rsplit("::").next() == Some("test")', 'path.starts_with("test")'),
    ("gates", "is_a_test_attribute", "a path, not the bare word",
     'path.rsplit("::").next() == Some("test")', 'path == "test"'),
    ("gates", "is_a_test_attribute", "a cfg_attr's arguments are resolved",
     '.strip_prefix("#[cfg_attr(")\n        .and_then(|rest| rest.strip_suffix(")]"))',
     '.strip_prefix("#[cfg_attrx(")\n        .and_then(|rest| rest.strip_suffix(")]"))'),
    ("gates", "runs", "a blank line stays inside a run",
     "        if code.is_empty() {\n            continue;\n        }\n", ""),
    ("gates", "runs", "an attribute accumulates until its brackets close",
     'if code.starts_with("#[") || code.starts_with("#![") {\n            if balanced(code) {',
     'if code.starts_with("#[") || code.starts_with("#![") {\n            if true {'),
    ("gates", "no_ignored_test", "an inner cfg_attr as well as an inner cfg",
     'if (code.starts_with("#![cfg(") || code.starts_with("#![cfg_attr(")) && has_a_test {',
     'if code.starts_with("#![cfg(") && has_a_test {'),
    ("gates", "no_ignored_test", "an inner condition needs a test below it",
     'if (code.starts_with("#![cfg(") || code.starts_with("#![cfg_attr(")) && has_a_test {',
     'if code.starts_with("#![cfg(") || code.starts_with("#![cfg_attr(") {'),
    ("gates", "no_ignored_test", 'ignore with a reason as well as bare',
     'let what = if attribute.starts_with("#[ignore") {',
     'let what = if attribute.starts_with("#[ignore]") {'),
    ("dev", "is_backdated", "a version below the highest applied",
     "version < highest", "false"),
    ("dev", "is_backdated", "one already applied is not backdated",
     "version < highest && !applied.contains(&version)", "version < highest"),
    ("gates", "no_ignored_test", "a module is a test module by what is in it",
     "|| opens_a_module(&run.item) && holds_a_test(&lines, run.at)",
     "|| false"),
    ("gates", "no_ignored_test", "a `mod x;` is a whole file of tests",
     "bare.starts_with(\"mod \") && bare.ends_with(';')", "false"),
    ("gates", "no_ignored_test", "a `mod x;` behind a visibility",
     "let bare = without_a_visibility(run.item.trim_start());",
     "let bare = run.item.trim_start();"),

    # -- excused + is_the_marker: an unbounded table --------------------------
    # Two halves — a `names(reason, table)` and a `.`-rule beside it — can be
    # satisfied by *different* occurrences: a reason about
    # `archive.login_attempts` plus a mention inside `login_attempts_history`
    # excuses a table with no entry at all. One scan decides it, so these
    # weaken the one scan: drop the word boundary before the name, and drop the
    # schema-qualifier rule.
    ("compiler", "names", "the reason names the table, not a longer word",
     "boundary(reason[..at].chars().next_back())\n            && (before != Some('.') || public)",
     "(before != Some('.') || public)"),
    ("compiler", "names", "a schema qualifier is another table",
     "&& (before != Some('.') || public)", "&& true"),
    # **`public.` is the one schema that names this table, and the escape is its
    # own clause.** Without it the gate refuses `public.login_attempts`, which
    # is what `psql` prints, measured.
    ("compiler", "names", "`public.` is not another schema",
     "&& (before != Some('.') || public)", "&& before != Some('.')"),
    # -- a_reason_names_what_it_exempts: a name is not a reason ---------------
    # The second half of both hatches — `nc:bounded:` and `pack_reason`. With
    # no corpus row whose reason is the bare table name, switching it off
    # leaves `cargo test --workspace --locked` at exit 0 and a bare table name
    # excuses an unbounded table, measured; `corpus.rs` carries that row, and
    # this operator is what says it still decides the clause.
    ("compiler", "a_reason_names_what_it_exempts", "a name repeated is still a name",
     'let rest = reason.to_lowercase().replace(&table.to_lowercase(), " ");',
     'let rest = reason.to_lowercase().replacen(&table.to_lowercase(), " ", 1);'),
    # And the fold itself: PostgreSQL resolves an unquoted identifier in one
    # case and `replace` does not, so the table's own name written in upper case
    # counted as prose about it.
    ("compiler", "a_reason_names_what_it_exempts", "the name is folded, because SQL folds it",
     'let rest = reason.to_lowercase().replace(&table.to_lowercase(), " ");',
     'let rest = reason.replace(table, " ");'),
    ("compiler", "a_reason_names_what_it_exempts", "either half refuses, not both",
     "if words < 2 || written < 10 {",
     "if words < 2 && written < 10 {"),
    ("compiler", "a_reason_names_what_it_exempts", "two words, not one",
     "if words < 2 ||", "if words < 1 ||"),
    ("compiler", "a_reason_names_what_it_exempts", "ten characters, not one",
     "|| written < 10 {", "|| written < 1 {"),
    # And what is counted: the characters of the words somebody wrote, not the
    # span they sit in. `rest.trim().len()` counts the runs of spaces between
    # two words, so ten spaces satisfy "ten characters of prose".
    ("compiler", "a_reason_names_what_it_exempts", "the characters, not the span they sit in",
     "let written: usize = distinct.iter().map(|word| word.chars().count()).sum();",
     "let written: usize = rest.trim().len();"),
    # A word has to hold a letter. The digits are in the *length* test, not the
    # *word* test — `at most 1 row` is a bound, and a word test refuses it — so
    # this is the clause that keeps `0000000000 0` from being two words, and it
    # is its own.
    ("compiler", "a_reason_names_what_it_exempts", "a word holds a letter, not any character",
     ".filter(|word| word.chars().any(char::is_alphabetic))",
     ".filter(|word| !word.is_empty())"),
    # And a word is a run of letters and digits: splitting on whitespace leaves
    # the punctuation attached, so one comma makes two words out of one.
    ("compiler", "a_reason_names_what_it_exempts", "a word is letters and digits, not non-spaces",
     "for word in rest.split(|c: char| !c.is_alphanumeric()) {",
     "for word in rest.split_whitespace() {"),
    # And the qualifier read the way SQL prints it, quotes and all.
    ("compiler", "names", "a quoted name is folded like an unquoted one",
     "    let folds = !table.chars().any(char::is_uppercase);", "    let folds = true;"),
    ("compiler", "names", "an unquoted name is not folded at all",
     "    let folds = !table.chars().any(char::is_uppercase);\n    let reason",
     "    let folds = false;\n    let reason"),
    # **One pass over quotes alone.** The clause it weakens — a quoted
    # qualifier is a qualifier — is a single `trim_end_matches`, because
    # whitespace is deliberately *not* trimmed: a `.` followed by a space is the
    # end of a sentence, and reading it as a schema refuses true markers.
    # Deleting the trim makes `archive."login_attempts"` name this table.
    #
    # **And a character a reader sees nothing of does not stand between the dot
    # and the name.** `archive.<U+200B>login_attempts` renders identically to
    # the spelling above and must not be excused; the corpus carries both it
    # and the tab.
    ("compiler", "names", "a quoted schema qualifier is still a qualifier",
     "        let head = reason[..at].trim_end_matches(|it: char| {\n            matches!(it, '\"' | '`' | '\\'' | '[') || renders_as_nothing(it)\n        });",
     "        let head = &reason[..at];"),
    ("compiler", "names", "an invisible character does not stand between the dot and the name",
     "        let head = reason[..at].trim_end_matches(|it: char| {\n            matches!(it, '\"' | '`' | '\\'' | '[') || renders_as_nothing(it)\n        });",
     "        let head = reason[..at].trim_end_matches('\"');"),
    # **And the quotes prose actually uses, and whitespace winning over them.**
    # A backtick is this repository's house style for an identifier and walks
    # past a rule that reads only quotes; a tab is in the C0 block that
    # `shows_nothing` answers from, and renders as a gap rather than as nothing.
    ("compiler", "names", "a backtick dresses a name as a quote does",
     "        let head = reason[..at].trim_end_matches(|it: char| {\n            matches!(it, '\"' | '`' | '\\'' | '[') || renders_as_nothing(it)\n        });",
     "        let head = reason[..at].trim_end_matches(|it: char| {\n            !it.is_whitespace() && (matches!(it, '\"') || shows_nothing(it))\n        });"),
    ("compiler", "names", "a boundary is not a character a reader sees nothing of",
     "        c.is_none_or(|c| !c.is_alphanumeric() && c != '_' && !renders_as_nothing(c))",
     "        c.is_none_or(|c| !c.is_alphanumeric() && c != '_')"),
    ("compiler", "names", "whitespace ends a sentence whatever else it is",
     "        let head = reason[..at].trim_end_matches(|it: char| {\n            matches!(it, '\"' | '`' | '\\'' | '[') || renders_as_nothing(it)\n        });",
     "        let head = reason[..at].trim_end_matches(|it: char| {\n            matches!(it, '\"' | '`' | '\\'' | '[') || shows_nothing(it)\n        });"),
    ("limits", "BOUNDED", "the marker is spelled `nc:bounded:`",
     'pub(crate) const BOUNDED: &str = "nc:bounded:";',
     'pub(crate) const BOUNDED: &str = "nc:bounded::";'),
    # **And the rename path, which is the same clause written twice.** The
    # second operator below rewrites the *create* path and reports `killed`, so
    # without the first the run prints a verdict reading "this rule is decided"
    # over a copy decided by nothing: `alone: true` on the rename left the whole
    # xtask suite green, measured.
    ("limits", "what_one_file_declares", "a second rename on the line is not excused",
     "            alone: !rename.shares_its_line && rename.alone_in_its_statement,",
     "            alone: true,"),
    ("limits", "what_one_file_declares", "a second statement on the line is not excused",
     "                alone: !table.shares_its_line && table.alone_in_its_statement,",
     "                alone: table.alone_in_its_statement,"),
    ("limits", "excused", "the block is contiguous with the statement",
     "        if !comments.contains(&at) {\n            return false;\n        }\n", ""),
    ("limits", "excused", "only a line the scanner called a comment",
     "if !comments.contains(&at) {", "if false {"),

    # -- the redirect rule: search_path, and the statements that re-aim it -----
    # Without operators and a corpus, this gate can be wired to nothing
    # unnoticed: measured, handed the first 400 bytes of every migration it
    # refused nothing with `check`'s output byte-identical.
    # `the_redirect_predicate_decides_every_clause` decides these.
    # The anchor carries the line below it, because `let all = statements(text);`
    # is written twice in this file and an operator that matches twice is one
    # `mutants.py` calls `not applicable` — which is a failure here, not a free
    # pass.
    ("limits", "unreadable_statements_in", "the reader is handed part of the file",
     "for (index, statement) in statements_in(text, dialect).into_iter().enumerate() {\n        let tokens = &statement.tokens;",
     "for (index, statement) in statements_in(&text[..400.min(text.len())], dialect)\n        .into_iter()\n        .enumerate()\n    {\n        let tokens = &statement.tokens;"),
    ("limits", "unreadable_statements_in", "the count is taken before the loop, not inside it",
     "        judged = judged.wrapping_add(the_weight_of(tokens));", ""),
    # **The token count, not the statement count.** A `judged += 1` per
    # iteration moves whatever the body does with its tokens, so re-slicing them
    # to nothing leaves the tail of every migration unjudged and both counts
    # agreeing.
    ("limits", "unreadable_statements_in", "the rules are handed nothing after the eighth statement",
     "        let tokens = &statement.tokens;\n        let is =",
     "        let tokens = &statement.tokens[..statement.tokens.len() * usize::from(index < 8)];\n        let is ="),
    # The comparison itself carries no operator. Deleting it alone changes
    # nothing any fixture can see, because with both readers healthy the two
    # counts agree — and truncation is not caught by the comparison alone: the
    # corpus case with a four-hundred-byte header catches truncation on its own.
    ("limits", "the_path_redirects_in", "`SET ROLE` only where the statement opens with it",
     "let aims_the_path = (0..tokens.len()).any(|at| {",
     "let aims_the_path = (0..1.min(tokens.len())).any(|at| {"),
    ("limits", "the_path_redirects_in", "`session_authorization` is not one word",
     'matches!(named.as_deref(), Some("role" | "session_authorization"))',
     'matches!(named.as_deref(), Some("role"))'),
    ("limits", "the_path_redirects_in", "`DISCARD ALL` is not `RESET ALL`",
     '|| (matches!(word(at).as_deref(), Some("discard"))\n                    && matches!(word(at + 1).as_deref(), Some("all")))',
     ""),
    ("limits", "the_path_redirects_in", "a `RESET` has to name the path",
     '|| tokens.iter().any(|token| token.is("ALL"))', ""),
    ("limits", "the_path_redirects_in", "every statement writes a setting",
     '(token.is("SET") || token.is("RESET")) && !assigns_a_column(at)',
     "!assigns_a_column(at)"),
    ("limits", "the_path_redirects_in", "a statement that writes rows also writes settings",
     'if !token.is("UPDATE") {',
     'if !token.is("NOT_A_SQL_KEYWORD") {'),
    # **The hatch decided by presence rather than by position.** `UPDATE` is an
    # unreserved keyword and PL/pgSQL merges an `UPDATE` and a later `SET` into
    # one statement, so "anywhere" lets two real redirects through.
    ("limits", "the_path_redirects_in", "an UPDATE anywhere is an UPDATE statement",
     'if !opens_a_statement(before) {',
     'if false {'),
    ("limits", "read_by_hand", "the cleared statement is a substring, not the line",
     "        .any(|cleared| source == just_the_words(cleared))",
     "        .any(|cleared| source.contains(&just_the_words(cleared)))"),

    # -- the refusals the driver sees because its filter reads every verdict ----
    # Measured, many clauses of this file were killed only by `limits::tests`,
    # which is why the runner's filter holds that suite; these are the ones a
    # corpus case can reach as well, because the driver reads every verdict
    # rather than one.
    ("limits", "unreadable_statements_in", "`COPY … FROM stdin` is read as ordinary SQL",
     '''is(0, "COPY")''', "false"),
    ("limits", "unreadable_statements_in", "`IMPORT FOREIGN SCHEMA` needs its last word",
     '''.any(|three| three[0].is("IMPORT") && three[1].is("FOREIGN") && three[2].is("SCHEMA"))''',
     '''.any(|three| three[0].is("IMPORTED") && three[1].is("FOREIGN") && three[2].is("SCHEMA"))'''),
    ("limits", "builds_a_statement_out_of_text", "`EXECUTE` anywhere is dynamic SQL",
     '''        tokens[at].is("EXECUTE")
            && at
                .checked_sub(1)
                .and_then(|before| tokens.get(before))
                .is_none_or(opens_a_statement)''',
     '''        tokens[at].is("EXECUTE")'''),
    ("limits", "the_path_redirects_in", "a signature is a call",
     "&& !tokens[at.saturating_sub(3)..*at]", "&& !tokens[*at..*at]"),

    # -- the positional hatch and the weight -----------------------------------
    ("limits", "the_path_redirects_in", "a row write clears the whole chunk, not its segment",
     "writing_rows.contains(&segment_of(where_it_writes))", "!writing_rows.is_empty()"),
    ("limits", "the_weight_of", "a name and a word are the same kind",
     "Token::Name(_) => 2,", "Token::Name(_) => 1,"),
    ("limits", "the_weight_of", "a token holding nothing has no kind",
     "Token::Punct(_) => 3,", "Token::Punct(_) => 4,"),
    # **The fingerprint's two halves.** A weight of lengths is blind to a
    # same-length rename and, being a sum, to a reversal.
    ("limits", "the_weight_of", "the text is a length, not its bytes",
     "held.wrapping_mul(31).wrapping_add(byte as usize)", "held.wrapping_add(1)"),
    ("limits", "the_weight_of", "the sum is order-blind",
     "sum.wrapping_add(held.wrapping_mul(at.wrapping_add(1)))", "sum.wrapping_add(held)"),

    # -- clauses `limits::tests` decides ----------------------------------------
    # Measured without the `limits::tests` rows that hold them, each of these,
    # planted, walked past the whole suite. That suite decides them, which is
    # why the runner's filter holds it as well as the corpus.
    ("limits", "renamed_table", "the verb is looked for below the name",
     "    let verb = after;\n    if tokens\n        .get(verb)\n        .is_none_or(|token| !token.is(\"RENAME\") && !token.is(\"SET\"))\n    {\n        return None;\n    }",
     "    let verb = after\n        + tokens\n            .get(after..)?\n            .iter()\n            .position(|token| token.is(\"RENAME\") || token.is(\"SET\"))?;"),
    # -- clauses a plant would walk past without these ------------------------
    ("limits", "the_existence_test_after_if", "an `IF` is always an existence test",
     "        [exists, ..] if exists.is(\"EXISTS\") => 1,\n        _ => 0,",
     "        [exists, ..] if exists.is(\"EXISTS\") => 1,\n        _ => 2,"),
    ("limits", "changes_in_as", "the table walk's fingerprint is taken before the loop",
     "        changes.tokens = changes\n            .tokens\n            .wrapping_add(the_weight_of(&statement.tokens));",
     ""),
    ("limits", "cut", "a fifth mark is a token",
     "        if here == '.' || here == '(' || here == ')' || here == ',' {",
     "        if here == '.' || here == '(' || here == ')' || here == ',' || here == '*' {"),
    ("limits", "renamed_table", "the schema a table moves to is spelled as written",
     "        let schema = tokens.get(verb + 2)?.resolved()?;",
     "        let schema = tokens.get(verb + 2)?.name()?.to_owned();"),
    ("limits", "renamed_table", "the moved table is the tail of a split on a dot",
     "        let bare = parts.last()?.clone();",
     "        let bare = from.rsplit('.').next().unwrap_or(&from).to_owned();"),
    ("limits", "the_qualified_name", "a qualified name ends where it begins",
     "    Some((parts, index))", "    Some((parts, at + 1))"),
    ("limits", "what_one_file_declares", "every create in a file before every drop",
     "            (Some(table), Some(rename)) => table.at <= rename.at,",
     "            (Some(_), Some(_)) => true,"),
    ("limits", "one_directory_declares_its_bounds", "the statement stamp does not cross files",
     "        stamp += changes.statements;", "        stamp = 0;"),
    ("limits", "the_readers_agree", "the two readers of a file may disagree",
     "    if judged == counted {", "    if true {"),
    ("limits", "read_by_hand", "a comment on the cleared statement's line is code",
     '    let source = just_the_words(&crate::gates::without_comments(\n        &lines[from..upto].join("\\n"),\n        "--",\n    ));',
     '    let source = just_the_words(&lines[from..upto].join("\\n"));'),
    ("limits", "cut", "a semicolon is a token",
     "        if here == ';' {\n            flush!();",
     "        if here == ';' {\n            flush!();\n            tokens.push(Token::Punct(';'));"),

    # -- the file that judges the others -------------------------------------
    # Operators over `check.rs`, each anchored to a clause one of this crate's
    # own `check::` rows decides. A repair in this file is not covered by
    # mutation without them, so these are the operators that measure whether
    # the rows do what they say.
    ("check", "is_a_plain_count", "a literal need not be the whole expression",
     "        return written[first.len()..].trim().is_empty()\n            && !first",
     "        return !first"),
    ("check", "is_a_plain_count", "a conversion is a count",
     '        if matches!(first, "from" | "try_from" | "into")\n            || rest_of_the_path.is_some_and(|last| matches!(last, "from" | "try_from" | "into"))\n        {\n            return false;\n        }',
     ""),
    ("check", "the_chain_reads", "a reader may be handed anything",
     '''if !held.is_empty() && !held.starts_with('|') && !held.starts_with('"') && !defaults_to_zero''',
     '''if !held.is_empty() && !held.starts_with('"') && !defaults_to_zero'''),
    ("check", "is_a_plain_expectation", "an expectation may be a number written out",
     "    if a_number_written_out(written) {\n        return false;\n    }",
     ""),
    ("check", "is_a_plain_expectation", "a constant may be clamped against a count",
     "        return !a_written_constant(left)\n            && !a_written_constant(right)\n            && is_a_plain_expectation(left)",
     "        return is_a_plain_expectation(left)"),
    ("check", "the_end_of_the_statement_at", "a binding ends at the first semicolon below it",
     "            ';' if depth <= 0 => return Some(at),",
     "            ';' => return Some(at),"),
    ("check", "the_function_holding", "a name may be bound in another function",
     '        if line.starts_with("fn ")\n            || line.starts_with("pub fn ")\n            || line.starts_with("pub(crate) fn ")\n        {\n            return start;\n        }',
     ""),
    ("check", "the_code_in", "a blanked character is one byte wide",
     "        for _ in 0..original.len_utf8() {",
     "        for _ in 0..1 {"),

    # -- the hatch reader, whose clauses decide whether a gate is switched off --
    ("references", "names", "a reason need not name its subject at a boundary",
     "        boundary(before) && (ends_itself || boundary(after))",
     "        true"),
    # **The left boundary alone, because the right one answers most cases.**
    # Forced to `None` — which is a boundary — the whole suite stays green
    # without the case that reaches it: a case is otherwise refused by what
    # *follows* the subject or accepted by both sides at once. What it decides
    # is a subject sitting at the end of a longer word, which is the count
    # hatch's own headline defect.
    ("references", "names", "what precedes a subject does not decide it",
     "        let before = reason[..at].chars().next_back();",
     "        let before: Option<char> = None;"),
    # **And the clause for a subject that brings its own right boundary.** Its
    # driver is a needle `determinism.rs` really carries, the opening quote and
    # root of an absolute path, followed in the reason by more word.
    ("references", "names", "a subject ending in punctuation needs a boundary after it",
     "        .is_some_and(|c| !c.is_alphanumeric() && c != '_');\n    reason.match_indices",
     "        .is_some_and(|_| false);\n    reason.match_indices"),
    # **Answered, not deleted.** Removing the guard does not weaken this
    # function, it hangs it: `"".find("")` is `Some(0)` and the slice below
    # never advances, so `names(reason, "")` loops for ever. Measured the hard
    # way — a run sat on that mutant for twenty-three hours. The weakening this
    # operator is written for is *a bare subject is named by everything*, and
    # answering `true` says exactly that and returns.
    ("references", "names", "a bare marker excuses everything",
     "    if subject.is_empty() {\n        return false;\n    }",
     "    if subject.is_empty() {\n        return true;\n    }"),
    ("references", "closes_a_char", "an escape closes wherever it likes",
     "        while at < from.len() && at < 12 {",
     "        while at < from.len() {"),
    ("references", "names", "a name with a slash after it is not a directory",
     "        if after == Some('/') {\n            return false;\n        }",
     ""),

    # -- clauses each measured in the suite its file is run against --
    # -- the crate sources with clauses no other operator reaches, one clause each --
    ("compiler_problem", "line_of", "a comment holding the needle is where it is declared",
     "        if line.trim_start().starts_with('#') {",
     "        if false {"),
    ("compiler_messages", "params", "a placeholder nothing supplies is fine",
     "        if !unused.is_empty() {",
     "        if false {"),
    ("pack_cases", "context_of", "a context key nothing consumed is fine",
     "        if context.remove(key).is_none() {",
     "        if false {"),
    ("pack_value", "table_to_cedar", "a one-key table is not a call",
     "    if table.len() == 1 {",
     "    if false {"),
    ("log_root", "line", "the field a log owns may be named by an event",
     '        fields.insert("_msg".to_owned(), nc_canon::Value::text(event.name()));',
     '        fields.insert("_message".to_owned(), nc_canon::Value::text(event.name()));'),
    # **The scale table, which is written twice.**
    # `xtask/src/store.rs`'s `bytes` and the copy in
    # `xtask/tests/the_stack_enforces_the_declared_bounds.rs` are the same four
    # rows, and `xtask/src/check.rs`'s exclusion table excuses the copy on the
    # ground that "the reader it shares with `store.rs` is mutated there". This
    # is the operator that makes that true. Measured — the weakening below,
    # alone, gives `cargo test -q -p xtask --bin xtask -- store::` a failure.
    ("store", "bytes", "a kibibyte is a thousand bytes",
     '("KiB", 1024),',
     '("KiB", 1000),'),
    ("compiler_model", "written", "a variant is spelled however it likes",
     'Self::Reserved => "not yet — the number is held"',
     'Self::Reserved => "yes"'),

    # -- two xtask sources no other operator edits --
    ("generator", "what_a_run_would_write", "a file already on disk is the one to write",
     "        .filter(|(path, contents)| !already_written(&root.join(path), contents))",
     "        .filter(|(path, contents)| already_written(&root.join(path), contents))"),
    ("members", "from_metadata", "a target this member does not build is one of its roots",
     "            .filter_map(|(relative, documented)| documented.then_some(relative))",
     "            .map(|(relative, _)| relative)"),

    ("main", "main", "a verb is handed something other than its own flag",
     "        Command::Build { reserve, .. } => build::run(&root, reserve),",
     "        Command::Build { .. } => build::run(&root, false),"),
    ("store", "the_answer_read", "an answer the store refused is still an answer",
     "    if !served(answer) {",
     "    if false {"),
    ("store", "the_answer_read", "the headers are part of the body",
     '    let Some((_, body)) = answer.split_once("\\r\\n\\r\\n") else {',
     '    let Some((_, body)) = answer.split_once("") else {'),
    ("store", "is_empty", "a store answering nothing is not empty",
     "        self.0.is_empty()",
     "        self.0.len() > 900"),
    ("wall", "promotes_to_static", "a binding behind a visibility is not a binding",
     "    let held = crate::gates::without_a_visibility(code.trim_start());",
     "    let held = code.trim_start();"),
    ("counts", "claim", "the runs emphasis leaves are not collapsed",
     '    let lowered = lowered.split_whitespace().collect::<Vec<_>>().join(" ");',
     "    let lowered = lowered;"),
    ("check", "the_unopinioned", "the flags a store answered are not read",
     "    held.names()\n        .filter(|name| {",
     "    held.names()\n        .take(0)\n        .filter(|name| {"),
    ("check", "the_running_log_store_is_bounded_as_declared",
     "the log store is asked on this checkout's port",
     "    let port = crate::dev::port(root, crate::dev::Port::Logs);",
     '    let port = crate::dev::setting(("LOGS_PORT", "9428"));'),

    # -- repaired clauses, each with an operator --
    # A repair is the most likely source of the next defect, so a repaired
    # clause is the last one that should go uncovered.
    ("dev", "names_its_project", "a project name pinned once is pinned everywhere",
     "        pinned > 0 && pinned == bindings",
     "        pinned > 0"),
    ("workflow", "a_command_this_gate_requires", "only a cargo command is a verdict",
     "    !cargo_commands(code).is_empty()\n        || (the_workflow",
     "    !cargo_commands(code).is_empty()\n        || (false"),
    ("workflow", "wraps_a_command", "a word inside a message closes a construct",
     "        let bare = without_quoted_spans(code);\n        let mut opened = 0_usize;",
     "        let bare = code.to_owned();\n        let mut opened = 0_usize;"),
    ("ui", "a_method_opens_at", "a method is written one way",
     '            let mut head = crate::gates::without_a_visibility(body);\n            loop {\n                let shorter = ["default ", "const ", "async ", "unsafe "]',
     '            let mut head = crate::gates::without_a_visibility(body);\n            loop {\n                let shorter = ["async "]'),
    ("ui", "a_method_opens_at", "a method behind a visibility is not one",
     "        while let Some(newline) = code[at..].find('\\n') {",
     "        while let Some(newline) = code[at..].find('\\u{0}') {"),

    # -- where a pack refusal says it is --
    ("problem", "line_of", "a comment quoting a policy is where it is declared",
     "        .position(|line| !a_comment(line) && line.contains(needle))",
     "        .position(|line| line.contains(needle))"),
    ("problem", "a_comment", "a policy comment is not a comment",
     '    opens.starts_with(\'#\') || opens.starts_with("//")',
     "    opens.starts_with('#')"),

    # -- the reason half of every hatch --
    # `marked_in` decides whether a marker is in a comment and is driven; the
    # function that reads the *reason* after it is reached by nothing else in
    # this file, and all three of its clauses survive being deleted without
    # these. Each is a measured defect that a paragraph alone would have to
    # remember.
    ("references", "reason_after", "the last marker anywhere supplies the reason",
     "    let at = *line\n        .match_indices(marker)\n        .filter(|(at, _)| in_a_comment(line, *at, marker.len(), comments))\n",
     "    let at = *line\n        .match_indices(marker)\n"),
    ("references", "reason_after", "a reason runs past the document comment it is in",
     '    let cut = after.split("-->").next().unwrap_or_default();',
     "    let cut = after;"),
    ("references", "reason_after", "a reason runs past the block comment it is in",
     '    Some(cut.split("*/").next().unwrap_or_default())',
     "    Some(cut)"),

    # **The clause that decides whether the gate reads a paragraph at all.**
    # A line with an odd backtick count opens a span that is not there, and
    # every citation below it is read one backtick out of step — measured, some
    # are never read at all.
    # **Where a paragraph ends, which decides how far a phantom span runs.**
    # A break in a comment block is `    //`, so a reset asking whether the line
    # is empty leaves real citations behind it read by nothing, measured.
    ("references", "ends_a_paragraph", "a comment marker is not content",
     "        .or_else(|| trimmed.strip_prefix(\"//\"))\n",
     ""),
    ("references", "backticked", "a run of two or more backticks is literal",
     "        if run != 1 {",
     "        if run == 0 {"),

    # -- the section reader, which no checkout ever runs ----------------------
    ("sections", "headings", "a subsection heading is not a heading",
     '.or_else(|| line.strip_prefix("### "))',
     '.or_else(|| None)'),
    ("sections", "headings", "a trailing dot is part of the number",
     "let token = token.trim_end_matches('.');",
     "let token = token;"),
    # Anchored to its own loop header, because `headings` holds the same three
    # lines and an operator that matches twice is one this runner calls
    # `not applicable`.
    ("sections", "numbered_items_under", "a fence holds items like any other line",
     'for line in text[at + heading.len()..].lines() {',
     'for line in text[at + heading.len()..].lines().filter(|line| !line.trim_start().starts_with("```")) {'),
    ("sections", "headings", "a fence holds headings like any other line",
     'for line in text.lines() {',
     'for line in text.lines().filter(|line| !line.trim_start().starts_with("```")) {'),
    ("sections", "numbered_items_under", "emphasis is part of the number",
     "let line = line.trim_start_matches(['*', '_', ' ', '>']);",
     "let line = line;"),

    # -- the CI reader --------------------------------------------------------
    ("workflow", "opens_a_key", "a key may be followed by anything",
     "&& (rest.is_empty() || rest.starts_with(' '))",
     '&& true'),
    ("workflow", "opens_a_key", "an empty key opens one",
     '    !key.is_empty()',
     '    true'),
    ("workflow", "reaches_rustc", "a prefixed flag does not reach the compiler",
     '    ) || (name.starts_with("CARGO_")\n        && (name.ends_with("RUSTFLAGS")',
     '    ) || (false\n        && (name.ends_with("RUSTFLAGS")'),
    ("workflow", "reaches_rustc", "a wrapper does not reach the compiler",
     '"RUSTFLAGS" | "RUSTDOCFLAGS" | "RUSTC" | "RUSTC_WRAPPER" | "RUSTC_WORKSPACE_WRAPPER"',
     '"RUSTFLAGS" | "RUSTDOCFLAGS" | "RUSTC"'),

    # -- the workflows' bounds: what `.github/` holds, the authority each
    # workflow opens with, the pins, and how the release is made. Each is
    # decided by a case in `workflow::tests` that names the sentence it expects.
    ("workflow", "github_holds_the_workflows", "a file of no workflow may sit under .github",
     "if !THE_WORKFLOWS.contains(&file.as_str()) {",
     "if false {"),
    ("workflow", "github_holds_the_workflows", "the release workflow may be missing",
     "if !held.iter().any(|file| file == RELEASE_WORKFLOW) {",
     "if false {"),
    ("workflow", "no_authority_but_reading", "a workflow need not open with no permission",
     "    if !opened {",
     "    if false {"),
    ("workflow", "no_authority_but_reading", "a job may ask its token for anything",
     "if grant != THE_ONE_PERMISSION {",
     "if false {"),
    ("workflow", "the_grants", "a flow mapping grants nothing",
     "        inside.split(',').map(str::to_owned).collect()\n    } else {",
     "        Vec::new()\n    } else {"),
    ("workflow", "nothing_made_elsewhere_starts_it", "a fork's change may start a workflow",
     "if code.contains(event) && !said.contains(&(at, event)) {",
     "if false {"),
    ("workflow", "every_action_is_a_commit", "an action named by a tag is pinned",
     "if !is_a_commit(reference.trim()) {",
     "if false {"),
    ("workflow", "every_action_is_a_commit", "a pin needs no release beside it",
     "} else if release.is_empty() {",
     "} else if false {"),
    ("workflow", "is_a_commit", "a short revision is a commit",
     "    reference.len() == 40",
     "    reference.len() >= 7"),
    ("workflow", "is_a_commit", "any letter is a hex digit",
     "(b'a'..=b'f').contains(&byte)",
     "byte.is_ascii_alphabetic()"),
    ("workflow", "the_release_fires_on_a_channel", "any event fires the release",
     'if event != "push" || !value.is_empty() {',
     "if false {"),
    ("workflow", "the_release_fires_on_a_channel", "a filter beside the branches is allowed",
     'if filter != "branches" {',
     "if false {"),
    ("workflow", "the_release_fires_on_a_channel", "any branches release",
     "    if named != channels {",
     "    if false {"),
    ("workflow", "the_release_is_two_jobs", "a third job may sit beside the gate and the release",
     "if !named.contains(&job.as_str()) || seen.contains(&job.as_str()) {",
     "if seen.contains(&job.as_str()) {"),
    ("workflow", "a_jobs_keys_are_its_own", "the release may enter any environment",
     "if entered.as_deref() != Some(THE_RELEASES_ENVIRONMENT) {",
     "if false {"),
    ("workflow", "a_jobs_keys_are_its_own", "the release may run on any system",
     'if !runner.starts_with("ubuntu-") {',
     "if false {"),
    ("workflow", "a_jobs_keys_are_its_own", "the release may run without the gate",
     "if waits.as_deref() != Some(THE_RELEASE_WAITS_FOR) {",
     "if false {"),
    ("workflow", "no_cache_reaches_the_release", "the half that only restores is no cache",
     'lowered == cache || lowered.starts_with(&format!("{cache}/"))',
     "lowered == cache"),
    ("workflow", "the_job_runs_its_steps", "a step more than the list is no difference",
     "ran.len().max(listed.len())",
     "ran.len().min(listed.len())"),
    ("workflow", "the_job_runs_its_steps", "a step's lines are not held",
     "        if step.runs != written[0] {\n            continue;",
     "        if true {\n            continue;"),
    ("workflow", "the_job_runs_its_steps", "a line more than a step's is no difference",
     "held.len().max(written.len())",
     "held.len().min(written.len())"),
    ("workflow", "every_expression_is_where_the_release_needs_it", "an escape hides an expression",
     'if !read.iter().any(|version| version.contains("${{")) {',
     'if !read[0].contains("${{") {'),
    ("workflow", "every_expression_is_where_the_release_needs_it", "an expression may stand anywhere it is written",
     "written == expression && the_lines_of(lines, steps, place).contains(&at)",
     "written == expression"),
    ("workflow", "the_release_runs_as_written", "the release may evaluate any expression",
     "    every_expression_is_where_the_release_needs_it(&lines, text, &steps, findings);\n",
     ""),
    ("workflow", "the_release_runs_as_written", "the release workflow's top may hold any key",
     "        &lines,\n        &tops,",
     "        &lines,\n        &[],"),
    ("workflow", "a_jobs_keys_are_its_own", "the release job may hold any key",
     "holds_these_keys_alone(lines, &keys, allowed,",
     "holds_these_keys_alone(lines, &[], allowed,"),
    ("workflow", "holds_these_keys_alone", "any key may be held",
     "if !allowed.contains(&key.as_str()) || seen.contains(&key) {",
     "if seen.contains(&key) {"),
    ("workflow", "holds_these_keys_alone", "a key may be held twice",
     "if !allowed.contains(&key.as_str()) || seen.contains(&key) {",
     "if !allowed.contains(&key.as_str()) {"),
    ("workflow", "every_workflow_is_bounded", "no other file is held to the release's authority",
     "    nothing_else_holds_the_releases_authority(root, &mut findings);\n",
     ""),
    ("workflow", "nothing_else_holds_the_releases_authority", "every file is passed over as the release's own",
     "        if relative == RELEASE_WORKFLOW {\n            continue;",
     "        if true {\n            continue;"),
    ("workflow", "github_runs_it", "a composite action is not GitHub's",
     '        || name.eq_ignore_ascii_case("action.yml")\n',
     ""),
    ("workflow", "github_runs_it", "a file under .github is not GitHub's",
     '    yaml && (relative.starts_with(".github/")',
     "    yaml && (false"),
    ("workflow", "the_releases_authority_in", "the token's action may be used anywhere",
     "if github && code.contains(&MINTS_THE_RELEASE_TOKEN.to_ascii_lowercase()) {",
     "if false {"),
    ("workflow", "the_releases_authority_in", "an environment may be entered anywhere",
     "                .any(as_a_key)",
     "                .any(|_| false)"),
    ("workflow", "the_releases_authority_in", "a secret is read only in an expression",
     '!the_ends_of_the_word(&code, "secrets").is_empty() || in_an_expression("secrets")',
     'in_an_expression("secrets")'),
    ("workflow", "the_releases_authority_in", "a secret in a comment is no secret",
     ' || in_an_expression("secrets")',
     ""),
    ("workflow", "the_releases_authority_in", "a step's output may be read anywhere",
     'if github && in_an_expression("steps") {',
     "if false {"),
    ("workflow", "the_releases_authority_in", "an expression in a comment is not read",
     "let expressions = the_expressions_in(&lowered);",
     "let expressions = the_expressions_in(&code);"),
    ("workflow", "the_releases_authority_in", "a publish spelled in parts is no publish",
     "let spent = code.replace(['\"', '\\'', '\\\\'], \"\");",
     "let spent = code.clone();"),
    ("workflow", "the_releases_authority_in", "a publish may be written anywhere",
     'if spent.contains("--publish") {',
     "if false {"),
    ("workflow", "the_releases_authority_in", "the release's gate may be turned off anywhere",
     "if spent.contains(crate::release::AFTER_THE_GATE_JOB) {",
     "if false {"),
    # **The gate job, the release's queue and the auditor's version.** A gate
    # job that reads a secret or a variable runs every build script of the tree
    # beside it; a concurrency block that drops `queue: max` lets the next merge
    # cancel the run that finishes a release; and an install with no exact
    # version runs whatever crates.io holds on the day. Only the test written
    # for each says so.
    ("workflow", "the_gate_holds_none_of_the_releases_authority", "the gate may hold the release's authority",
     "        let mut held = the_releases_authority_in(&read, true);",
     "        let mut held: Vec<String> = Vec::new();"),
    ("workflow", "the_gate_holds_none_of_the_releases_authority", "the gate may read a variable",
     '.any(|expression| !the_ends_of_the_word(expression, "vars").is_empty())',
     ".any(|_| false)"),
    ("workflow", "the_releases_concurrency_is_a_queue", "the release's concurrency may hold anything",
     "    if held != THE_RELEASES_CONCURRENCY {",
     "    if false {"),
    ("workflow", "the_releases_concurrency_is_a_queue", "a line of the concurrency at another depth is the same line",
     'format!("(at another depth) {code}")',
     "code"),
    ("workflow", "every_workflow_is_bounded", "an install is held to no version",
     "    every_install_names_its_version(root, &mut findings);\n",
     ""),
    ("workflow", "every_install_names_its_version", "an install may name no version",
     "                if one_crate_locked_at_an_exact_version(&installed) {",
     "                if true {"),
    ("workflow", "one_crate_locked_at_an_exact_version",
     "a second crate, or another source, beside one at an exact version is installed",
     "matches!(named.as_slice(), [named] if names_an_exact_version(named))",
     "named.iter().any(|named| names_an_exact_version(named))"),
    ("workflow", "one_crate_locked_at_an_exact_version", "an install need not be locked",
     "    locked.len() == 1 && ",
     "    locked.len() <= 1 && "),
    ("workflow", "names_an_exact_version", "two numbers are an exact version",
     "        && numbers.len() == 3",
     "        && numbers.len() >= 2"),
    ("workflow", "names_an_exact_version", "a range is an exact version",
     ".all(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))",
     ".all(|number| !number.is_empty())"),
    ("workflow", "the_lines_a_backslash_joins", "a line a backslash continues is two lines",
     "while joined.ends_with('\\\\') && at + 1 < lines.len() {",
     "while false && at + 1 < lines.len() {"),
    ("workflow", "as_yaml_reads_them", "an escape is read as written",
     "let both = |line: &str| [line.to_owned(), with_the_letters_its_escapes_spell(line)];",
     "let both = |line: &str| [line.to_owned(), line.to_owned()];"),
    ("workflow", "the_release_runs_as_written", "a step of the release may run on a condition",
     "    no_step_runs_on_a_condition(RELEASE_WORKFLOW, text, findings);\n",
     ""),
    ("workflow", "every_workflow_is_bounded", "the release workflow is held to nothing",
     "if file == RELEASE_WORKFLOW {",
     "if false {"),
    ("workflow", "no_authority_but_reading", "an escaped key grants nothing",
     "        for version in &read {",
     "        for version in &read[..1] {"),
    ("workflow", "the_grants", "a quoted grant is not the grant it names",
     "    let unquoted = |text: &str| text.trim().trim_matches(['\"', '\\'']).trim().to_owned();",
     "    let unquoted = |text: &str| text.trim().to_owned();"),
    ("workflow", "every_action_is_a_commit", "an action it cannot read is passed over",
     "                    findings.push(not_read(at));\n                    continue;",
     "                    continue;"),
    ("workflow", "every_action_is_a_commit", "a key across a line break is passed over",
     "            for _ in the_actions_used(&joined) {",
     '            for _ in the_actions_used("") {'),
    ("workflow", "the_releases_authority_in", "an explicit key enters no environment",
     "                || follows_an_explicit_key(&code, *end - \"environment\".len())\n",
     "\n"),
    ("workflow", "every_workflow_is_bounded", "a key may be written any way YAML allows",
     "    every_key_is_written_out(root, &mut findings);\n",
     ""),
    ("workflow", "a_key_may_open_at", "a key opens only where a line does",
     "        if trimmed.is_empty() || trimmed.ends_with(['{', '[', ',']) {",
     "        if trimmed.is_empty() {"),
    ("workflow", "a_key_may_open_at", "an anchor or a tag hides the key after it",
     "        if spaced && let Some(property) = the_property_ending(trimmed) {",
     "        if false && let Some(property) = the_property_ending(trimmed) {"),
    ("workflow", "a_key_may_open_at", "a marker needs no space after it",
     "                if spaced\n                    && (rest.is_empty()",
     "                if spaced || true\n                    && (rest.is_empty()"),
    ("workflow", "the_keys_named", "a quoted key is no key",
     "        let at = start - quote.map_or(0, char::len_utf8);",
     "        let at = start + 0 * quote.map_or(0, char::len_utf8);"),
    ("workflow", "the_keys_named", "a key may open anywhere on a line",
     "        if !a_key_may_open_at(code, at) {\n            continue;\n        }",
     "        if false {\n            continue;\n        }"),
    ("workflow", "the_keys_named", "a word glued to its colon is a key",
     "        if quote.is_none() && !a_colon_that_ends_a_key {",
     "        if false && !a_colon_that_ends_a_key {"),
    ("workflow", "the_entry", "a flow collection runs to the first comma",
     "        Some('{' | '[') => text.char_indices()",
     "        Some('{' | '[') if false => text.char_indices()"),
    ("workflow", "the_entry", "a value in a flow collection runs to the end of the line",
     "        _ if in_flow => text.find([',', '}', ']']),",
     "        _ if false => text.find([',', '}', ']']),"),
    ("workflow", "the_keys_not_written_out", "a key may be written anywhere on a line",
     "        .filter(|&(at, _)| a_key_may_open_at(code, at));",
     "        .filter(|&(at, _)| at < usize::MAX);"),
    ("workflow", "the_keys_not_written_out", "a key after a question mark is written out",
     "            '?' if rest.is_empty() || rest.starts_with(char::is_whitespace) => {",
     "            '?' if false => {"),
    ("workflow", "the_keys_not_written_out", "an alias is no key",
     "                if !name.is_empty()\n                    && (name.ends_with(':')",
     "                if false\n                    && (name.ends_with(':')"),
    ("workflow", "the_keys_not_written_out", "a merge is no key",
     "            '<' if rest\n                .strip_prefix('<')",
     "            '<' if false && rest\n                .strip_prefix('<')"),
    ("workflow", "the_lines_left_open", "a flow collection may run over lines",
     '        if depth > 0 {\n            open.push((at, "a flow collection".to_owned()));',
     '        if false {\n            open.push((at, "a flow collection".to_owned()));'),
    ("workflow", "the_block_scalars", "a block scalar's braces are a flow collection",
     "            if line.trim().is_empty() || indent > parent {",
     "            if false {"),
    ("workflow", "runs_cargo_as_an_action", "an action it cannot read is on the list",
     "                return Some(\"uses: <an action this walk cannot read>\".to_owned());",
     "                return None;"),
    ("workflow", "the_actions_used", "an escape hides an action",
     "    let code = with_the_escapes_resolved(&the_command_without_its_comment(line));",
     "    let code = the_command_without_its_comment(line);"),
    ("workflow", "the_actions_used", "a value off its line is an action",
     "if value.is_empty() || value.starts_with(['|', '>', '&', '!', '*', '{', '[']) {",
     "if false {"),
    ("workflow", "the_actions_used", "a quoted action keeps its quotes",
     "                Some(quote @ ('\"' | '\\'')) => value",
     "                Some(quote @ ('\"' | '\\'')) if false => value"),
    ("workflow", "every_workflow_is_bounded", "a line may end wherever YAML ends one",
     "    every_line_ends_at_a_line_feed(root, &mut findings);\n",
     ""),
    ("workflow", "every_line_ends_at_a_line_feed", "a line holding another break is one line",
     ".filter(|&&(other, _)| line.contains(other))",
     ".filter(|&&(other, _)| false && line.contains(other))"),
    ("workflow", "every_line_ends_at_a_line_feed", "only a file GitHub runs ends its lines",
     "        for (at, line) in text.lines().enumerate() {\n            for (_, named) in THE_OTHER_LINE_BREAKS",
     "        for (at, line) in text.lines().enumerate().filter(|_| github_runs_it(&relative)) {\n            for (_, named) in THE_OTHER_LINE_BREAKS"),
    ("workflow", "as_yaml_reads_them", "the line after a backslash is read joined alone",
     "        .map(|(at, line)| (at, both(line)))",
     "        .filter(|&(at, _)| at == 0 || !text.lines().nth(at - 1).is_some_and(|above| above.ends_with('\\\\')))\n        .map(|(at, line)| (at, both(line)))"),
    ("workflow", "the_lines_a_backslash_joins", "a run of backslashes is joined from its first line alone",
     "        if !line.ends_with('\\\\') {\n            continue;\n        }",
     "        if !line.ends_with('\\\\') || lines[..starts].last().is_some_and(|above| above.ends_with('\\\\')) {\n            continue;\n        }"),
    ("workflow", "nothing_made_elsewhere_starts_it", "an escaped event is read as written",
     ".to_ascii_lowercase());\n        for code in codes {",
     ".to_ascii_lowercase()).take(1);\n        for code in codes {"),
    ("workflow", "nothing_made_elsewhere_starts_it", "an event in capitals is another event",
     ".map(|version| the_code_before_a_comment(version).to_ascii_lowercase());",
     ".map(|version| the_code_before_a_comment(version));"),
    ("workflow", "the_property_ending", "a verbatim tag is read as words",
     "    if let Some(opens) = verbatim {",
     "    if let Some(opens) = verbatim.filter(|_| false) {"),
    ("workflow", "the_property_ending", "a verbatim tag may hold a space",
     "            !text[opens + 2..text.len() - 1].contains(|letter: char| letter.is_whitespace())",
     "            text[opens + 2..text.len() - 1].len() < usize::MAX"),
    ("workflow", "the_property_ending", "a verbatim tag may follow any letter",
     "                && text[..opens].chars().next_back().is_none_or(|letter| {",
     "                || text[..opens].chars().next_back().is_none_or(|letter| {"),
    ("workflow", "the_lines_left_open", "a quoted scalar may run over lines",
     '        if quote.is_some() {\n            open.push((at, "a quoted scalar".to_owned()));',
     '        if false {\n            open.push((at, "a quoted scalar".to_owned()));'),
    ("workflow", "the_line_read_as_yaml", "a quote is always a letter",
     '            (None, \'"\' | \'\\\'\') if a_node_may_open_after(&line[..at], depth) => {',
     '            (None, \'"\' | \'\\\'\') if false => {'),
    ("workflow", "the_line_read_as_yaml", "a quote anywhere opens a scalar",
     '            (None, \'"\' | \'\\\'\') if a_node_may_open_after(&line[..at], depth) => {',
     '            (None, \'"\' | \'\\\'\') if a_node_may_open_after(&line[..at], depth) || true => {'),
    ("workflow", "the_line_read_as_yaml", "a doubled single quote closes the scalar",
     "            (Some('\\''), '\\'') if letters.peek().is_some_and(|&(_, next)| next == '\\'') => {",
     "            (Some('\\''), '\\'') if false => {"),
    ("workflow", "the_line_read_as_yaml", "a backslash's quote closes the scalar",
     '            (Some(\'"\'), \'\\\\\') => {\n                letters.next();\n            }',
     '            (Some(\'"\'), \'\\\\\') if false => {\n                letters.next();\n            }'),
    ("workflow", "the_line_read_as_yaml", "a # inside a quote opens a comment",
     "            (None, '#')\n",
     "            (_, '#')\n"),
    ("workflow", "a_node_may_open_after", "a value's quote is a letter",
     "        || (trimmed.ends_with(':') && (depth > 0 || trimmed.len() < before.len()))",
     "        || false"),
    ("workflow", "a_node_may_open_after", "a value's colon needs no space",
     "(depth > 0 || trimmed.len() < before.len())",
     "(depth < usize::MAX)"),
    ("workflow", "a_node_may_open_after", "a property hides the quote after it",
     "            Some(property) if trimmed.len() < before.len() => {",
     "            Some(property) if false => {"),
    ("workflow", "the_workflow_actually_runs", "a pull request may run on GitHub's default types",
     '        } else if event == "pull_request" && !widened {',
     '        } else if event == "never" && !widened {'),
    ("workflow", "the_workflow_actually_runs", "any list of a pull request's types widens it",
     "            && held.first().map(String::as_str) == Some(THE_PULL_REQUEST_TYPES);",
     '            && held.first().is_some_and(|line| line.starts_with("types:"));'),
    ("workflow", "the_workflow_actually_runs", "a line beside the pull request's types is passed",
     "        if let Some(filter) = held.get(usize::from(widened)) {",
     "        if let Some(filter) = held.get(usize::from(widened) * held.len()) {"),
    ("workflow", "the_workflow_actually_runs", "a push may carry the pull request's types",
     '        let widened = event == "pull_request"',
     '        let widened = !event.is_empty()'),
    ("workflow", "no_step_runs_on_a_condition", "an escaped condition is read as written",
     "            .map(|version| the_code_before_a_comment(version));\n        for keyed in codes",
     "            .map(|version| the_code_before_a_comment(version))\n            .take(1);\n        for keyed in codes"),
    ("workflow", "no_step_runs_on_a_condition", "a condition below its key is none",
     "            let written = if keyed.value.is_empty() {",
     "            let written = if false {"),
    ("workflow", "no_step_runs_on_a_condition", "a condition is read where a line opens on it alone",
     '        for keyed in codes.flat_map(|code| the_keys_named(&code, "if")) {',
     '        for keyed in codes.flat_map(|code| the_keys_named(&code, "if")).filter(|keyed| keyed.at == 0) {'),
    ("workflow", "every_key_is_written_out", "a comment cut too soon is passed",
     "        for at in the_comments_cut_too_soon(&text) {",
     "        for at in the_comments_cut_too_soon(&text).into_iter().take(0) {"),
    ("workflow", "the_comments_cut_too_soon", "a comment cut later is cut too soon",
     "            the_command_without_its_comment(line).len() < code",
     "            the_command_without_its_comment(line).len() != code"),
    ("workflow", "the_comments_cut_too_soon", "no comment is cut too soon",
     "            the_command_without_its_comment(line).len() < code",
     "            the_command_without_its_comment(line).len() < code && false"),
    ("workflow", "the_line_read_as_yaml", "a comment ends no code",
     "                return (depth, quote, at);",
     "                return (depth, quote, line.len());"),
    ("workflow", "with_the_letters_its_escapes_spell", "an escape that spells a # or a space is resolved",
     "        out.push_str(if ends_code { &written } else { &resolved });",
     "        out.push_str(if false { &written } else { &resolved });"),
    ("workflow", "with_the_letters_its_escapes_spell", "no escape is resolved",
     "        out.push_str(if ends_code { &written } else { &resolved });",
     "        out.push_str(if true { &written } else { &resolved });"),
    ("workflow", "no_verdict_is_discarded", "a key that neuters a step is read as a needle alone",
     "    if !github_runs_it(file) {\n        return;\n    }",
     "    if true {\n        return;\n    }"),
    ("workflow", "the_block_scalars", "a block scalar's key stands two columns past its dash",
     "        let entry = past_its_dashes(&code);",
     '        let entry = code.trim_start_matches("- ");'),
    ("workflow", "the_lines_of_yaml", "a block's text is read as YAML",
     "        .filter(|&(at, line)| blocks[at].is_none() && !the_code_before_a_comment(line).is_empty())",
     "        .filter(|&(_, line)| !the_code_before_a_comment(line).is_empty())"),
    ("workflow", "the_block_scalars", "a header on a line of its own opens no block",
     "            block = node.or(pending).map(|parent| (parent, style));",
     "            block = node.map(|parent| (parent, style));"),
    ("workflow", "the_block_scalars", "a header on a line of its own opens at its own column",
     "            block = node.or(pending).map(|parent| (parent, style));",
     "            block = node.or(pending.map(|_| indent)).map(|parent| (parent, style));"),
    ("workflow", "the_block_scalars", "a header alone always opens a block",
     "            block = node.or(pending).map(|parent| (parent, style));",
     "            block = node.or(pending).or(Some(indent)).map(|parent| (parent, style));"),
    ("workflow", "the_block_scalars", "a property hides the header after it",
     "        let rest = past_its_properties(value);",
     "        let rest = value.trim_start();"),
    ("workflow", "the_block_scalars", "a dash's node stands where its entry does",
     "            None => (dashes.rfind('-').map(|at| indent + at), entry),",
     "            None => (dashes.rfind('-').map(|_| indent + dashes.len()), entry),"),
    ("workflow", "the_block_scalars", "a dash that ends its line opens no entry",
     '            Some("") => &entry[1..],',
     '            Some("") if false => &entry[1..],'),
    ("workflow", "the_block_scalars", "a comment forgets the node above it",
     "        styles.push(None);\n        if code.is_empty() {\n            continue;\n        }",
     "        styles.push(None);\n        if code.is_empty() {\n            pending = None;\n            continue;\n        }"),
    ("workflow", "the_block_scalars", "properties alone forget the node above them",
     "        } else if node.is_some() {\n            pending = node;\n        }",
     "        } else {\n            pending = node;\n        }"),
    ("workflow", "the_block_scalars", "a written value's next line is read for its header",
     "            if indent > column {\n                continue;\n            }",
     "            if indent > column && false {\n                continue;\n            }"),
    ("workflow", "the_block_scalars", "a value below its key runs on nowhere",
     "            written = node.or(pending);",
     "            written = node;"),
    ("workflow", "the_value_of_its_key", "a key ends at its first colon",
     "        let ends_a_key = after.is_empty() || after.starts_with(char::is_whitespace);",
     "        let ends_a_key = true;"),
    ("workflow", "the_value_of_its_key", "a colon in a quoted key ends it",
     "        (ends_a_key && depth == 0 && quote.is_none()).then_some(after)",
     "        (ends_a_key && depth == 0).then_some(after)"),
    ("workflow", "the_value_of_its_key", "a colon in a flow collection ends a key",
     "        (ends_a_key && depth == 0 && quote.is_none()).then_some(after)",
     "        (ends_a_key && quote.is_none()).then_some(after)"),
    ("workflow", "past_its_properties", "a verbatim tag before a header is read as words",
     '        let end = if rest.starts_with("!<") {',
     "        let end = if false {"),
    ("workflow", "past_its_properties", "one property hides a header behind a second",
     "    while rest.starts_with(['&', '!']) {",
     "    if rest.starts_with(['&', '!']) {"),
    ("workflow", "joined_continuations", "a literal block's lines are a plain scalar's",
     "        if blocks[index] == Some('|') {",
     "        if false {"),
    ("workflow", "past_its_dashes", "one space follows a dash",
     "        entry = rest.trim_start();\n    }\n    entry",
     "        entry = rest.strip_prefix(' ').unwrap_or(rest);\n    }\n    entry"),
    ("workflow", "past_its_dashes", "a dash under a dash is a letter",
     "    while let Some(rest) = entry\n        .strip_prefix('-')",
     "    if let Some(rest) = entry\n        .strip_prefix('-')"),
    ("workflow", "is_label", "a label opens two columns past its dash",
     "    past_its_dashes(trimmed).starts_with(\"name:\")",
     "    trimmed.strip_prefix(\"- \").unwrap_or(trimmed).starts_with(\"name:\")"),
    ("workflow", "only_runs_the_gate", "the walk skips a label wherever it stands",
     "            if yaml.binary_search(&index).is_ok() && is_label(trimmed) {",
     "            if is_label(trimmed) {"),
    ("workflow", "a_second_count_of", "the second count skips a label wherever it stands",
     "                        .filter(move |piece| !(read_as_yaml && is_label(piece)))",
     "                        .filter(move |piece| !is_label(piece))"),
    ("workflow", "a_second_count_of", "the second count reads every line as YAML",
     "                    let read_as_yaml = yaml.binary_search(&at).is_ok();",
     "                    let read_as_yaml = true;"),
    ("workflow", "no_verdict_is_discarded", "a label discards no verdict wherever it stands",
     "        if yaml.binary_search(&index).is_ok() && is_label(code) {",
     "        if is_label(code) {"),
    ("workflow", "joined_continuations", "the walk reads every line as YAML",
     "        let held = if yaml.binary_search(&index).is_ok() {\n            the_lines_behind_the_escapes(line)",
     "        let held = if true {\n            the_lines_behind_the_escapes(line)"),
    ("workflow", "the_lines_github_reads_as_yaml", "every file is one GitHub runs",
     "    if !github_runs_it(relative) {\n        return Vec::new();\n    }\n    the_lines_of_yaml(text)",
     "    the_lines_of_yaml(text)"),
    ("workflow", "the_lines_github_reads_as_yaml", "a block scalar's text is YAML",
     "    the_lines_of_yaml(text)\n        .into_iter()\n        .map(|(at, _)| at)\n        .collect()",
     "    (0..text.lines().count()).collect()"),
    ("workflow", "every_workflow_is_bounded", "a step may run a file GitHub reads",
     "    nothing_runs_what_github_reads(root, &mut findings);\n",
     ""),
    ("workflow", "nothing_runs_what_github_reads", "a file is the one its spelling names",
     "                let itself = the_file_itself(&named);",
     "                let itself = named.clone();"),
    ("workflow", "nothing_runs_what_github_reads", "a comment runs what it names",
     "                    let code = the_command_without_its_comment(&reading);",
     "                    let code = reading.clone();"),
    ("workflow", "the_readings_of", "a line is read as it is written alone",
     "    if read_as_yaml {\n        readings.extend(",
     "    if false {\n        readings.extend("),
    ("workflow", "reached_from", "a script is named as a line is written alone",
     "            for reading in the_readings_of(trimmed, yaml.binary_search(&at).is_ok()) {",
     "            for reading in the_readings_of(trimmed, false) {"),
    ("workflow", "the_variable_one_statement_assigns", "an append assigns nothing",
     "    let before = before.strip_suffix('+').unwrap_or(before);\n",
     ""),
    ("workflow", "the_variable_one_statement_assigns", "an element assigns nothing",
     "        .and_then(|inside| inside.rfind('[').map(|at| &inside[..at]))",
     "        .and_then(|inside| inside.rfind('[').map(|_| inside))"),
    ("workflow", "the_variable_one_statement_assigns", "an array holds no value",
     "        .chain(&elements)\n",
     ""),
    ("workflow", "the_variable_one_statement_assigns", "an element's key is its value",
     "                        .map_or(element, |(_, value)| value)",
     "                        .map_or(element, |(key, _)| key)"),
    ("workflow", "cargo_commands", "a here-string's word is glued to its marker",
     "        let bare = word.trim_matches(['\"', '\\'', '(', ')', '$', '`', '@', '-', '+', '<']);",
     "        let bare = word.trim_matches(['\"', '\\'', '(', ')', '$', '`', '@', '-', '+']);"),
    ("workflow", "assigns_cargo", "only the first `=` of a line assigns",
     "    code.match_indices('=')\n        .find_map(",
     "    code.match_indices('=')\n        .take(1)\n        .find_map("),
    ("workflow", "the_variable_one_statement_assigns", "a name for cargo opens its line",
     ".is_none_or(|letter| letter.is_whitespace() || A_WORD_OPENS_AFTER.contains(&letter));",
     ".is_none();"),
    ("workflow", "the_variable_one_statement_assigns", "a value runs to the end of its statement",
     "            letter.is_whitespace() || matches!(letter, ';' | '&' | '|' | ')' | '}' | ']' | ',')",
     "            matches!(letter, ';' | '&' | '|')"),
    ("workflow", "the_variable_one_statement_assigns", "a quoted value runs to the end of its statement",
     "        Some(quote @ ('\"' | '\\'' | '`')) => after[1..].find(quote).map(|at| at + 2),",
     "        Some('\"' | '\\'' | '`') => None,"),
    ("workflow", "the_variable_one_statement_assigns", "a substitution ends at its first space",
     "        Some('$') if after.starts_with(\"$(\") => after.find(')').map(|at| at + 1),",
     "        Some('$') if false => after.find(')').map(|at| at + 1),"),
    ("workflow", "reached_from", "a local action is walked where a line opens on it",
     "                .filter(|used| used.starts_with('.'))",
     "                .filter(|used| used.starts_with('.') && line.trim_start().starts_with(\"- uses:\"))"),
    ("workflow", "the_directories_a_file_names", "a working-directory names no directory",
     "        for keyed in the_keys_named(&code, \"working-directory\") {",
     "        for keyed in the_keys_named(&code, \"working-directory\").into_iter().take(0) {"),
    ("workflow", "the_directories_a_file_names", "a directory below its key is no directory",
     "            named.push(past_its_dashes(&code).to_owned());",
     "            named.push(String::new());"),
    ("workflow", "the_directories_a_file_names", "a cd is read where a line opens on it",
     "            if matches!(word.trim_matches(['\"', '\\'']), \"cd\" | \"pushd\")",
     "            if at == 0 && matches!(word.trim_matches(['\"', '\\'']), \"cd\" | \"pushd\")"),
    ("workflow", "the_directories_a_file_names", "the checkout's root is a word of a directory",
     "            .replace(\"${GITHUB_WORKSPACE}\", \".\")\n            .replace(\"$GITHUB_WORKSPACE\", \".\");",
     "            .replace(\"${GITHUB_WORKSPACE}\", \".\")\n            .replace(\"$GITHUB_WORKSPACE\", \"$GITHUB_WORKSPACE\");"),
    ("workflow", "every_workflow_is_bounded", "a space may be any whitespace",
     "    no_letter_reads_as_a_space(root, &mut findings);\n",
     ""),
    ("workflow", "a_space_to_the_readers_alone", "only an ASCII space is a space to the readers",
     "    letter.is_whitespace()\n        && !matches!(letter, ' ' | '\\t' | '\\n')",
     "    letter.is_ascii_whitespace()\n        && !matches!(letter, ' ' | '\\t' | '\\n')"),
    ("workflow", "the_lines_behind_the_escapes", "a double-quoted scalar of one line is read as written",
     "            format!(\"{key}: {resolved}{after}\")\n        }];",
     "            line.to_owned()\n        }];"),
    ("workflow", "the_lines_behind_the_escapes", "what follows a closing quote is a comment",
     "            format!(\"{key}: {resolved}{after}\")\n        }];",
     "            format!(\"{key}: {resolved}\")\n        }];"),
    ("workflow", "the_lines_behind_the_escapes", "what follows a closing quote on a later line is a comment",
     "    if let Some(last) = held.last_mut() {\n        last.push_str(&after);\n    }\n",
     ""),
    ("workflow", "the_branches", "every dash before a branch opens its entry",
     "            .map(|index| past_its_dashes(&the_code_before_a_comment(lines[index])).to_owned())",
     "            .map(|index| the_code_before_a_comment(lines[index]).trim_start_matches('-').to_owned())"),

    # -- the absolute-path rule: a file on this machine in generated output ----
    # Every clause below is one a reader of `determinism.rs` finds by hand, and
    # each is one character wide.
    # `the_absolute_path_predicate_decides_every_clause` decides them.
    ("determinism", "names_an_absolute_path", "a doubled slash is a comment, not a path",
     "        let mut end = at + 1;\n        while characters.get(end) == Some(&'/') {\n            end += 1;\n        }",
     "        let end = at + 1;\n        if characters.get(end) == Some(&'/') {\n            return false;\n        }"),
    ("determinism", "names_an_absolute_path", "a placeholder does not open a path segment",
     "after.is_alphanumeric() || matches!(after, '.' | '_' | '-' | '~' | '{')",
     "after.is_alphanumeric() || matches!(after, '.' | '_' | '-' | '~')"),
    ("determinism", "names_an_absolute_path", "a climb only at the front of the literal",
     "(window[0] == '\"' || window[0] == '/')",
     "window[0] == '\"'"),
    ("determinism", "names_an_absolute_path", "a climb is not bounded by a separator",
     "&& (window[3] == '/' || window[3] == '\"')",
     "&& true"),
    ("determinism", "names_an_absolute_path", "a climb is not read at all",
     "    absolute || climbs\n", "    absolute\n"),

    # -- pinned_by_digest + is_a_build_key: the running stack -----------------
    ("gates", "pinned_by_digest", "sixty-four characters, not at least",
     "&& digest.len() == 64", "&& digest.len() >= 64"),
    ("gates", "pinned_by_digest", "a length at all",
     "&& digest.len() == 64", "&& !digest.is_empty()"),
    ("gates", "pinned_by_digest", "hex, not any letter",
     "character.is_ascii_digit() || ('a'..='f').contains(&character)",
     "character.is_ascii_digit() || ('a'..='z').contains(&character)"),
    ("gates", "pinned_by_digest", "lower case, which is what docker accepts",
     "character.is_ascii_digit() || ('a'..='f').contains(&character)",
     "character.is_ascii_hexdigit()"),
    ("gates", "pinned_by_digest", "a name in front of the digest",
     "!name.is_empty()\n            && digest.len() == 64", "digest.len() == 64"),
    ("gates", "is_a_build_key", "flow style opens a mapping too",
     "line.split(['{', ','])", "line.split(',')"),
    ("gates", "is_a_build_key", "a quoted key is a key",
     """.map(|piece| piece.trim_start().replace(['"', '\\''], ""))""",
     """.map(|piece| piece.trim_start().replace(['\\''], ""))"""),
    # `build:{` is not a clause and carries no operator: `split(['{', ','])`
    # guarantees no piece can hold one, so an operator aimed at it would be a
    # no-op reported `not applicable` — which `main` counts as a failure.
    ("gates", "is_a_build_key", "a bare `build:` key",
     'piece == "build:" || piece.starts_with("build: ")',
     'piece.starts_with("build: ")'),
    ("gates", "COMPOSE_FILES", 'the name list loses "compose.override.yaml"',
     '    "compose.override.yml",\n    "compose.override.yaml",\n]',
     '    "compose.override.yml",\n    "compose.override.yml",\n]'),
    ("gates", "COMPOSE_FILES", 'the name list loses "docker-compose.yaml"',
     '    "docker-compose.yml",\n    "docker-compose.yaml",',
     '    "docker-compose.yml",\n    "docker-compose.yml",'),
    ("gates", "the_image_on", "a quoted key is the same key",
     "key.trim_end().trim_matches(['\"', '\\''])", "key.trim_end()"),
    ("gates", "the_effective_stack", "every profile, not the default projection",
     '.args(["compose", "--profile", "*", "config"])', '.args(["compose", "config"])'),
    ("gates", "the_effective_stack", "the image gate is handed the checkout's ports",
     '        .envs(crate::dev::Port::ALL.map(|which| (which.variable(), crate::dev::port(root, which))))\n'
     '        .current_dir(root)',
     '        .current_dir(root)'),
    ("gates", "the_image_on", "the resolved line is indented",
     "let (key, rest) = line.trim().split_once(':')?;",
     "let (key, rest) = line.split_once(':')?;"),
    ("gates", "the_image_on", "whitespace before the colon is not part of the key",
     'key.trim_end().trim_matches([\'"\', \'\\\'\']) == "image"',
     'key.trim_matches([\'"\', \'\\\'\']) == "image"'),

    # -- every_asserted_statement_is_named: a statement vouched for unread ------
    # `sqlx` 0.9 runs text built at run time only once `AssertSqlSafe` vouches
    # for it, and the compiler takes the wrap on trust; the table is where each
    # file's vouching was read, and each of these lets one kind of unread wrap in.
    ("gates", "every_asserted_statement_is_named", "a value its row does not name is a finding",
     "            if !row.from.contains(&name.as_str()) {",
     "            if false {"),
    ("gates", "every_asserted_statement_is_named", "a wrap added or taken away moves the count",
     "        if calls.len() != row.calls {",
     "        if calls.len() > row.calls + 1 {"),
    ("gates", "every_asserted_statement_is_named", "a statement builder is a finding",
     "    for line in builders_named(text) {",
     "    for line in builders_named(\"\") {"),
    # `copy_in_raw` and `copy_out_raw` take a plain `&str`, so a `COPY` built
    # from values is a statement vouched for with no wrap at all.
    ("gates", "every_asserted_statement_is_named", "a copy built from values is a vouched statement",
     "        .chain(copied_statements(text))",
     "        .chain(copied_statements(\"\"))"),
    # And a pragma set from values, which `sqlx-sqlite` runs as text it vouches
    # for itself: without this, past every gate.
    ("gates", "every_asserted_statement_is_named", "a pragma set from values is a vouched statement",
     "        .chain(pragmas_set(text))",
     "        .chain(pragmas_set(\"\"))"),
    ("gates", "copied_statements", "a copy's statement is its last argument",
     "                .rev()\n                .find(|(from, to)|",
     "                .find(|(from, to)|"),
    ("gates", "copied_statements", "a copy written as a literal names nothing",
     "            if !names.is_empty() {\n                copies.push",
     "            if true {\n                copies.push"),
    # A second `noctua-kernel.wit` elsewhere under `crates/` is a second world,
    # and a row that matches the name alone admits it.
    ("check", "is_the_file_named", "a row holding a path admits that path alone",
     "        must_be == relative\n",
     "        relative.ends_with(name)\n"),
    ("gates", "asserted_files_unread", "a row whose file no walk read is a finding",
     "        .filter(|row| !read.iter().any(|file| file == row.file))",
     "        .filter(|_| false)"),

    # -- the_remedies_in: which verdict each remedy answers -------------------
    ("check", "the_remedies_in", "a return type is not a value",
     '|| before.contains("->")', '|| false'),
    ("check", "the_remedies_in", "a literal past the block is not this block's",
     '*at >= from && *at < until', '*at >= from'),

    # -- ledger::read: an error code cannot be renumbered (criterion 7) -------
    ("ledger", "read", "the checksum is compared to its first four characters",
     "if ledger.meta.checksum != expected {",
     "if !expected.starts_with(ledger.meta.checksum.get(..4).unwrap_or(ledger.meta.checksum.as_str())) {"),
    ("ledger", "read", "one line short is not short",
     "if ledger.reserved.len() < ledger.meta.count {",
     "if ledger.reserved.len() + 1 < ledger.meta.count {"),
    ("ledger", "read", "a ledger need not state its count",
     "if !states_a_count(&text) {", "if false {"),
    ("ledger", "read", "a ledger reserving nothing is a ledger",
     "if ledger.reserved.is_empty() {", "if false {"),
    ("ledger", "read", "an empty checksum vouches for the file",
     "if ledger.meta.checksum.is_empty() {", "if false {"),

    # -- dev: the verb that brings the stack up (criterion 10) --------------
    ("dev", "highest_applied", "the highest applied is the last row read",
     "    applied.iter().copied().max()", "    applied.last().copied()"),
    ("dev", "highest_applied", "the highest applied is the first row read",
     "    applied.iter().copied().max()\n}", "    applied.first().copied()\n}"),

    ("dev", "standing_of", "a service that is starting is healthy",
     '    if health == "healthy" {\n        return Standing::Healthy;\n    }',
     '    if health != "unhealthy" {\n        return Standing::Healthy;\n    }'),
    ("dev", "standing_of", "a container that is not running is judged by its health column",
     '    if state != "running" {', '    if false {'),
    ("dev", "standing_of", "no healthcheck at all is a healthcheck that has not answered yet",
     '    if health.is_empty() {\n        return Standing::NoHealthcheck(service.to_owned());\n    }',
     '    if health.is_empty() {\n        unhealthy.push(format!("{service} is starting"));\n        return Standing::NotYet;\n    }'),

    # The stack's ports: the checkout's pair, moved only by its variable, a
    # port something else holds refused before compose runs, and a port asked
    # of the service that publishes it.
    ("dev", "of", "the log store is on the checkout's second port",
     '            Self::Logs => stack.logs,', '            Self::Logs => stack.postgres,'),
    ("dev", "port_of", "a port's variable moves the port",
     '    as_compose_reads(said).unwrap_or_else(|| {', '    as_compose_reads(said.filter(|_| false)).unwrap_or_else(|| {'),
    ("dev", "as_compose_reads", "a variable set empty says nothing",
     '    said.filter(|said| !said.is_empty())', '    said'),
    ("dev", "free_or_this_stacks", "port zero is not a port",
     '.filter(|number| *number != 0)', '.filter(|_| true)'),
    ("dev", "free_or_this_stacks", "a port something else holds is refused",
     '    let bound = std::net::TcpListener::bind(("127.0.0.1", number)).map(drop);',
     '    let bound = std::net::TcpListener::bind(("127.0.0.1", number)).map(drop).or(Ok(()));'),
    ("dev", "bound_or_this_stacks", "a port is this stack's only where this project publishes it",
     'said.lines().any(|line| line.trim() == this_stacks)', '!said.is_empty()'),
    ("dev", "bound_or_this_stacks", "a port is held only when its bind says it is in use",
     '    if error.kind() == std::io::ErrorKind::AddrInUse {', '    if true {'),
    ("dev", "published_by", "the log store's port is asked of the service that publishes it",
     '            Self::Logs => ("logs", "9428"),', '            Self::Logs => ("postgres", "9428"),'),
    ("dev", "published_by", "a port is asked at the container port its service publishes",
     '            Self::Postgres => ("postgres", "5432"),', '            Self::Postgres => ("postgres", "9428"),'),

    # -- the policy step: criterion 11 -------------------------------------
    ("policy", "the_commands_a_schema_cannot_apply", "a resource its action cannot reach is fine",
     "        if !allowed.contains(&named) {",
     "        if !allowed.contains(&named) && named.is_empty() {"),
    ("policy", "the_type_a_selector_names", "the type is the one the schema names",
     "        .map(|selected| selected.entity_type().to_owned())",
     "        .map(|_| \"Realm\".to_owned())"),
    ("policy", "the_commands_a_schema_cannot_apply", "an undeclared action is judged here too",
     "        if allowed.is_empty() {", "        if false {"),
    ("policy", "the_cases_are_the_written_number", "the written case count is never compared",
     "    if ran != CASES {", "    if ran != CASES && ran == usize::MAX {"),
    ("policy", "assert_the_same_policies_are_pinned", "a policy whose pinned clauses moved is not named",
     "    if !moved.is_empty() {", "    if !moved.is_empty() && moved.is_empty() {"),
    ("policy", "the_two_ways_they_disagree", "the schema may fall behind the reference",
     "        declared.difference(transcribed).collect(),", "        Vec::new(),"),
    ("policy", "the_two_ways_they_disagree", "the schema may invent a declaration",
     "        transcribed.difference(declared).collect(),", "        Vec::new(),"),
    ("policy", "declarations_in", "the two spellings of an optional attribute are two declarations",
     "            optionality_in_one_place(&collapsed).replace(\", }\", \" }\")",
     "            collapsed.replace(\", }\", \" }\")"),

    # -- the journal in PostgreSQL: criterion 3's other half -----------------
    # **The `INSERT`'s own ordering clause, which nothing can kill.** The
    # comment above it says the check and the write are one statement so the
    # answer cannot change underneath it — and measured, widened to `>=` and
    # then removed entirely, it left every `in_postgres` test green against a
    # live PostgreSQL. It is an equivalent
    # mutant *in this deployment*: a sequence behind the tail is refused by the
    # primary key and a sequence ahead by `0010`'s `BEFORE INSERT` trigger, whose
    # SQLSTATE is read back into `OutOfOrder`. Recorded rather than left
    # unwritten, because a clause no operator reaches is indistinguishable from
    # one nobody looked at.
    ("journal_store", "append", "the sequence is the one after the tail",
     "WHERE $1 - 1 = COALESCE((SELECT MAX(sequence) FROM journal_entries), -1) \\",
     "WHERE $1 >= 0 \\"),
    ("journal_store", "from", "a read starts where the store likes",
     "WHERE sequence >= $1 ORDER BY sequence",
     "WHERE sequence >= 0 AND $1 >= 0 ORDER BY sequence"),
    ("journal_store", "from", "a read comes back in whatever order the heap holds",
     "WHERE sequence >= $1 ORDER BY sequence",
     "WHERE sequence >= $1 -- ORDER BY sequence"),
    ("journal_store", "head", "the last sequence is the last row recorded",
     "SELECT sequence, hash FROM journal_entries ORDER BY sequence DESC LIMIT 1",
     "SELECT sequence, hash FROM journal_entries ORDER BY recorded_at DESC LIMIT 1"),
    ("journal_store", "tail", "the tail is the last row recorded",
     'format!("SELECT {COLUMNS} FROM journal_entries ORDER BY sequence DESC LIMIT 1")',
     'format!("SELECT {COLUMNS} FROM journal_entries ORDER BY recorded_at DESC LIMIT 1")'),
    ("journal_store", "why_nothing_landed", "a refusal with nothing wrong is an ordering failure",
     "        Err(Error::Contended)",
     "        Err(Error::OutOfOrder {\n            expected: free,\n            found: entry.sequence,\n        })"),
    ("journal_store", "reason", "the sentence is taken from the last colon",
     "        .split_once(\": \")", "        .rsplit_once(\": \")"),
    ("journal_store", "digest", "a hash of the wrong width is a hash",
     "    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {",
     "    <[u8; 32]>::try_from(&bytes.iter().copied().chain(core::iter::repeat(0)).take(32).collect::<Vec<u8>>()[..]).map_err(|_| {"),
    ("journal_store", "unsigned", "a negative number is read as the number it wraps to",
     "    u64::try_from(value).map_err(|_| {",
     "    u64::try_from(value).or_else(|_| Ok::<u64, ()>(value.unsigned_abs())).map_err(|()| {"),

    # -- the pruner in PostgreSQL: criterion 5's other half -------------------
    ("prune_store", "only_for", "a partition of another table is somebody else's inheritance",
     "               WHEN c.relispartition \\\n                 THEN 'it is a partition of another table, so deleting here empties part \\\n                   of a table this catalog does not name' \\\n",
     ""),
    ("prune_store", "over_deleted", "removing more than the batch is not over-deleting",
     "    (removed > batch).then(|| {", "    (removed > batch + batch).then(|| {"),
    ("prune_store", "identifier", "a name of three parts is two identifiers",
     "    let shaped = parts.len() <= 2", "    let shaped = parts.len() <= 3"),
    ("prune_store", "identifier", "a name may begin with a digit",
     "                && part.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')",
     "                && part.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')"),

    # -- value.rs: what a field becomes on the line (criterion 2) ------------
    ("value", "to_canonical", "a count goes out as a number, and collapses at 2^63",
     "            Self::Unsigned(value) | Self::Millis(value) => nc_canon::Value::text(value.to_string()),",
     "            Self::Unsigned(value) | Self::Millis(value) => nc_canon::Value::Integer(i64::try_from(*value).unwrap_or(-1)),"),
    ("value", "LogValue for Duration", "a duration past what a number holds is no time at all",
     "            u64::try_from(self.as_millis()).unwrap_or(u64::MAX),",
     "            u64::try_from(self.as_millis()).unwrap_or(0),"),
    ("value", "to_canonical", "a signed number goes out as text",
     "            Self::Signed(value) => nc_canon::Value::Integer(*value),",
     "            Self::Signed(value) => nc_canon::Value::text(value.to_string()),"),
    ("value", "to_canonical", "a flag goes out as text",
     "            Self::Bool(value) => nc_canon::Value::Bool(*value),",
     "            Self::Bool(value) => nc_canon::Value::text(value.to_string()),"),
    ("value", "to_canonical", "an identifier goes out blank",
     "            Self::Id(text) => nc_canon::Value::text(text.clone()),",
     "            Self::Id(_) => nc_canon::Value::text(String::new()),"),
    ("value", "LogValue for Variant", "an enum state is a source literal",
     "        Field::Variant(self.0)", "        Field::Static(self.0)"),
    ("value", "LogValue for Snowflake", "a snowflake goes out as a number",
     "        Field::Id(self.to_crockford())", "        Field::Unsigned(0)"),
    ("value", "LogValue for Option", "an absent optional field is a dash",
     '        self.as_ref().map_or(Field::Static(""), LogValue::to_field)',
     '        self.as_ref().map_or(Field::Static("-"), LogValue::to_field)'),
    ("value", "LogValue for u32", "a u32 is a signed number",
     "        Field::Unsigned(u64::from(*self))", "        Field::Signed(i64::from(*self))"),

    # -- ship.rs: the endpoint, which is the one `&str` on this surface -------
    ("ship", "parse", "a control character is a character like any other",
     "        if let Some(bad) = endpoint.chars().find(|character| character.is_control()) {",
     "        if let Some(bad) = None::<char> {"),
    ("ship", "parse", "an address wearing a route's clothes is a route",
     '        const ROUTE: &str = "/-._~;=:";', '        const ROUTE: &str = "/-._~;=:@";'),
    ("ship", "parse", "the longest route is twice the number its paragraph argues for",
     "        if path.len() > LONGEST_PATH {", "        if path.len() > LONGEST_PATH * 2 {"),

    # -- ship.rs: whether the lines are kept, which is a loss either way ------
    ("ship", "peer_closed", "a broken pipe is not a peer that closed",
     "            | std::io::ErrorKind::BrokenPipe\n            | std::io::ErrorKind::NotConnected",
     "            | std::io::ErrorKind::NotConnected"),
    ("ship", "verdict", "a store that is merely silent is a store that has gone",
     '        (true, false) => Delivery::Silent(format!("the store did not answer: {error}")),',
     '        (true, false) => Delivery::Unreachable(format!("the store did not answer: {error}")),'),
    ("ship", "keep_on_stderr", "an unreadable answer is an answer",
     "            Self::Unreachable(_) | Self::Refused(_) | Self::Silent(_) | Self::Unreadable(_)",
     "            Self::Unreachable(_) | Self::Refused(_) | Self::Silent(_)"),

    # -- ship.rs: the numbers a shutdown is bounded by ------------------------
    ("ship", "bounds", "a shutdown waits the full connect budget",
     "    if abandoning {\n        (ABANDONED_ANSWER, ABANDONED_ANSWER)\n    } else {\n        (CONNECT_TIMEOUT, WRITE_TIMEOUT)\n    }",
     "    if abandoning {\n        (CONNECT_TIMEOUT, WRITE_TIMEOUT)\n    } else {\n        (CONNECT_TIMEOUT, WRITE_TIMEOUT)\n    }"),
    ("ship", "BATCH", "a batch is eight lines",
     "pub(crate) const BATCH: usize = 64;", "pub(crate) const BATCH: usize = 8;"),
    ("ship", "GIVE_UP_AFTER", "the store is never given up on",
     "const GIVE_UP_AFTER: usize = 2;", "const GIVE_UP_AFTER: usize = usize::MAX;"),
    ("ship", "to", "a shutdown does not stop the loop",
     "                    if failures >= GIVE_UP_AFTER || abandoning.load(Ordering::Relaxed) {",
     "                    if failures >= GIVE_UP_AFTER {"),
    ("ship", "one_answer", "the status line is read without a bound",
     "        let mut answer = stream.take(STATUS_LIMIT);",
     "        let mut answer = stream.take(u64::MAX);"),
    # -- the two crate roots, each holding a repair --
    ("ids", "OpaqueId", "an empty text is an identifier",
     "        if text.is_empty() || text.len() > OPAQUE_ID_MAX {",
     "        if text.len() > OPAQUE_ID_MAX {"),
    ("ids", "OpaqueId", "the identifier length bound is a bound",
     "        if text.is_empty() || text.len() > OPAQUE_ID_MAX {",
     "        if text.is_empty() || text.len() > OPAQUE_ID_MAX + 1 {"),
    ("ids", "OpaqueId", "a letter outside ASCII is a letter",
     "            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')))",
     "            .find(|c| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')))"),
    ("ids", "OpaqueId", "the punctuation this admits is four marks",
     "            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')))",
     "            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))"),
    ("ids", "Display for OpaqueIdError", "the refusal names the bytes it judged",
     "                \"an identifier of {bytes} bytes is outside 1..={OPAQUE_ID_MAX}\"",
     "                \"an identifier of {} bytes is outside 1..={OPAQUE_ID_MAX}\",\n                bytes / 2"),
    ("ids", "Display for OpaqueIdError", "the identifier bound reads upwards",
     "                \"an identifier of {bytes} bytes is outside 1..={OPAQUE_ID_MAX}\"",
     "                \"an identifier of {bytes} bytes is outside {OPAQUE_ID_MAX}..=1\""),
    ("pruner", "refusal", "the code is at the front of the sentence",
     "        \"[{}] `{table}` could not be {what}: {why}\",",
     "        \"prune: [{}] `{table}` could not be {what}: {why}\","),
    ("pruner", "Failure", "a store that did not answer is not a refusal",
     "        if !self.answered {",
     "        if false {"),
    ("pruner", "Failure", "a wait is not a refusal",
     "        } else if self.waited {",
     "        } else if false {"),
    ("pruner", "Pruned", "the code is read from the front",
     "                    .find(|code| why.starts_with(&format!(\"[{}]\", code.code())))",
     "                    .find(|code| why.contains(&format!(\"[{}]\", code.code())))"),

    # -- the database gates and the policy crate root -------------------------
    ("database", "quoted_identifier", "a quote inside a name is doubled",
     "    format!(\"\\\"{}\\\"\", name.replace('\"', \"\\\"\\\"\"))",
     "    format!(\"\\\"{}\\\"\", name)"),
    ("database", "configured_schema", "a url with no query names the default schema",
     "        return \"public\".to_owned();",
     "        return String::new();"),
    ("database", "configured_schema", "the schema option is percent-decoded",
     "        .map(percent_decoded)",
     "        .map(str::to_owned)"),
    ("database", "default_url", "the default url is on this checkout's port",
     "    let port = crate::dev::port(&crate::repository_root(), crate::dev::Port::Postgres);",
     '    let port = crate::dev::setting(("POSTGRES_PORT", "5432"));'),
    ("pack", "coverage", "an allow case is not a denial test",
     "        if case.decision != Expected::Deny {",
     "        if false {"),
    # The principal flag is met by *some* principal the permit allows. *Every*
    # one is the wrong quantifier: a correct control on another realm refuses
    # both cases it exists for, and the first one alone is wrong the same way.
    ("pack", "the_principal_is_what_refuses_it", "the principal claim needs one permitting control",
     "    let permitted = controls.iter().any(|control| {",
     "    let permitted = controls.iter().all(|control| {"),
    ("pack", "the_principal_is_what_refuses_it", "the principal claim reads every control",
     "    let permitted = controls.iter().any(|control| {",
     "    let permitted = controls.iter().take(1).any(|control| {"),
    ("pack", "guards_a_dereference", "a guard is a single conjunct",
     "    let single = !text.contains(\"&&\");",
     "    let single = true;"),
    # A pack floor's type test in a condition is excused only beside the read it
    # guards; excused everywhere, a floor narrowed to one kind of resource in its
    # `when` prints the policy block byte-identical. And a boundary flag is a
    # claim about a `no_permit_matched` case, refused on any other.
    ("pack", "report_narrowings", "a type test in a floor's condition guards a read",
     "            && (where_ == Narrowing::Scope || guards_a_dereference(&narrowing, &rendered))\n",
     "            && (where_ == Narrowing::Scope || true)\n"),
    ("pack_cases", "judge", "a boundary flag needs a boundary to be about",
     "    if written.no_permit_matched.is_none() && !flags.is_empty() {",
     "    if false {"),
    ("dev", "the_invocations_that_name_no_project", "the inspect exemption is about the argument",
     '            if head.contains("\\"inspect\\"") && by_id {',
     '            if head.contains("\\"inspect\\"") {'),
    ("wall", "bound_as_a_parameter", "an angle group that opens a declaration",
     "            let Some(angles) = rest[head.len()..].trim_start().strip_prefix('<') else {",
     "            let Some(angles) = rest[head.len()..].split_once('<').map(|(_, rest)| rest) else {"),
    ("wall", "bound_as_a_parameter", "the space rustc allows before the group",
     "            let Some(angles) = rest[head.len()..].trim_start().strip_prefix('<') else {",
     "            let Some(angles) = rest[head.len()..].strip_prefix('<') else {"),
    ("limits", "order_by_names_a_column", "a change after the creation, not any change",
     "            .filter(|change| change.table == table.name && change.at >= since)",
     "            .filter(|change| change.table == table.name)"),
    ("limits", "is_the_marker", "the marker opens its line",
     '    let Some(comment) = line.trim_start().strip_prefix("--") else {',
     '    let Some(comment) = line.find("--").map(|at| &line[at + 2..]) else {'),
    ("check", "never_applied", "a file the record does not hold is behind",
     "        if !applied.iter().any(|(at, _, _)| *at == version) {",
     "        if false && !applied.iter().any(|(at, _, _)| *at == version) {"),
    ("journal", "chained_and_contiguous", "the break is where the loop stopped trusting",
     "            let where_it_is = previous.map_or(from, |before| before.sequence + 1);",
     "            let where_it_is = entry.sequence;"),
    # **Named for a function this file declares once.** `append` is written
    # twice — the trait's method and `Journal`'s — so the meta-gate's item slice
    # lands on the first and the clause it claims to cover is not in it.
    ("journal", "readable_record", "the nesting bound is a bound",
     "    if depth > nc_canon::parse::MAX_NESTING {",
     "    if depth > nc_canon::parse::MAX_NESTING + 1 {"),
    ("pack", "references_marked", "the excuse is on the marker's own line",
     "            let named = reason.split_once('\\n').map_or(reason, |(first, _)| first);",
     "            let named = reason;"),

    # -- sources that need an operator of their own ---------------------------
    #
    # **A `SOURCES` entry that carries no operator is a file this script backs
    # up, restores and names in its own summary without ever weakening a line of
    # it**, and with nothing comparing the two lists the omission reads as
    # coverage. `every_source_the_meta_gate_edits_carries_an_operator` compares
    # them.
    ("unit", "record", "a closed unit refuses a record",
     "        if self.closed {\n            return Err(Error::UnitClosed { unit: self.id });\n        }\n        record.work_unit = Some(self.id);",
     "        if false {\n            return Err(Error::UnitClosed { unit: self.id });\n        }\n        record.work_unit = Some(self.id);"),
    ("memory", "append", "an entry arrives in sequence",
     "        if entry.sequence != expected {",
     "        if false {"),

    # -- work units by identifier: the doors the kernel opens and closes through
    # `close_unit` refuses a unit this realm never opened, and each store says
    # *how* a unit closed rather than only that it did — the difference between
    # a step that stands and one that was rolled back.
    ("journal", "close_unit", "a unit this realm never opened is refused before its close is written",
     "        if !self.store.unit_opened(&self.realm, unit).await? {\n            return Err(Error::UnitNeverOpened { unit });",
     "        if false && !self.store.unit_opened(&self.realm, unit).await? {\n            return Err(Error::UnitNeverOpened { unit });"),
    ("memory", "unit_closing", "a closing bracket says which way its unit closed",
     "            .map(|entry| entry.record.result);\n        core::future::ready(Ok(closing))",
     "            .map(|_| crate::Outcome::UnitCommitted);\n        core::future::ready(Ok(closing))"),
    ("journal_store", "unit_closing", "the store reads the outcome of the closing bracket",
     '            "SELECT result FROM journal_entries \\',
     "            \"SELECT 'unit-committed' FROM journal_entries \\"),
    # **Which closing bracket answers, and the two guards a conforming store
    # hides.** Every store refuses a second closing bracket, so the order of
    # the question is decided only by a test that plants one; `close`'s own
    # guard answers what `append` answers anyway; and a close refused as
    # already closed must not keep its label for good. Each has its plant and
    # carries the test that turns it red.
    ("journal_store", "unit_closing", "the first closing bracket by sequence is the one that closed the unit",
     'ORDER BY sequence LIMIT 1",',
     'ORDER BY sequence DESC LIMIT 1",'),
    ("memory", "unit_closing", "the first closing bracket in the history is the one that closed the unit",
     "            .find(|entry| {",
     "            .rfind(|entry| {"),
    ("unit", "close", "a closed unit refuses a second close",
     "        if self.closed {\n            return Err(Error::UnitClosed { unit: self.id });\n        }\n        let entry = self.journal.close_unit(self.id, how).await?;",
     "        if false {\n            return Err(Error::UnitClosed { unit: self.id });\n        }\n        let entry = self.journal.close_unit(self.id, how).await?;"),
    ("journal", "close_unit", "a close refused as already closed drops the label no retry can use",
     "        if matches!(landed, Ok(_) | Err(Error::UnitClosed { .. })) {",
     "        if matches!(landed, Ok(_)) {"),
    # **A store that cannot answer is not an answer.** Each of these, weakened,
    # reads a failure to answer as "no" with every other suite green:
    # `close_unit` reports a store it could not ask as a unit nobody opened, and
    # both ways of asking whether a unit closed answer "not closed" over a table
    # that is not there. Each has its plant; the store over a
    # missing schema in `in_postgres.rs` turns all three red, and `CannotSay` in
    # `unit.rs` the two that are not PostgreSQL's own.
    ("journal", "close_unit", "a store that cannot say whether a unit opened is not a unit that never opened",
     "        if !self.store.unit_opened(&self.realm, unit).await? {",
     "        if self.store.unit_opened(&self.realm, unit).await != Ok(true) {"),
    ("journal_store", "unit_closing", "a store that cannot read is not a unit nobody closed",
     "        .map_err(|error| unavailable(&error))?;\n        Ok(closing.as_deref().and_then(Outcome::parse))",
     "        .unwrap_or_default();\n        Ok(closing.as_deref().and_then(Outcome::parse))"),
    ("journal", "unit_closed", "a store that cannot answer is not a unit that is open",
     "        async move { Ok(closing.await?.is_some()) }",
     "        async move { Ok(closing.await.ok().flatten().is_some()) }"),
    ("emitter", "number", "digits group in threes",
     "        if index > 0 && (digits.len() - index) % 3 == 0 {",
     "        if index > 0 && (digits.len() - index) % 4 == 0 {"),

    # -- schema: the closed subset a command's arguments are declared in ------
    # Each is a refusal the crate's own suite names one test for, and each is
    # what the catalog compiler relies on and the kernel will.
    ("schema", "the_kind_of", "a keyword outside the subset is refused",
     "        .find(|written| !KEYWORDS.contains(&written.as_str()))",
     "        .find(|written| written.is_empty())"),
    ("schema", "the_kind_of", "a keyword belongs to the type it is written on",
     "        if !schema.contains_key(keyword) || owner.is_none_or(|owner| owner == kind) {",
     "        if !schema.contains_key(keyword) || true {"),
    ("schema", "the_kind_of", "the schema depth bound is a bound",
     "    if depth > MAX_SCHEMA_DEPTH {",
     "    if depth > MAX_SCHEMA_DEPTH + 1 {"),
    ("schema", "compile_object", "an object is closed only by saying false",
     "    if schema.get(\"additionalProperties\") != Some(&Value::Bool(false)) {",
     "    if schema.get(\"additionalProperties\") == Some(&Value::Bool(true)) {"),
    ("schema", "compile_object", "required names a property the object declares",
     "            if !index.contains_key(name) {",
     "            if index.is_empty() {"),
    ("schema", "compile_string", "a string's two length bounds are compared",
     "    if let (Some(lower), Some(upper)) = (min_length, max_length)",
     "    if let (Some(lower), Some(upper)) = (min_length, min_length)"),
    ("schema", "check", "a string's length is its characters",
     "            let length = text.chars().count() as u64;",
     "            let length = text.len() as u64;"),
    ("schema", "check", "an integer argument is a 64-bit integer",
     "            let Some(number) = value.as_i64() else {",
     "            let Some(number) = value.as_f64().map(|held| held as i64) else {"),
    ("schema", "check_object", "an argument nobody declared is refused",
     "            note(found, path, ViolationKind::Unexpected(excerpt(key)));",
     "            if key.is_empty() {\n                note(found, path, ViolationKind::Unexpected(excerpt(key)));\n            }"),
    # Decided by a bound in time rather than by a verdict: the list is full
    # either way, and only the walk after it changes.
    ("schema", "check", "the walk stops descending once its list is full",
     "    if found.len() >= MAX_VIOLATIONS {\n        return;\n    }\n    match node {",
     "    match node {"),
    ("schema", "note", "a refusal's list stops at its bound",
     "    if found.len() < MAX_VIOLATIONS {",
     "    if found.len() <= MAX_VIOLATIONS {"),
    ("schema", "visit_map", "a key written twice in a schema is refused",
     "                if object.contains_key(&key) {",
     "                if false {"),
    ("schema", "is_a_cedar_type_name", "a word Cedar reserves is not a type name",
     "            && !CEDAR_RESERVED.contains(&segment)",
     "            && !CEDAR_RESERVED.is_empty()"),
    # The crate's own suite drives two of the nine words, so this one is
    # decided by `ALSO_RUN`'s comparison with Cedar's parser.
    ("schema", "CEDAR_RESERVED", "every word Cedar reserves is on the list",
     '"if", "then", "else"',
     '"if", "if", "else"'),
    # **The characters a reader sees nothing of, in a schema's text.** With the
    # rule in the catalog compiler alone, the judge the kernel asks at
    # `activate` accepts a description spelled in tag characters. Narrowed to
    # the controls, the direction family passes; the exemption dropped, a
    # schema written over several lines is refused; the tag plane cut to the
    # block that spells ASCII, its far end passes.
    ("schema", "compile", "a schema's text holds nothing a reader cannot see",
     ".find(|held| unseen(*held) && !matches!(held, '\\n' | '\\r' | '\\t'))",
     ".find(|held| held.is_control() && !matches!(held, '\\n' | '\\r' | '\\t'))"),
    ("schema", "compile", "JSON's own whitespace is not refused as unseen",
     "unseen(*held) && !matches!(held, '\\n' | '\\r' | '\\t')",
     "unseen(*held)"),
    ("schema", "INVISIBLE_RANGES", "the tag plane is unseen to its last point",
     "(0xE0000, 0xE0FFF)",
     "(0xE0000, 0xE007F)"),
    # **Every place a schema's own words come back, cut short.** Uncut, each of
    # these can repeat a megabyte of name, escape sequences and all, with every
    # suite green.
    ("schema", "visit_map", "a key written twice is repeated cut short",
     "super::excerpt(&key)",
     "&key"),
    ("schema", "the_kind_of", "a `type` outside the five is repeated cut short",
     "SchemaFault::TypeUnsupported(excerpt(named))",
     "SchemaFault::TypeUnsupported(named.clone())"),
    ("schema", "compile_object", "a `required` name is repeated cut short",
     "SchemaFault::RequiredNamesNoProperty(excerpt(name))",
     "SchemaFault::RequiredNamesNoProperty(name.to_owned())"),
    ("schema", "compile_string", "an `x-entity` value is repeated cut short",
     "SchemaFault::EntityNotATypeName(excerpt(named))",
     "SchemaFault::EntityNotATypeName(named.clone())"),
    # And what a name is repeated with: every character the table holds, not
    # the controls alone, or an override in an argument's name turns the rest
    # of the printed line around.
    ("schema", "excerpt", "a name is repeated with nothing a reader cannot see",
     "        if shows_nothing(character) {",
     "        if character.is_control() {"),
    ("schema", "unseen", "a reader sees nothing of a default-ignorable point",
     "    shows_nothing(character)\n        || BLANK_RANGES\n",
     "    shows_nothing(character)\n        || false && BLANK_RANGES\n"),

    # -- selector: what a command acts on ------------------------------------
    ("selector", "resolved", "a selected argument is required",
     "        if !root.requires(name) {",
     "        if root.required.is_empty() {"),
    ("selector", "resolved", "a selected argument is no longer than an id may be",
     "        if max_length.is_none_or(|bound| bound > ENTITY_ID_MAX as u64) {",
     "        if max_length.is_none() {"),
    ("selector", "id_is_well_formed", "an empty id is not an id",
     "    (1..=ENTITY_ID_MAX).contains(&id.len())",
     "    (0..=ENTITY_ID_MAX).contains(&id.len())"),
    ("selector", "id_is_well_formed", "an id holds only the id alphabet",
     "            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))",
     "            .all(|byte| byte.is_ascii_graphic())"),
    ("selector", "resolved", "a name a selector refuses is repeated cut short",
     "SelectorError::NotDeclared(excerpt(name))",
     "SelectorError::NotDeclared(name.to_owned())"),
    ("selector", "written", "a selected name is the grammar's",
     "        && characters.all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_');",
     "        && characters.all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_' || rest == '-');"),
    # **The realm is selected by `realm` alone.** Otherwise an argument whose
    # `x-entity` is `Realm` lets a plugin's `Write` choose which realm the
    # decision point is handed, past a rule written against `realm`.
    ("selector", "resolved", "an argument cannot select the realm",
     "        if entity_type == REALM_TYPE {",
     "        if entity_type.is_empty() {"),
    # And both ways out of the grammar repeat the text cut short.
    ("selector", "written", "a selector without the prefix is repeated cut short",
     "        return Err(SelectorError::Malformed(excerpt(text)));",
     "        return Err(SelectorError::Malformed(text.to_owned()));"),
    ("selector", "written", "a name outside the grammar is repeated cut short",
     "        Err(SelectorError::Malformed(excerpt(text)))\n    }",
     "        Err(SelectorError::Malformed(text.to_owned()))\n    }"),

    # -- descriptor: the judge ------------------------------------------------
    ("descriptor", "judge", "a read that mutates is refused by the judge",
     "        (Reversibility::Read, true) => problems.push(DescriptorProblem::ReadThatMutates),",
     "        (Reversibility::Read, true) => {}"),
    ("descriptor", "judge", "a title is measured in characters",
     "    } else if declared.title.chars().count() > TITLE_MAX {",
     "    } else if declared.title.len() > TITLE_MAX {"),
    ("descriptor", "judge", "any problem refuses the declaration",
     "        (Some(schema), Some(selector)) if problems.is_empty() => Ok(Judged { schema, selector }),",
     "        (Some(schema), Some(selector)) => Ok(Judged { schema, selector }),"),
    ("descriptor", "command_name_is_well_formed", "a command name has a namespace",
     "        && name.split('.').count() >= 2",
     "        && name.split('.').count() >= 1"),
    # The title is the one other field of free text, and a person reads it
    # before running the command; narrowed to the controls, an override passes.
    ("descriptor", "judge", "a title holds nothing a reader cannot see",
     "    } else if let Some(hidden) = declared.title.chars().find(|held| unseen(*held)) {",
     "    } else if let Some(hidden) = declared.title.chars().find(|held| held.is_control()) {"),

    # -- packs: what a policy may be called, and what a realm pins -------------
    # The decision point's prefix is refused where an id begins, and only there.
    ("pack", "refused_id", "an id under the decision point's prefix is refused",
     "    if name.starts_with(RESERVED_PREFIX) {",
     "    if false {"),
    ("pack", "refused_id", "the reserved prefix is refused only where an id begins",
     "    if name.starts_with(RESERVED_PREFIX) {",
     "    if name.contains(RESERVED_PREFIX) {"),
    # And every other id has one shape, because the prefix is compared as bytes:
    # another case, a space in front or another script reads as a sentinel.
    ("pack", "refused_id", "an id of another shape is refused",
     "    if !id_is_well_formed(name) {",
     "    if false {"),
    # And its first word is the pack its file is in: measured with nothing
    # holding that, a family file's rule compiled as `core.…`.
    ("pack", "refused_id", "an id's first word is its file's pack",
     "        && name.split('.').next() != Some(pack)",
     "        && false"),
    ("pack", "pack_of", "a file under the core is the core's",
     "    if relative.starts_with(\"policy/core/\") {",
     "    if false {"),
    ("pack", "id_is_well_formed", "an id is the pack's word and the rule's",
     "    id.split('.').count() >= 2",
     "    id.split('.').count() >= 1"),
    ("pack", "id_is_well_formed", "a word of an id begins with a letter",
     ".is_some_and(|first| first.is_ascii_lowercase())",
     ".is_some_and(|_| true)"),
    ("pack", "id_is_well_formed", "a word of an id is written in ASCII",
     "rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_'",
     "rest.is_lowercase() || rest.is_ascii_digit() || rest == '_'"),
    # ASCII is asked of every clause, not only of the one a test happened to
    # reach: a Cyrillic first letter draws `kernel.`, and U+0663 is a digit to
    # `is_numeric`.
    ("pack", "id_is_well_formed", "the first letter of a word is ASCII",
     ".is_some_and(|first| first.is_ascii_lowercase())",
     ".is_some_and(|first| first.is_lowercase())"),
    ("pack", "id_is_well_formed", "a digit of an id is ASCII",
     "|| rest.is_ascii_digit() ||",
     "|| rest.is_numeric() ||"),
    # One defect, one diagnostic: a set that did not parse is not validated too.
    ("pack", "compile", "validation waits for the policies to parse",
     "    if problems.is_empty() {\n        what_the_validator_says(&parsed, &policies, &mut problems);",
     "    if true {\n        what_the_validator_says(&parsed, &policies, &mut problems);"),
    # Read from the whole tree, every id appears under every prefix, and a
    # decision point is held to the core and every pack at once. Nothing in
    # `nc-policy` calls this: `ALSO_RUN` is what decides it.
    ("pack", "ids_under", "a directory's policies are the ones read from under it",
     "        self.effects_under(under)",
     "        self.effects_under(\"\")"),
    ("pack_cases", "is_a_case_file", "a case file is judged inside the policy directory",
     "    let inside = crate::relative_to(policy, path);",
     "    let inside = path.to_string_lossy();"),
    ("pack_manifest", "versions", "a pack holding policies has a manifest",
     "            if holds_a_policy {",
     "            if false {"),
    ("pack_manifest", "versions", "a manifest sits only beside a policy",
     "        if !holds_a_policy {",
     "        if false {"),
    ("pack_manifest", "versions", "the version read is the version kept",
     "                versions.insert(profile, read.version());",
     "                versions.insert(profile, read.version() + 1);"),
    # A pack holds its policies however deep, for this search as for placement:
    # read one level down, a pack whose rules sit in a subdirectory has no version.
    ("pack_manifest", "versions", "a pack holds a policy however deep",
     "            if file.extension().is_some_and(|kind| kind == \"cedar\") {",
     "            if file.parent() == Some(pack.as_path()) && file.extension().is_some_and(|kind| kind == \"cedar\") {"),
    ("pack_manifest", "pins", "a realm pinning another version is refused",
     "        if pinned != carried {",
     "        if false {"),
    ("pack_manifest", "pin", "a pin is the profile at the version",
     "        format!(\"{profile}@{}\", self.version)",
     "        format!(\"{profile}-{}\", self.version)"),
    ("pack_manifest", "every_toml_is_read", "a toml nothing reads is refused",
     "        if crate::cases::is_a_case_file(policy, path) || is_a_manifest(policy, path) {",
     "        if true {"),
    ("pack_manifest", "is_a_manifest", "a manifest sits directly in its pack's directory",
     "    matches!(parts.as_slice(), [PACKS, _, FILE])",
     "    matches!(parts.as_slice(), [PACKS, ..])"),
    # A pin names the realm's own profile, whether or not the tree carries it:
    # checked only for a carried pack, `office` could pin `family@1`.
    ("pack_manifest", "pins", "a realm pinning another profile's pack is refused",
     "        if !pins_its_own_profile(&pinned, &profile) {",
     "        if false {"),
    ("pack_manifest", "pins_its_own_profile", "a pin's profile is all that comes before the @",
     "        .is_some_and(|version| version.starts_with('@'))",
     "        .is_some_and(|_| true)"),
    # Every policy sits where a decision point compiles it, or it passes every
    # case here and is enforced nowhere.
    ("pack_manifest", "every_policy_is_in_a_pack", "a policy in no pack is refused",
     "        if is_in_a_pack(policy, path) {",
     "        if true {"),
    ("pack_manifest", "is_in_a_pack", "a policy directly under packs is in no pack",
     "    matches!(parts.as_slice(), [CORE, _, ..] | [PACKS, _, _, ..])",
     "    matches!(parts.as_slice(), [CORE, _, ..] | [PACKS, _, ..])"),
    ("pack_manifest", "is_in_a_pack", "a pack holds its policies however deep",
     "    matches!(parts.as_slice(), [CORE, _, ..] | [PACKS, _, _, ..])",
     "    matches!(parts.as_slice(), [CORE, _, ..] | [PACKS, _, _])"),
    ("policy", "the_realms_pin_the_packs_they_select", "a world holding no realm to a carried pack is a finding",
     "    if realms == 0 {",
     "    if false {"),
    # Both numbers the line prints are held: the packs the realms are matched
    # to, and a pin no declared version builds refused rather than counted.
    ("policy", "the_realms_pin_the_packs_they_select", "a pin no declared version builds is a finding",
     "    } else if !unheld.is_empty() {",
     "    } else if false {"),
    ("policy", "the_packs_the_pins_are_held_to", "the packs holding the realms are counted",
     "    (pins.len(), held.len(), unheld)",
     "    (pins.len(), 0_usize, unheld)"),

    # -- authz: every way the decision point denies ----------------------------
    # Cedar skips an erring `forbid` and answers from the rest, so an error is a
    # denial whatever Cedar said.
    ("authz", "decide", "any evaluation error denies, whatever cedar answered",
     "        if !erring.is_empty() {",
     "        if false {"),
    # The half an `Allow` never reaches: held on `Allow` alone, an erring rule
    # on a request Cedar denied anyway is journaled as a forbid or as no permit,
    # under NC-POL-0003 rather than NC-POL-0005.
    ("authz", "decide", "an evaluation error is named even where cedar denied",
     "        if !erring.is_empty() {",
     "        if !erring.is_empty() && response.decision() == Verdict::Allow {"),
    # Without the schema, a principal of a type no action applies to is decided
    # by whatever policies happen to match it.
    ("authz", "request", "a request is validated against the schema",
     "        Request::new(principal, action, resource, context, Some(schema))",
     "        Request::new(principal, action, resource, context, None)"),
    # A principal of another realm asked of Cedar is named by the floors that
    # refuse an outsider, where it is denied as no permit matched.
    ("authz", "decide", "a principal the slice does not hold is denied before cedar is asked",
     "        if !slice.holds(question.principal) {",
     "        if false {"),
    # A resource nobody holds makes the owner permit err, and the denial named
    # for that error says the entity does not exist.
    ("authz", "decide", "a resource the slice does not hold is denied whatever cedar answers",
     "        if !slice.holds(question.resource) {",
     "        if false {"),
    # The existence oracle: a delete without a quorum named its forbid for a
    # held resource and no permit for a missing one.
    ("authz", "absent", "a missing resource is named by the forbids that fired on it",
     "        if forbids.is_empty() {",
     "        if true {"),
    # A permit reading nothing of the resource matches a missing one too, and
    # naming it reports an allow for a denial.
    ("authz", "absent", "only a forbid names the denial of a missing resource",
     "                        .is_some_and(|policy| policy.effect() == Effect::Forbid)",
     "                        .is_some_and(|_| true)"),
    ("authz", "decide", "a denial no forbid decided is named as no permit matched",
     "            Verdict::Deny if reasons.is_empty() => Decision::denied(",
     "            Verdict::Deny if false => Decision::denied("),
    # A realm of another profile pinning this pack names rules it was never
    # reviewed against.
    ("authz", "carries", "a realm's profile is compared as well as its pin",
     "        realm.profile == self.profile && realm.pinned_pack == self.pack",
     "        realm.pinned_pack == self.pack"),
    ("authz", "compile", "text that is not of its kind is unreadable, not invalid",
     "                .any(|problem| UNREADABLE.contains(&problem.check))",
     "                .any(|_| false)"),
    # NC-POL-0002 means Cedar's validator refused the pack, and the ledger will
    # not let it mean more: a reserved `@id` logged under it is a schema failure
    # that never happened.
    ("authz", "compile", "a rule a pack breaks is not a validation failure",
     "                .all(|problem| VALIDATION.contains(&problem.check))",
     "                .all(|_| true)"),
    ("authz", "code", "a broken rule carries its own code",
     "            Self::RuleBroken(_) => ErrorCode::PolicyRuleBroken,",
     "            Self::RuleBroken(_) => ErrorCode::PolicyInvalid,"),
    ("authz", "civil_from_days", "january and february close the shifted year",
     "if month <= 2 { 1 } else { 0 }",
     "if month <= 1 { 1 } else { 0 }"),
    # Each fact the kernel vouches for reaches the context under its own name.
    # Written as a constant, each turns one answer into another, and the quorum
    # fails open: a delete nobody approved is allowed.
    ("authz", "context", "the quorum the kernel minted reaches the rule that reads it",
     "            \"quorum_met\": self.quorum_met,",
     "            \"quorum_met\": true,"),
    ("authz", "context", "the chain depth reaches the rule that bounds it",
     "            \"actor_chain_depth\": i64::from(self.actor_chain_depth),",
     "            \"actor_chain_depth\": 0_i64,"),
    ("authz", "context", "a human's confirmation reaches the rule that asks for one",
     "            \"confirmed_by_human\": self.confirmed_by_human,",
     "            \"confirmed_by_human\": true,"),
    ("authz", "context", "an agent acting reaches the rules that read it",
     "            \"via_agent\": self.via_agent,",
     "            \"via_agent\": false,"),
    # Each carries text from outside — a database's error, the schema's
    # complaint about a world — and printed raw it can drive a terminal.
    ("authz_directory", "fmt", "an unavailable directory's reason is printed escaped",
     "\"the directory did not answer: {why:?}\"",
     "\"the directory did not answer: {why}\""),
    ("authz_directory", "fmt", "a refused world's reason is printed escaped",
     "\"the directory refused what it was given: {why:?}\"",
     "\"the directory refused what it was given: {why}\""),
    ("authz_directory", "slice_now", "a slice holds only what its realm holds",
     "                    .is_some_and(|realm| self.holds_in(each, realm))",
     "                    .is_some_and(|_| true)"),
    # Action entities come only from the schema, so a slice built without it
    # holds no action group, and a `forbid` over one never fires: the permit
    # beside it allows. Without a test of that, rustc's dead-field warning is
    # the one other signal, and this runner reads warnings as passes.
    ("authz_directory", "slice_now", "a slice is built with the schema, so an action group is an ancestor",
     "        Entities::from_entities(entities, Some(&self.schema))",
     "        Entities::from_entities(entities, None)"),
    ("authz_directory", "holds_in", "a realm holds itself",
     "            && (entity == realm || self.world.is_ancestor_of(realm, entity))",
     "            && self.world.is_ancestor_of(realm, entity)"),
    ("authz_directory", "with_grant", "a grant the realm does not hold is refused",
     "        if !held {",
     "        if false {"),
    # As for `principal` below: a grant recorded under an id other than the one
    # it was decided with puts one grant in the journal and another in Cedar.
    ("authz_directory", "with_grant", "a grant is recorded under the id as written",
     "        let id = OpaqueId::new(grant_id)",
     "        let id = OpaqueId::new(grant_id.replace(' ', \"_\"))"),
    # The identifier is held to the rule as the caller wrote it, not as it
    # could be made to pass.
    ("authz_directory", "principal", "a principal is minted under the id as written",
     "        let minted = OpaqueId::new(id).ok()?;\n        self.world.get(&uid(kind.type_name(), id)?)?;",
     "        let minted = OpaqueId::new(id.replace(' ', \"_\")).ok()?;\n        self.world.get(&uid(kind.type_name(), id)?)?;"),
    ("authz_directory", "principal", "a principal is minted only for an entity the world holds",
     "        self.world.get(&uid(kind.type_name(), id)?)?;\n        Some(Principal",
     "        Some(Principal"),
    ("authz_embedded", "carried", "a profile selects only its own pack",
     "    CARRIED.iter().find(|pack| pack.profile == profile)",
     "    CARRIED.iter().find(|_| true)"),

    # -- the kernel: the world, the bindings, the engine, the component reader --
    ("kernel", "activate_for_tests", "the door answers what the plugin declared",
     "        .map_err(|error| format!(\"{error:?}\"))?",
     "        .map_or_else(|error| Err(format!(\"{error:?}\")), |_| Ok(Vec::new()))"),
    ("kernel", "component_for_tests", "the door hands over the component it read",
     "    built::component(name)",
     "    Vec::new()"),
    ("kernel_world", "WORLD_VERSION", "the version is the one the package line declares",
     "pub const WORLD_VERSION: &str = \"0.2.0\";",
     "pub const WORLD_VERSION: &str = \"0.1.0\";"),
    ("kernel_world", "WIT_BLAKE3", "the digest is the world's on disk",
     "pub const WIT_BLAKE3: &str = \"e58f1050",
     "pub const WIT_BLAKE3: &str = \"e58f1051"),
    ("kernel_bindings", "activated", "activate runs on the fuel it is given",
     "        nc_catalog::KERNEL.fuel_per_activate,\n",
     "        0,\n"),
    # **And on the activation's fuel, not a dispatch's.** The one above proves
    # only that some fuel is given: handed four times as much, every suite is
    # green but a test that reads what activate left.
    ("kernel_bindings", "activated", "activate runs on the activation's fuel, not a dispatch's",
     "        nc_catalog::KERNEL.fuel_per_activate,\n",
     "        nc_catalog::KERNEL.fuel_per_dispatch,\n"),
    # A no-op of the same length, so no line-count pin moves: the one probe
    # that imports a function is what tells a host that links the world from a
    # refusal at the link.
    ("kernel_bindings", "linked", "every import of the world is linked to the host",
     "    Plugin::add_to_linker::<HostState<E>, HasSelf<HostState<E>>>(&mut linker, |state| state)?;\n",
     "    linker.allow_shadowing(false);\n"),
    ("kernel_engine", "configuration", "the engine meters fuel",
     "    config.consume_fuel(true);",
     "    config.consume_fuel(false);"),
    ("kernel_engine", "configuration", "the engine interrupts on the epoch",
     "    config.epoch_interruption(true);",
     "    config.epoch_interruption(false);"),
    ("kernel_engine", "functions_in", "only a function is an imported function",
     "        _ => {}",
     "        _ => into.push(path.to_owned()),"),
    # The two shapes WIT does not write and a component can: a function
    # imported by itself, which has no `#` in its path, and an instance inside
    # an instance, which has one. Each is decided by the probe built for it.
    ("kernel_engine", "functions_in", "a function imported by itself is listed",
     "        ComponentItem::ComponentFunc(_) => into.push(path.to_owned()),",
     "        ComponentItem::ComponentFunc(_) if path.contains('#') => into.push(path.to_owned()),"),
    ("kernel_engine", "functions_in", "an instance inside an instance is walked",
     "        ComponentItem::ComponentInstance(instance) => {",
     "        ComponentItem::ComponentInstance(instance) if !path.contains('#') => {"),
    # The other direction: a walk that can find nothing passes the plugin that
    # imports nothing, and only the probe that imports one thing refuses it.
    ("kernel_engine", "imports", "an import walk that finds nothing is refused",
     "        }\n        functions\n    }",
     "        }\n        Vec::new()\n    }"),
    ("kernel_built", "current", "an input changed since the build is stale",
     "                if digest_of(path).as_deref() != Some(*hash) {",
     "                if digest_of(path).is_none() {"),
    ("kernel_built", "current", "a file that appeared under a root is stale",
     "            .find(|file| !listed.contains(&file.as_str()))",
     "            .find(|_| false)"),
    ("kernel_built", "current", "the component is the one the list was written for",
     "    if digest_of(path).as_deref() != Some(hash) {",
     "    if digest_of(path).is_none() {"),
    ("kernel_built", "current", "a list with no directory or no file is refused",
     "    if roots.is_empty() || listed.is_empty() {",
     "    if roots.is_empty() && listed.is_empty() {"),

    # -- the kernel: what a plugin may call, and every host function that asks --
    ("kernel_capability", "check", "a capability not granted is refused",
     "        if self.holds(capability) {",
     "        if true {"),
    ("kernel_capability", "holds", "a capability granted is held",
     "        self.granted.contains(&capability)",
     "        false"),
    ("kernel_capability", "fmt", "a denial opens on its own code",
     "            ErrorCode::PluginCapabilityDenied,\n            spec.interface,",
     "            ErrorCode::PluginManifestInvalid,\n            spec.interface,"),
    ("kernel_capability", "every", "every capability the catalog declares can be granted",
     ".iter().map(|spec| spec.capability))",
     ".iter().skip(1).map(|spec| spec.capability))"),
    ("kernel_limits", "lowered", "a limit raised past the catalog's is refused",
     "if asked == 0 || asked > ceiling =>",
     "if asked == 0 =>"),
    ("kernel_limits", "lowered", "a limit lowered to zero is refused",
     "Some(asked) if asked == 0 ||",
     "Some(asked) if false ||"),
    ("kernel_limits", "lowered", "a limit lowered to its ceiling is accepted",
     "|| asked > ceiling",
     "|| asked >= ceiling"),
    ("kernel_limits", "lowered", "every bound refused is named, not one of them",
     "                refused.push(LimitRefused {",
     "                refused.clear();\n                refused.push(LimitRefused {"),
    ("kernel_limits", "catalog", "the store's quota is the one its table refuses at",
     ".find(|table| table.name == \"store_entries\")\n            .and_then(|table| table.max_rows)\n            .expect(",
     ".find(|table| table.name == \"store_writes\")\n            .and_then(|table| table.max_rows)\n            .expect("),
    ("kernel_manifest", "granted", "a capability the catalog does not declare is refused",
     "            None => problems.push(ManifestProblem::CapabilityUnknown(name.clone())),",
     "            None => {}"),
    ("kernel_manifest", "granted", "a capability asked for twice is refused",
     "            Some(spec) if held.contains(&spec.capability) => {",
     "            Some(spec) if held.is_empty() && held.contains(&spec.capability) => {"),
    ("kernel_manifest", "declared_domains", "a domain outside the grammar is refused",
     "        if !domain_is_well_formed(domain) {",
     "        if domain.is_empty() {"),
    ("kernel_manifest", "domain_is_well_formed", "an address is not a domain",
     "!last.chars().all(|character| character.is_ascii_digit())",
     "!last.is_empty()"),
    ("kernel_manifest", "domain_is_well_formed", "an address spelt in hexadecimal is not a domain",
     ".strip_prefix(\"0x\")",
     ".strip_prefix(\"0X\")"),
    ("kernel_manifest", "domain_is_well_formed", "localhost is not a domain",
     "        && labels.last() != Some(&\"localhost\")",
     "        && !labels.is_empty()"),
    ("kernel_manifest", "domain_is_well_formed", "a wildcard covers no top-level domain",
     "        && labels.len() >= 2",
     "        && !labels.is_empty()"),
    ("kernel_manifest", "declared_domains", "net without domains is refused",
     "            problems.push(ManifestProblem::NetWithoutDomains);\n",
     ""),
    ("kernel_manifest", "declared_domains", "domains without net are refused",
     "            problems.push(ManifestProblem::DomainsWithoutNet);\n",
     ""),
    ("kernel_manifest", "declared_domains", "a domain past the bound is refused",
     "    if domains.len() > DOMAINS_MAX {",
     "    if domains.len() > DOMAINS_MAX + 1 {"),
    ("kernel_manifest", "declared_domains", "a domain declared twice is refused",
     "        } else if domains[..at].contains(domain) {",
     "        } else if domains[..at].is_empty() && domains[..at].contains(domain) {"),
    ("kernel_manifest", "declared_topics", "a topic outside the grammar is refused",
     "        if !topic_pattern_is_well_formed(topic) {",
     "        if topic.is_empty() {"),
    ("kernel_manifest", "declared_topics", "subscribing without topics is refused",
     "            problems.push(ManifestProblem::SubscribingWithoutTopics);\n",
     ""),
    ("kernel_manifest", "declared_topics", "an empty subscription list is refused",
     "(true, Some(events)) if !events.subscribe.is_empty() => events.subscribe,",
     "(true, Some(events)) => events.subscribe,"),
    ("kernel_manifest", "topic_pattern_is_well_formed", "a topic is at least two words",
     "(words.len() >= 2 || topic.ends_with(\".*\"))",
     "(!words.is_empty() || topic.ends_with(\".*\"))"),
    ("kernel_manifest", "parse", "an id outside the grammar is refused",
     "        if !id_is_well_formed(&written.id) {",
     "        if written.id.is_empty() {"),
    ("kernel_manifest", "parse", "a namespace outside the grammar is refused",
     "        if !namespace_is_well_formed(&written.namespace) {",
     "        if written.namespace.is_empty() {"),
    ("kernel_manifest", "parse", "a limit the manifest raises refuses it",
     ".map_err(|refused| problems.extend(refused.into_iter().map(ManifestProblem::Limit)));",
     ".or_else(|_| Ok::<Limits, ()>(Limits::catalog()));"),
    ("kernel_manifest", "parse", "every limit a manifest raises is reported, not the first",
     "refused.into_iter().map(ManifestProblem::Limit)",
     "refused.into_iter().take(1).map(ManifestProblem::Limit)"),
    ("kernel_manifest", "id_is_well_formed", "an id is held to its length",
     "    id.len() <= ID_MAX",
     "    id.len() <= ID_MAX + 1"),
    ("kernel_manifest", "namespace_is_well_formed", "a namespace of exactly the bound is accepted",
     "namespace.len() <= NAMESPACE_MAX",
     "namespace.len() < NAMESPACE_MAX"),
    ("kernel_manifest", "domain_is_well_formed", "a name of exactly 253 bytes is accepted",
     "    name.len() <= 253",
     "    name.len() <= 252"),
    ("kernel_manifest", "Display for ManifestError", "a refused manifest opens on its own code",
     "            ErrorCode::PluginManifestInvalid\n",
     "            ErrorCode::PluginCapabilityDenied\n"),
    ("kernel_host", "convert_error", "a refusal reaches the plugin as a value",
     "            KernelError::Refused(refusal) => Ok(refusal),",
     "            KernelError::Refused(refusal) => Err(wasmtime::Error::msg(format!(\"{refusal:?}\"))),"),
    ("kernel_host", "convert_error", "a fault traps rather than reaching the plugin",
     "            KernelError::Trap(fault) => Err(fault),",
     "            KernelError::Trap(fault) => Ok(types::Error::Internal(fault.to_string())),"),
    ("kernel_host", "require", "a function reached is recorded",
     "        self.reached.insert(Linked {\n            interface: capability.spec().interface,",
     "        drop(Linked {\n            interface: capability.spec().interface,"),
    ("kernel_host", "require", "a function is recorded under the capability it asked for",
     "            capability: Some(capability),",
     "            capability: Some(Capability::Store),"),
    ("kernel_host", "require", "a denial is recorded",
     "                None => self.denials.push((denial, 1)),",
     "                None => {}"),
    ("kernel_host", "require", "a function refused is kept once, not once per call",
     ".find(|(seen, _)| *seen == denial)",
     ".find(|_| false)"),
    ("kernel_host", "require", "every refusal is counted",
     "                Some((_, times)) => *times = times.saturating_add(1),",
     "                Some(_) => {}"),
    ("kernel_host", "release", "a handle is released under its own capability",
     ".check(capability, \"[resource-drop]\")",
     ".check(Capability::Store, \"[resource-drop]\")"),
    ("kernel_host", "ungated", "an ungated function is recorded with no capability",
     "            interface,\n            function,\n            capability: None,",
     "            interface,\n            function,\n            capability: Some(Capability::Store),"),
    ("kernel_host_log", "write", "a write is recorded as reached",
     "        self.ungated(\"log\", \"write\");\n",
     ""),
    ("kernel_host_log", "write", "a write reaches the ring",
     "        crate::dispatch::lock(&self.ring).push(kept_level(lvl), &msg);\n",
     ""),
    ("kernel_host_log", "push", "a full ring lets its oldest line go",
     "        while self.lines.len() >= lines {",
     "        while self.lines.len() > lines {"),
    ("kernel_host_log", "push", "a character a reader sees nothing of is replaced",
     "            let shown = if nc_schema::unseen(character) {",
     "            let shown = if character == '\\u{0}' {"),
    ("kernel_host_log", "push", "a line is cut at its bound and not before",
     "            if kept.len() + shown.len_utf8() > bound {",
     "            if kept.len() + shown.len_utf8() >= bound {"),
    ("kernel_host_log", "kept_level", "each level of the world is kept as its own",
     "        world::Level::Warn => LogLevel::Warn,",
     "        world::Level::Warn => LogLevel::Info,"),
    ("kernel_host_context", "principal", "the principal asks for its capability",
     "        self.require(Capability::Context, \"principal\")?;\n",
     ""),
    ("kernel_host_context", "actor_chain", "the actor chain asks for the context capability",
     "        self.require(Capability::Context, \"actor-chain\")?;",
     "        self.require(Capability::Store, \"actor-chain\")?;"),
    ("kernel_host_store", "get", "a read of the store asks for its capability",
     "        self.require(Capability::Store, \"get\")?;\n",
     ""),
    ("kernel_host_store", "put", "a write asks for the store capability",
     "        self.require(Capability::Store, \"put\")?;",
     "        self.require(Capability::Net, \"put\")?;"),
    ("kernel_host_blobs", "open", "opening a blob asks for its capability",
     "        self.require(Capability::BlobsRead, \"open\")?;\n",
     ""),
    ("kernel_host_blobs", "create", "creating a blob asks for blobs.write",
     "        self.require(Capability::BlobsWrite, \"create\")?;",
     "        self.require(Capability::BlobsRead, \"create\")?;"),
    ("kernel_host_blobs", "size", "a reader's size asks for its capability",
     "        self.require_or_trap(Capability::BlobsRead, \"[method]reader.size\")?;\n",
     ""),
    ("kernel_host_blobs", "seek", "a seek is recorded by the name the world gives it",
     "\"[method]reader.seek\")?;",
     "\"[method]reader.sek\")?;"),
    ("kernel_host_blobs", "HostReader for HostState", "a reader is released under blobs.read",
     "        self.release(Capability::BlobsRead)?;\n",
     ""),
    ("kernel_host_commands", "dispatch", "a dispatch asks for its capability",
     "        self.require(Capability::CommandsDispatch, \"dispatch\")?;\n",
     ""),
    ("kernel_host_commands", "begin", "a unit is begun under commands.begin",
     "        self.require(Capability::CommandsBegin, \"begin\")?;",
     "        self.require(Capability::CommandsDispatch, \"begin\")?;"),
    ("kernel_host_commands", "commit", "a unit is committed under commands.begin",
     "        self.require(Capability::CommandsBegin, \"[method]unit.commit\")?;",
     "        self.require(Capability::CommandsDispatch, \"[method]unit.commit\")?;"),
    ("kernel_host_commands", "abort", "an abort asks for its capability",
     "        self.require_or_trap(Capability::CommandsBegin, \"[method]unit.abort\")?;\n",
     ""),
    ("kernel_host_events", "publish", "publishing asks for events.publish",
     "        self.require(Capability::EventsPublish, \"publish\")?;",
     "        self.require(Capability::EventsSubscribe, \"publish\")?;"),
    ("kernel_host_ui", "render", "rendering asks for its capability",
     "        self.require(Capability::Ui, \"render\")?;\n",
     ""),
    ("kernel_host_net", "fetch", "a fetch asks for net",
     "        self.require(Capability::Net, \"fetch\")?;\n",
     ""),
    ("kernel_host_timers", "cancel", "cancelling a timer asks for its capability",
     "        self.require(Capability::Timers, \"cancel\")?;\n",
     ""),
    ("kernel_bindings", "execute", "a command runs holding the grants it was given",
     "        HostState::in_memory(grants.clone()),\n",
     "        HostState::in_memory(crate::capability::GrantSet::every()),\n"),
    ("kernel", "vacant", "a namespace the kernel keeps is refused at load",
     "        let taken = if registry::RESERVED.contains(&namespace) {",
     "        let taken = if false {"),
    ("kernel", "vacant", "an identity loaded already is taken",
     "held.clone().any(|loaded| loaded.manifest.id() == plugin)",
     "held.clone().any(|_| false)"),
    ("kernel", "vacant", "a namespace another loaded plugin holds is taken",
     "held.find(|loaded| loaded.manifest.namespace() == namespace)",
     "held.find(|_| false)"),
    ("kernel", "vacant", "an identity or a namespace taken is refused with the plugin's code",
     "            ErrorCode::PluginIdentityTaken,\n",
     "            ErrorCode::KernelNameTaken,\n"),
    # The state a plugin is loaded holding is built by `HostState::for_plugin`,
    # which its restarts build theirs with as well.
    ("kernel_host", "for_plugin", "a plugin activates holding its manifest's grants",
     "            manifest.grants().clone(),\n",
     "            GrantSet::none(),\n"),
    # **And the other way.** The one above narrows the grants and is caught by
    # every plugin that reads its store; this widens them, and a load granting
    # every plugin everything leaves every suite green but the test of a
    # plugin not granted its store, loaded over a seed it would have read.
    ("kernel_host", "for_plugin", "a plugin is granted no more than its manifest names",
     "            manifest.grants().clone(),\n",
     "            GrantSet::every(),\n"),
    ("kernel_host", "for_plugin", "a plugin's memory is held to its manifest's bound",
     "            MemoryGuard::new(manifest.limits()),\n",
     "            MemoryGuard::new(&crate::limits::Limits::catalog()),\n"),
    ("kernel", "load", "a load asks again once it has activated",
     "        plugins.vacant(&manifest).map_err(LoadError::raced)?;\n",
     ""),
    ("kernel", "load", "a plugin's own refusal is repeated cut to its bound",
     "registry::quoted_up_to(&why, REFUSAL_QUOTED)",
     "registry::quoted_up_to(&why, usize::MAX)"),
    # What wasmtime says of bytes it refuses quotes the component's own names,
    # which are cut short: repeated whole, an imported function named with
    # 100,000 characters comes back in full.
    ("kernel", "load", "what wasmtime says of bytes it will not compile is cut to its bound",
     "registry::quoted_up_to(&refused.to_string(), REFUSAL_QUOTED)",
     "registry::quoted_up_to(&refused.to_string(), usize::MAX)"),
    ("kernel", "load", "what wasmtime says of a component that does not link is cut to its bound",
     "registry::quoted_up_to(&format!(\"{refused:#}\"), REFUSAL_QUOTED)",
     "registry::quoted_up_to(&format!(\"{refused:#}\"), usize::MAX)"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a page holds no more keys than its limit",
     "        keys.truncate(limit);",
     "        keys.truncate(limit + 1);"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a limit of zero is a page of one in memory too",
     "        let limit = usize::try_from(limit.max(1)).unwrap_or(usize::MAX);",
     "        let limit = usize::try_from(limit).unwrap_or(usize::MAX);"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a person's units are listed newest first",
     ".held()\n            .units\n            .values()\n            .rev()",
     ".held()\n            .units\n            .values()\n            .skip(0)"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a person's newest units are the ones kept",
     "            .skip(usize::try_from(keep).unwrap_or(usize::MAX))",
     "            .skip(usize::MAX)"),
    # **The realm half of each filter.** With one realm in every script and
    # every sequence, both are realm-blind and green; the suite names a second.
    ("kernel_backend", "StoreBackend for MemoryBackend", "a person's units are listed within their realm",
     "detail.row.meta.realm == *realm && detail.row.meta.principal == *principal\n            })\n            .take(",
     "!realm.as_str().is_empty() && detail.row.meta.principal == *principal\n            })\n            .take("),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a person's units are forgotten within their realm",
     "detail.row.meta.realm == *realm && detail.row.meta.principal == *principal\n            })\n            .skip(",
     "!realm.as_str().is_empty() && detail.row.meta.principal == *principal\n            })\n            .skip("),
    ("kernel_backend", "StoreBackend for MemoryBackend", "unpinning at a time the file cannot hold is refused",
     "            as_signed(at_ms, \"a time\").map(|_| Some(at_ms))",
     "            Ok::<_, LocalError>(Some(at_ms))"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "the sweep lists only what was unpinned before its time",
     "                        .filter(|at| *at < before_ms)",
     "                        .filter(|at| *at <= before_ms)"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a pinned blob is not forgotten",
     ".is_some_and(|row| !row.pinned);",
     ".is_some();"),
    ("kernel_backend", "committed", "a key holding other than what was expected is a conflict",
     "        if !holds_what_was_expected(before.as_ref(), &write.expect) {",
     "        if false {"),
    ("kernel_backend", "committed", "a plugin is held to its own quota",
     "> quotas.entries_for(plugin) {",
     "> quotas.entries_per_plugin {"),
    # **Only a new key counts against a quota.** A script holding a plugin to
    # one number from its first commit never puts it over, so against such
    # scripts alone either store counting a rewrite as a new key leaves every
    # suite green — measured, both planted — which is why a script here lowers
    # the number.
    ("kernel_backend", "committed", "a key a plugin holds is not a new key in memory",
     "        if before.is_none() && after.is_some() && !grew.contains(&write.plugin.as_str()) {",
     "        if after.is_some() && !grew.contains(&write.plugin.as_str()) {"),
    ("kernel_backend", "pinned", "a new blob past the machine's bound is refused",
     ">= quotas.blobs {",
     "> quotas.blobs {"),
    ("kernel_backend", "pinned", "a held blob is pinned again",
     "            row.pinned = true;",
     "            row.pinned = row.pinned;"),
    ("kernel_backend", "committed", "a mark sets its unit's state",
     "        detail.row.state = *state;",
     "        detail.row.state = UnitState::Committed;"),
    ("kernel_backend", "within_what_one_unit_may_record", "more images or notes than a unit records is refused",
     "        if u64::try_from(held).unwrap_or(u64::MAX) > bound {",
     "        if false {"),
    ("kernel_backend", "shaped", "a key written twice in one unit is refused",
     "        if seen.contains(&key) {",
     "        if false {"),
    ("kernel_backend", "canonical", "text that is not its own canonical form is refused",
     "        Ok(value) if value.to_canonical() == text => Ok(()),",
     "        Ok(_) => Ok(()),"),
    ("kernel_registry", "admit", "more commands than a plugin may declare is refused",
     "        if commands.len() > most {",
     "        if false {"),
    # **A load refused for its count is told it was not started**, not
    # NC-PLUG-0017, whose message says a part reached the most it may keep —
    # which reads wrong beside the refusal.
    ("kernel_registry", "admit", "a plugin declaring too many commands is told it was not started",
     "                ErrorCode::PluginDescriptorInvalid,\n                format!(\n                    \"`{plugin}` declares {} commands",
     "                ErrorCode::PluginBoundReached,\n                format!(\n                    \"`{plugin}` declares {} commands"),
    ("kernel_registry", "admit", "an empty field is a missing field, not an invalid one",
     "if matches!(problem, DescriptorProblem::Missing(_)) {",
     "if false {"),
    ("kernel_registry", "admit", "a declaration with a problem registers nothing",
     "        if problems.is_empty() {\n            Ok(admitted)",
     "        if true {\n            Ok(admitted)"),
    ("kernel_registry", "name_refused", "a name in a namespace the kernel keeps is taken",
     "        if in_a_reserved_namespace(&command.name) {\n            return Some(format!(",
     "        if false {\n            return Some(format!("),
    ("kernel_registry", "name_refused", "a name is inside its namespace, dot included",
     "        if !command.name.starts_with(&format!(\"{namespace}.\")) {",
     "        if !command.name.starts_with(namespace) {"),
    ("kernel_registry", "name_refused", "a name declared twice is taken",
     "        if earlier.iter().any(|before| before.name == command.name) {",
     "        if false {"),
    ("kernel_registry", "name_refused", "a name already registered is taken",
     "        if self.commands.contains_key(&command.name) {",
     "        if false {"),
    ("kernel_registry", "differs", "a command the catalog describes matches its argument schema",
     "        (\"argument schema\", judged.schema != described.judged.schema),",
     "        (\"argument schema\", false),"),
    ("kernel_registry", "undecidable", "an action the schema does not declare is refused",
     "    if !actions.contains(&command.action) {",
     "    if false {"),
    # The entity type is the plugin's own text, of any length the judge
    # admits, so it is not repeated whole: one of 100,000 characters would be.
    ("kernel_registry", "undecidable", "an entity type a refusal repeats is cut to its bound",
     "            quoted(entity),\n",
     "            entity,\n"),
    ("kernel_registry", "undecidable", "a plugin selects the realm only to read it",
     "THE_REALM_IS_READ_WITH || command.mutating)",
     "THE_REALM_IS_READ_WITH)"),
    ("kernel_registry", "unheard", "a subscription its manifest does not declare is refused",
     "        } else if declared.contains(topic) {",
     "        } else if true {"),
    # A malformed name is never repeated; the weakening below repeats every
    # name that is not blank, which a test has to catch. The guard is in the
    # file twice, so the declaring line is part of the text.
    ("kernel_registry", "subject_of", "a malformed name is named by its position",
     "fn subject_of(command: &Declared, at: usize, plugin: &str) -> String {\n    if descriptor::command_name_is_well_formed(&command.name) {",
     "fn subject_of(command: &Declared, at: usize, plugin: &str) -> String {\n    if !command.name.trim().is_empty() {"),
    ("kernel_registry", "subject_of", "a well-formed name is repeated whole",
     "        format!(\"`{}` of `{plugin}`\", command.name)",
     "        format!(\"{} of `{plugin}`\", quoted(&command.name))"),
    # The field the decision point's irreversible precondition reads, which a
    # test compares: every command listed as a read is otherwise green. The bare
    # line is in the file twice, so the next one is part of the text.
    ("kernel_registry", "listed", "a command is listed with the reversibility it declares",
     "            reversibility: self.reversibility,\n            owner,",
     "            reversibility: Reversibility::Read,\n            owner,"),
    ("kernel_registry", "quoted_up_to", "a text a refusal repeats is cut at its bound",
     "        .take(most)\n",
     "        .take(usize::MAX)\n"),
    ("kernel_limits", "memory_growing", "memory grows to its bound and not one byte past it",
     "        if desired > self.memory_bytes {",
     "        if desired > self.memory_bytes + 1 {"),
    ("kernel_limits", "table_growing", "a table grows to the catalog's bound and no further",
     "<= u64::from(KERNEL.table_elements))",
     "< u64::from(KERNEL.table_elements))"),
    ("kernel_host_store", "current", "a read is of the plugin's own key",
     "            .get(&self.realm, &self.plugin, key)\n            .await",
     "            .get(&self.realm, \"tests:door\", key)\n            .await"),
    ("kernel_host_store", "get", "a read answers the revision as the text a write names",
     "            rev: entry.revision.to_string(),",
     "            rev: String::new(),"),
    ("kernel_units", "store_failed", "a store that did not answer is unavailable",
     "        LocalError::Unavailable(_) => RefusalKind::Unavailable,",
     "        LocalError::Unavailable(_) => RefusalKind::Internal,"),

    # -- the reference plugin, judged by building it ----
    ("reference", "adapter", "the adapter is what the wasm32 build compiles",
     "mod adapter;",
     "#[cfg(any())]\nmod adapter;"),
    ("reference_adapter", "descriptor", "a command is declared under its own name",
     "        name: command.name,",
     "        name: command.name + \"s\","),
    # The fields a name-only suite let through: the kernel's suite reads every
    # field the component declares against `behaviour`'s table.
    ("reference_adapter", "descriptor", "a command mutates as its table says",
     "        mutating: command.mutating,",
     "        mutating: !command.mutating,"),
    # Both arms swapped rather than one: two arms answering `Write` is a
    # `match_same_arms` the `wasm32` clippy refuses, and a weakening the build
    # refuses is killed by a lint rather than by the suite that reads it.
    ("reference_adapter", "descriptor", "a read is declared a read",
     "            behaviour::Reversibility::Read => Reversibility::Read,\n            behaviour::Reversibility::Write => Reversibility::Write,",
     "            behaviour::Reversibility::Read => Reversibility::Write,\n            behaviour::Reversibility::Write => Reversibility::Read,"),
    ("reference_behaviour", "execute", "a declared command is told apart from an unknown one",
     "        \"note.get\" => {",
     "        \"note.got\" => {"),
    ("reference_behaviour", "row", "the note is read, not written",
     "        mutating: reversibility != Reversibility::Read,",
     "        mutating: true,"),
    ("reference_behaviour", "probe", "the probe calls every function it names",
     "    nc_canon::Value::object(PROBED.iter().map(|function| {",
     "    nc_canon::Value::object(PROBED.iter().take(1).map(|function| {"),
    ("reference_adapter", "on", "a method behind a refused constructor is not called",
     "        Err(_) => Outcome::Unreachable,",
     "        Err(_) => Outcome::Answered,"),
    ("reference_behaviour", "broken", "a missing action is an empty one",
     "        Variant::MissingAction => command.action = \"\",",
     "        Variant::MissingAction => {}"),
    ("reference_behaviour", "read", "an activation holding anything else is refused",
     "        if !fields.is_empty() {",
     "        if false {"),
    ("reference_behaviour", "commands_for", "only the first command is broken",
     "    if let Some(first) = table.first_mut() {",
     "    if let Some(first) = table.last_mut() {"),
    # The key named, not an arm replaced: two arms answering the default are a
    # `match_same_arms` the `wasm32` clippy refuses, and a weakening the build
    # refuses is killed by a lint rather than by the suite that reads it.
    ("reference_adapter", "activation", "the activation is read from the store",
     "    match store::get(behaviour::ACTIVATION) {",
     "    match store::get(\"\") {"),

    # -- plugins.rs: what a component is shown to be built from -----------------
    ("plugins", "inputs_of", "every member is a directory the list holds whole",
     "        roots.push(directory.clone());",
     "        roots.push(format!(\"{directory}/src\"));"),
    ("plugins", "listing", "a file is listed with its digest",
     "                .map(|(digest, path)| format!(\"file {digest} {path}\\n\")),",
     "                .map(|(_, path)| format!(\"file {path}\\n\")),"),
    ("plugins", "directory_of", "a plugin is the package named for its directory",
     "    if plugin.name == format!(\"nc-plugin-{directory}\") {",
     "    if true {"),
    # The encoder encodes against every world a module carries, a dependency's
    # included, so what keeps a stray import out is the check after it.
    ("plugins", "encode", "a component reaching outside the world is refused",
     "    if outside.is_empty() {",
     "    if true {"),
    ("plugins", "outside_the_world", "an import the world declares is not outside it",
     "        .filter(|name| !world.0.contains(name))",
     "        .filter(|_| true)"),
    ("plugins", "plugin_commands", "a plugin's lint denies its warnings",
     "        [&[\"clippy\"], &common[..], &[\"--\", \"-D\", \"warnings\"]].concat(),\n",
     "        [&[\"clippy\"], &common[..], &[\"--\", \"-W\", \"warnings\"]].concat(),\n"),
    ("plugins", "plugin_commands", "a plugin is documented for wasm32",
     "            &[\"doc\", \"--no-deps\", \"--document-private-items\"],\n            &common[..],\n",
     "            &[\"doc\", \"--no-deps\", \"--document-private-items\"],\n            &common[..2],\n"),
    ("plugins", "plugin_commands", "a plugin is linted, not only checked",
     "        [&[\"clippy\"], &common[..], &[\"--\", \"-D\", \"warnings\"]].concat(),\n",
     "        [&[\"check\"], &common[..], &[\"--\", \"-D\", \"warnings\"]].concat(),\n"),
    ("plugins", "plugin_commands", "a plugin is built for its target",
     "        \"--target\",\n        TARGET,\n        \"--target-dir\",\n",
     "        \"--target-dir\",\n        TARGET,\n        \"--target-dir\",\n"),
    ("members", "built_from_in", "a dev-dependency is not what a component is built from",
     "                        kind.get(\"kind\").and_then(serde_json::Value::as_str) != Some(\"dev\")",
     "                        true"),

    # -- the local database: its file, its schema, its commit, its pruner, its gates --
    # Each is the defect its guard exists for, and each, applied by hand, fails
    # under the command that decides its source, measured.
    ("local_lib", "said", "a busy database is not answering, not broken",
     "Some(5 | 6 | 8 | 10 | 13 | 14) | None => LocalError::Unavailable(why),",
     "Some(6 | 8 | 10 | 13 | 14) | None => LocalError::Unavailable(why),"),
    ("local_lib", "is_a_trigger_refusal", "a trigger's refusal is its extended code",
     ".is_some_and(|code| code == TRIGGER_REFUSED)",
     ".is_some_and(|code| code == \"19\")"),
    ("platform_unix", "judged", "a group or other bit is refused",
     "if mode & OTHERS != 0 {",
     "if mode & OTHERS == OTHERS {"),
    ("platform_unix", "judged", "a link is refused however it points",
     "if file_type.is_symlink() {",
     "if false {"),
    ("platform_unix", "judged", "a foreign owner is refused",
     "if found.uid() != owner.0 {",
     "if false {"),
    ("platform_unix", "create_private", "the file is created with no other bits",
     "file.set_permissions(Permissions::from_mode(0o600))?;",
     "file.set_permissions(Permissions::from_mode(0o644))?;"),
    ("platform_unix", "create_private_directory", "the directory is made private",
     ".mode(0o700)",
     ".mode(0o755)"),
    ("local_schema", "after_migrating", "the schema is held to the list at open",
     "    judge(&found).map_err(LocalError::Unexpected)\n}",
     "    let _ = judge(&found);\n    Ok(())\n}"),
    ("local_schema", "judge_against", "an object nobody listed is refused",
     "        if !expected\n            .iter()\n            .any(|object| object.kind == held.kind && object.name == held.name)\n        {",
     "        if false {"),
    ("local_schema", "at_least", "the floor is compared, not ignored",
     "[Some(major), Some(minor), Some(patch)] => (*major, *minor, *patch) >= floor,",
     "[Some(_), Some(_), Some(_)] => true,"),
    ("local_schema", "no_temporary_object", "a temporary object stops the write",
     "    if temporary == 0 {",
     "    if true {"),
    ("local_store", "holds_what_was_expected", "a revision is compared, not assumed",
     "Expect::Revision(revision) => held.is_some_and(|entry| entry.revision == *revision),",
     "Expect::Revision(_) => held.is_some(),"),
    ("local_store", "write_exactly", "a write that changed the wrong number of rows is refused",
     "    if changed == expected {",
     "    if true {"),
    ("local_store", "within_its_quota", "a plugin past its quota is refused",
     "if as_unsigned(held, \"a count of keys\")? > quotas.entries_for(plugin) {",
     "if as_unsigned(held, \"a count of keys\")? > quotas.entries_for(plugin) + 1 {"),
    ("local_store", "write_one", "a key a plugin holds is not a new key in the file",
     "    Ok(before.is_none() && write.after.is_some())",
     "    Ok(write.after.is_some())"),
    ("local_store", "of", "a key written twice is refused",
     "            if seen.contains(&key) {",
     "            if false {"),
    ("local_store", "of", "a redo is recorded as a redo",
     "            UnitKind::Redo { target } => (\"redo\", Some(target)),",
     "            UnitKind::Redo { target } => (\"undo\", Some(target)),"),
    ("local_store", "canonical", "a value that is not its own canonical form is refused",
     "Ok(value) if value.to_canonical() == text => Ok(()),",
     "Ok(value) if !value.to_canonical().is_empty() => Ok(()),"),
    # Clauses of the local database, each held here so a test has to decide
    # it: the
    # self-check's `quick_check`, the code of a foreign header and of a path
    # the filesystem cannot answer for, the cascade the SQLite pruner follows,
    # a string continued across lines, and whose rows a limit counts.
    ("local_schema", "before_migrating", "a damaged page is refused at open",
     "    if verdicts != [\"ok\"] {",
     "    if false {"),
    ("local_store", "belongs_here", "another application's file is unexpected, not refused",
     "    Err(LocalError::Unexpected(format!(\n        \"the file's header carries application",
     "    Err(LocalError::Refused(format!(\n        \"the file's header carries application"),
    ("local_open", "failed", "a missing parent is the data not being reachable",
     "        _ => LocalError::Unavailable(why),",
     "        _ => LocalError::Refused(why),"),
    ("local_open", "failed", "a permission this process lacks is a refusal",
     "        io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists => LocalError::Refused(why),",
     "        io::ErrorKind::AlreadyExists => LocalError::Refused(why),"),
    ("prune_sqlite", "reached_by_a_delete_from", "a cascade from the table is followed",
     "AND f.on_delete = 'CASCADE'",
     "AND f.on_delete = 'never'"),
    ("prune_sqlite", "reached_by_a_delete_from", "a cascade is followed as far as it goes",
     "f.\\\"table\\\" = reached.name COLLATE NOCASE",
     "f.\\\"table\\\" = ?1 COLLATE NOCASE"),
    ("local_gates", "string_body", "a string continued across a line is one word",
     "            Some('\\n' | '\\r') => {",
     "            Some('\\u{0}') => {"),
    ("loader", "only_a_writer_counts_per_owner", "a per-owner count needs a refusing writer",
     "    if table.per.is_some() && table.strategy != Strategy::RefuseAtLimit {",
     "    if false && table.strategy != Strategy::RefuseAtLimit {"),
    # The kernel's limits and the capabilities, each relation at its boundary.
    ("loader", "check_kernel", "a call tree's deadline is longer than a host call's",
     "kernel.dispatch_deadline_ms > kernel.host_call_deadline_ms,",
     "kernel.dispatch_deadline_ms >= kernel.host_call_deadline_ms,"),
    ("loader", "check_kernel", "a host call's deadline is longer than a request's",
     "kernel.host_call_deadline_ms > kernel.net_timeout_ms,",
     "kernel.host_call_deadline_ms >= kernel.net_timeout_ms,"),
    ("loader", "check_kernel", "the epoch's deadline comes before the call tree's",
     ".is_some_and(|epoch| epoch < kernel.dispatch_deadline_ms),",
     ".is_some_and(|epoch| epoch <= kernel.dispatch_deadline_ms),"),
    ("loader", "check_kernel", "an epoch deadline too long to multiply is refused",
     ".checked_mul(kernel.epoch_deadline_ticks)",
     ".wrapping_mul(kernel.epoch_deadline_ticks)\n                .checked_add(0)"),
    ("loader", "check_kernel", "a payload fits in one store value",
     "kernel.payload_bytes <= kernel.store_value_bytes,",
     "kernel.payload_bytes <= kernel.store_value_bytes + 1,"),
    ("loader", "check_kernel_nonzero", "a kernel limit of zero is refused at either width",
     "        if limit.value == 0 {",
     "        if limit.value == 0 && limit.wide {"),
    ("loader", "check_capabilities", "a function is granted once per interface",
     "granted.insert((&entry.interface, function), &entry.name)",
     "granted.insert((&entry.name, function), &entry.name)"),
    ("loader", "names_its_interface", "a capability is named for its interface",
     "    name == interface\n        || name",
     "    !name.is_empty()\n        || name"),
    ("loader", "check_ungated", "an interface given to every plugin says why",
     "        if says_nothing(&entry.reason) {",
     "        if entry.reason.is_empty() {"),
    ("loader", "check_ungated_functions", "a function given to every plugin is one the world names",
     "if !a_world_function(given) {",
     "if given.is_empty() {"),
    ("loader", "check_ungated_functions", "a function given to every plugin is granted by no capability",
     "capability.interface == entry.interface && capability.functions.contains(given)",
     "capability.interface == entry.interface && capability.functions.is_empty()"),
    ("loader", "check_ungated_functions", "a function given to every plugin is listed once",
     "} else if !listed.insert((&entry.interface, given)) {",
     "} else if listed.is_empty() && !listed.insert((&entry.interface, given)) {"),
    ("pages", "what_a_limit_counts", "a limit counted per plugin is published as one",
     "        (Some(_), Some(owner)) => owner.counted_over(),",
     "        (Some(_), Some(_)) => \"the whole table\","),
    ("prune_sqlite", "beyond_reach", "a cascade that rewrites another table's rows is refused",
     "AND f.on_delete IN ('SET NULL', 'SET DEFAULT')",
     "AND f.on_delete IN ('SET DEFAULT')"),
    ("prune_sqlite", "beyond_reach", "a connection without foreign keys is refused",
     "    if enforced != 1 {",
     "    if false {"),
    ("prune_sqlite", "delete_oldest", "the oldest go first",
     "ORDER BY {ordered} ASC, {key} ASC LIMIT ?1",
     "ORDER BY {ordered} DESC, {key} ASC LIMIT ?1"),
    ("prune_sqlite", "WAITED_RATHER_THAN_REFUSED", "a busy file is a wait",
     "const WAITED_RATHER_THAN_REFUSED: [u32; 2] = [5, 6];",
     "const WAITED_RATHER_THAN_REFUSED: [u32; 2] = [6, 6];"),
    ("pruner", "prune_these_with", "a table of the other database is refused",
     "        if table.database != target.database() {",
     "        if false {"),
    ("pruner", "run", "a run reads its target's own tables",
     "            .filter(|table| table.database == database)",
     "            .filter(|_| true)"),
    ("pruner", "verdict", "a table left to its writer is not work done",
     "            Some(Untouched::APolicyPackBoundsIt | Untouched::ItsWriterRefusesAtTheLimit)",
     "            Some(Untouched::APolicyPackBoundsIt)"),
    ("pruner", "the_catalog_orders_its_own_tables", "the delete door refuses the other database's table",
     "    if declared.database != database {",
     "    if false {"),
    ("local_gates", "built_and_judged", "the schema is held to the list",
     "    nc_local::schema::judge(&found).map_err(|differs| {",
     "    Ok::<(), String>(()).map_err(|differs: String| {"),
    ("local_gates", "built_and_judged", "a foreign key naming nothing is refused",
     "    if orphans != 0 {",
     "    if false {"),
    ("local_gates", "defines_the_schema", "a drop is DDL",
     "\"DROP\" => OBJECTS.contains(&word(at + 1)),",
     "\"DROP\" => false,"),
    ("local_gates", "attaches", "ATTACH is refused",
     ".filter(|(word, _)| word == \"ATTACH\" || word == \"DETACH\")",
     ".filter(|(word, _)| word == \"DETACH\")"),
    ("limits", "cut", "a SQLite trigger's body is one statement",
     "            if dialect == Dialect::Sqlite && a_trigger_body_is_open(&tokens) {",
     "            if false && a_trigger_body_is_open(&tokens) {"),
    ("limits", "a_trigger_body_is_open", "a CASE's END does not close the body",
     "        if token.is(\"CASE\") {\n            cases += 1;",
     "        if token.is(\"CASX\") {\n            cases += 1;"),
    ("limits", "one_directory_declares_its_bounds", "a table is bounded only in its own database",
     "        .filter(|table| table.database == database)\n        .map(|table| table.name)\n        .collect();",
     "        .map(|table| table.name)\n        .collect();"),
    ("gates", "in_the_error_code_scope", "local-migrations/ is inside the §15.1 scope",
     "        || relative.starts_with(\"local-migrations/\")\n",
     ""),
    # More clauses of the local database, each held here so a test has to
    # decide it: a version floor, a quota one number wide for a unit that writes
    # for several, a probe
    # name every open shares, owner bits, a unit committed twice read as a
    # damaged file, a virtual table and a commented trigger the pruner must see,
    # statistics that make the next open refuse, and three ways SQLite writes a
    # table the bound gate must read.
    ("local_lib", "said", "a read-only database is not answering, not broken",
     "Some(5 | 6 | 8 | 10 | 13 | 14) | None => LocalError::Unavailable(why),",
     "Some(5 | 6 | 10 | 13 | 14) | None => LocalError::Unavailable(why),"),
    ("local_lib", "said", "a row still referred to is said so, not as a trigger's",
     "    if is_a_trigger_refusal(error) && database.message() == STILL_REFERRED_TO {\n",
     "    if is_a_trigger_refusal(error) && database.message() == STILL_HELD {\n"),
    ("platform_unix", "judged", "a bit its owner needs is required",
     "    if mode & needed != needed {",
     "    if false {"),
    ("platform", "probe_name", "each open makes a probe of its own",
     "    format!(\"{PROBE}-{}-{now}-{made}\", std::process::id())",
     "    format!(\"{PROBE}-{}\", std::process::id())"),
    ("local_schema", "before_migrating", "the floor is applied at open",
     "    if !at_least(&version, floor) {",
     "    if false && !at_least(&version, floor) {"),
    ("local_store", "entries_for", "each plugin is held to its own quota",
     "            .map_or(self.entries_per_plugin, |own| {\n                (*own).min(self.entries_per_plugin)\n            })",
     "            .map_or(self.entries_per_plugin, |_| self.entries_per_plugin)"),
    ("local_store", "entries_for", "a manifest lowers a quota and never raises it",
     "                (*own).min(self.entries_per_plugin)",
     "                *own"),
    ("local_store", "record_the_unit", "a unit commits once",
     "    if held != 0 {",
     "    if false {"),
    ("prune_sqlite", "is_a_virtual_table", "a virtual table is not an ordinary table",
     "    bare_words(sql).starts_with(&[",
     "    false && bare_words(sql).starts_with(&["),
    ("prune_sqlite", "bare_words", "a comment cannot hide a trigger's event",
     "            ('/', Some('*')) => Some(gap_until(&characters, at + 2, &['*', '/'], false)),",
     "            ('/', Some('*')) => None,"),
    ("prune_sqlite", "bare_words", "a quoted name is not a keyword",
     "            ('\"', _) => Some(gap_until(&characters, at + 1, &['\"'], true)),",
     "            ('\"', _) => None,"),
    ("prune_sqlite", "fires_on_delete", "a trigger's event is read before its first ON",
     "        .take_while(|word| *word != \"ON\")",
     "        .take_while(|_| true)"),
    ("local_gates", "gathers_statistics", "statistics are never gathered",
     "        .filter(|(word, _)| GATHER.contains(&word.as_str()))",
     "        .filter(|_| false)"),
    ("local_gates", "built_and_judged", "the gate applies the floor",
     "    if !nc_local::schema::at_least(&sqlite, floor) {",
     "    if false && !nc_local::schema::at_least(&sqlite, floor) {"),
    ("limits", "cut", "SQLite quotes a name with brackets and backticks",
     "        if dialect == Dialect::Sqlite && (here == '[' || here == '`') {",
     "        if false && (here == '[' || here == '`') {"),
    ("limits", "creates_a_virtual_table", "a virtual table is refused in a migration",
     "three[1].is(\"VIRTUAL\")",
     "three[1].is(\"VIRTUAX\")"),
    # And the rest of them: a migrator that writes into a file the self-check
    # then refuses, a permission arm, SQL comments that part two keywords, a
    # pruner the source gate must read, images and notes a unit is bounded in,
    # and a page of zero keys that says the listing is over.
    ("local_schema", "before_migrating", "a file is judged by its own history before it is migrated",
     "    judge_at(&found, applied.as_deref()).map_err(LocalError::Unexpected)\n}",
     "    let _ = judge_at(&found, applied.as_deref());\n    Ok(())\n}"),
    ("local_schema", "judge_at", "a file is held to what its own history built",
     "            applied.is_some_and(|versions| object.since == 0 || versions.contains(&object.since))",
     "            applied.is_some()"),
    ("local_schema", "written_by_this_build", "a history with a gap is refused",
     "    if !versions.starts_with(applied) {",
     "    if false {"),
    ("local_schema", "written_by_this_build", "a version this build does not carry is refused before the migrator runs",
     "        .find(|row| this_builds(row.version).is_none())\n    {",
     "        .find(|_| false)\n    {"),
    ("local_store", "judged_before_migrating", "the file is judged before the migrator writes",
     "    schema::before_migrating(connection, schema::MINIMUM_SQLITE, &carried).await\n}",
     "    let _ = &carried;\n    Ok(())\n}"),
    ("local_store", "open", "a file is judged on a connection that writes nothing",
     "        let mut judging = crate::open::for_judging(&file)\n",
     "        let mut judging = crate::open::options(&file)\n"),
    ("local_store", "within_what_one_unit_may_record", "a unit past what one unit may record is refused",
     "        if u64::try_from(held).unwrap_or(u64::MAX) > bound {",
     "        if u64::try_from(held).unwrap_or(u64::MAX) > bound + 1 {"),
    ("local_store", "list", "a limit of zero is a page of one",
     "        let limit = limit.max(1);\n",
     ""),
    ("local_gates", "words", "a block comment inside a string is a gap",
     "                (None, '/', Some('*')) => Some(&['*', '/']),",
     "                (None, '/', Some('*')) => None,"),
    ("local_gates", "words", "a line comment inside a string is a gap",
     "                (None, '-', Some('-')) => Some(&['\\n']),",
     "                (None, '-', Some('-')) => None,"),
    ("local_gates", "words", "a SQL quote holds a comment marker as text",
     "            if here == '\\'' || here == '\"' {",
     "            if false {"),
    ("local_gates", "as_compiled", "a character literal opens no string",
     "        if here == '\\''\n            && let Some(width) = crate::gates::char_literal_at(&characters, at)",
     "        if false\n            && let Some(width) = crate::gates::char_literal_at(&characters, at)"),
    ("local_gates", "unread_writers", "a member that opens the file is read or named",
     "        if !read && !named {",
     "        if false {"),
    ("local_gates", "built_one_migration_at_a_time", "each migration is judged as it lands",
     "        nc_local::schema::judge_at(&found, Some(&versions[..=at])).map_err(|differs| {",
     "        Ok::<(), String>(()).map_err(|differs: String| {"),
    ("members", "hands_over_sqlite", "a dev-dependency does not open the local database",
     "        .is_none_or(|kind| kind.is_null() || kind.as_str() == Some(\"build\"));",
     "        .is_none_or(|_| true);"),
    ("members", "resolved_features_in", "a package's resolved features are its own nodes'",
     "                .is_some_and(|id| ids.contains(&id))",
     "                .is_some()"),
    ("members", "hands_over_sqlite", "sqlx's any feature hands a member SQLite",
     ".any(|feature| feature.starts_with(\"sqlite\") || feature == \"any\")",
     ".any(|feature| feature.starts_with(\"sqlite\"))"),
    ("compiler_model", "counted_over", "a limit counted per unit is published as one",
     "            Self::Unit => \"each unit's rows\",",
     "            Self::Unit => \"each plugin's rows in each realm\","),
    ("check", "the_path_attribute_on", "a comparison naming path is not a path attribute",
     "    if after.starts_with('=') {",
     "    if false {"),
    ("check", "the_runnable_doc_fences", "the doc-fence census reads every member library",
     "        if !gates::inside_a_member(&relative).is_some_and(|(_, inside)| inside.starts_with(\"src/\"))\n        {",
     "        if !relative.starts_with(\"crates/\") || !relative.contains(\"/src/\") {"),
    ("check", "the_local_migrations_are_the_ones_pinned", "a local migration edited after a build carried it fails the step",
     "        if now != *pin {",
     "        if false {"),
    ("check", "unset_what_proptest_reads", "a variable proptest reads never reaches the gate's cargo",
     "        if name.as_encoded_bytes().starts_with(b\"PROPTEST_\") {",
     "        if false {"),
    # A suite a system stands aside where it lacks what the suite needs is
    # named by the gate, before the tests and on the count line, and the
    # verdict is held to the list of them as it is to the stack's parts.
    ("check", "the_suites_not_applicable", "a suite stands aside where the system has what it needs",
     "        .filter(|&(_, has)| !has)",
     "        .filter(|&(_, has)| has)"),
    ("check", "what_does_not_apply", "the verdict is not held to the suites a system stands aside",
     "    parts.extend(the_suites_not_applicable(has));",
     "    parts.extend(the_suites_not_applicable([true; 2]));"),
    ("check", "the_suites_the_system_stands_aside", "a suite the system stands aside need not say so",
     "    if !a_posix_shell {",
     "    if !a_posix_shell && listed_as_made {"),
    ("check", "what_the_count_leaves_out", "the count line need not say what the system stood aside",
     "    if !the_suites_not_applicable(has).is_empty() {",
     "    if the_suites_not_applicable(has).len() > THE_SUITES_A_SYSTEM_STANDS_ASIDE.len() {"),
    # The advisories `deny.toml` leaves out of the audit: exactly the ones
    # `THE_ADVISORIES_LEFT_OUT` names, in its order, each with a reason, read
    # from the document cargo-deny reads. The id comparisons and the reason
    # requirement are killed by
    # `an_advisory_is_left_out_only_when_the_gate_names_it_with_a_reason`, the
    # parse refusals by `a_deny_toml_cargo_deny_cannot_read_is_a_finding`.
    ("check", "the_advisories_left_out_are_the_listed_ones", "no fewer advisories are left out than the gate names",
     "    if written != THE_ADVISORIES_LEFT_OUT {",
     "    if written.len() > THE_ADVISORIES_LEFT_OUT.len() {"),
    ("check", "the_advisories_left_out_are_the_listed_ones", "no advisory is left out that the gate does not name",
     "    if written != THE_ADVISORIES_LEFT_OUT {",
     "    if !THE_ADVISORIES_LEFT_OUT.iter().all(|id| written.contains(id)) {"),
    ("check", "the_advisories_left_out_are_the_listed_ones", "an advisory left out with no reason is refused",
     "        if !reasoned {",
     "        if false {"),
    ("check", "the_advisories_left_out", "an empty reason is no reason",
     "                    reason.is_some_and(|reason| !reason.trim().is_empty()),",
     "                    reason.is_some(),"),
    ("check", "the_advisories_left_out", "a bare id is an entry with no reason",
     "            toml::Value::String(id) => Ok((id.clone(), false)),",
     "            toml::Value::String(id) => Ok((id.clone(), true)),"),
    ("check", "the_advisories_left_out", "a deny.toml that is not TOML is not read as one",
     "        .map_err(|error| error.message().trim().to_owned())?;",
     "        .unwrap_or_default();"),
    ("check", "the_advisories_left_out_are_the_listed_ones", "a deny.toml the audit cannot read is a finding",
     "            return false;\n        }\n    };\n    let mut held = true;",
     "            return true;\n        }\n    };\n    let mut held = true;"),
    # -- the decision point, the units of work and the log --------------------
    ("kernel_dispatch", "spend", "a call tree nests no deeper than its depth",
     "        if depth > usize::try_from(KERNEL.dispatch_depth).unwrap_or(usize::MAX) {",
     "        if depth > usize::MAX {"),
    ("kernel_dispatch", "spend", "a call tree is stopped past its deadline",
     "        if now_ms >= self.deadline_ms {",
     "        if now_ms >= u64::MAX {"),
    ("kernel_dispatch", "spend", "a call tree dispatches no more than its count",
     "        if self.dispatches > KERNEL.dispatches_per_tree {",
     "        if false {"),
    ("kernel_dispatch", "registered", "a mutation is refused beneath a read",
     "    if command.listed().mutating && lock(tree).reading() {",
     "    if false {"),
    ("kernel_dispatch", "registered", "a name no command could have is refused as malformed",
     "    if !nc_schema::descriptor::command_name_is_well_formed(name) {\n        return Err(Box::new(Turned::early(",
     "    if false {\n        return Err(Box::new(Turned::early("),
    ("kernel_dispatch", "resource_of", "a malformed name is never written into the journal",
     "        return Value::object([(\"refused\", Value::text(\"malformed-name\"))]);",
     "        return Value::object([(\"refused\", Value::text(name))]);"),
    ("kernel_dispatch", "understood", "a payload past its bound is refused",
     "    if u64::try_from(args.len()).unwrap_or(u64::MAX) > KERNEL.payload_bytes {",
     "    if u64::try_from(args.len()).unwrap_or(u64::MAX) > u64::MAX {"),
    ("kernel_dispatch", "understood", "arguments are held to their command's schema",
     "    if let Err(violations) = judged.schema.validate(&json) {",
     "    if let Some(violations) = None::<Vec<nc_schema::Violation>> {"),
    ("kernel_dispatch", "decision", "a realm pinning a pack not carried is denied",
     "    if !pdp.carries(&facts_of_the_realm) {",
     "    if false {"),
    ("kernel_dispatch", "authorize", "what cannot be undone needs a confirmation",
     "    if asked.command.listed().reversibility == Reversibility::Irreversible && !held.confirmed {",
     "    if false {"),
    ("kernel_dispatch", "bound", "a token is redeemed only with the arguments it was issued for",
     "            && binding.args == digest(args)\n",
     "            && !args.is_empty()\n"),
    ("kernel_dispatch", "used", "a token redeemed is held no longer",
     "    fn used(&mut self, serial: u64) {\n        self.outstanding.remove(&serial);\n    }",
     "    fn used(&mut self, serial: u64) {\n        let _ = serial;\n    }"),
    ("kernel_dispatch", "expired", "a token past its time is no token",
     "fn expired(at_ms: u64, issued: Instant, now_ms: u64) -> bool {\n    let ttl = KERNEL.confirmation_ttl_ms;\n",
     "fn expired(at_ms: u64, issued: Instant, now_ms: u64) -> bool {\n    let ttl = u64::MAX;\n"),
    ("kernel_dispatch", "expired", "a wall clock behind a token's issue is past its time",
     "now_ms < at_ms || now_ms - at_ms > ttl",
     "now_ms.saturating_sub(at_ms) > ttl"),
    ("kernel_dispatch", "expired", "a token's life is held on the monotonic clock too",
     " || issued.elapsed() > Duration::from_millis(ttl)",
     ""),
    ("kernel_dispatch", "executed", "a nested command that failed having written dooms the unit",
     "            if failure.kills || wrote {",
     "            if failure.kills {"),
    ("kernel_dispatch", "answered", "an answer is held to being a payload",
     "            if canonical && u64::try_from(text.len())",
     "            if true || canonical && u64::try_from(text.len())"),
    ("kernel_dispatch", "journaled", "a failed dispatch is journaled as failed",
     "            Outcome::Failed\n",
     "            Outcome::Allowed\n"),
    ("kernel_dispatch", "run", "a command that changes state runs in a unit of its own",
     "    let opens = asked.command.listed().mutating && lock(tree).unit().is_none();",
     "    let opens = false;"),
    ("kernel_dispatch", "run", "a stopped plugin is refused before it is asked",
     "        && shared.stopped(slot)",
     "        && false"),
    ("kernel_units", "staged_as", "a unit writes no more keys than one unit records",
     "            if u64::try_from(self.order.len()).unwrap_or(u64::MAX) >= bound {",
     "            if u64::try_from(self.order.len()).unwrap_or(u64::MAX) >= u64::MAX {"),
    ("kernel_units", "stage", "a staged key expects what the store held when it was first written",
     "        let expect = committed.map_or(Expect::Absent, |entry| Expect::Revision(entry.revision));",
     "        let expect = Expect::Anything;"),
    ("kernel_units", "expected", "a write naming no revision creates a key only where none is",
     "        (None, Some(_)) if creating => conflict(",
     "        (None, Some(_)) if false => conflict("),
    ("kernel_units", "expected", "a write naming another revision is a conflict",
     "        (Some(token), Some(held)) if held.revision.to_string() != token => conflict(format!(",
     "        (Some(token), Some(held)) if false && held.revision.to_string() != token => conflict(format!("),
    ("kernel_units", "merged", "a page shows no key past what the store read",
     "        .filter(|key| reached.as_ref().is_none_or(|last| key <= last))",
     "        .filter(|_| true)"),
    ("kernel_units", "close", "a unit the journal did not close is reverted at once",
     "    let sentence = match revert(shared, unit).await {",
     "    let sentence = match Ok::<(), Refusal>(()) {"),
    ("kernel_units", "reversal", "a reversal lands only over what the unit left",
     "                expect: Expect::Value(image.after.as_ref().map(|entry| entry.value.clone())),",
     "                expect: Expect::Anything,"),
    ("kernel_logging", "capability", "a capability is logged by its catalog name",
     "        Capability::Store => nc_log::variant!(\"store\"),",
     "        Capability::Store => nc_log::variant!(\"stores\"),"),
    ("kernel_host_store", "write", "a write is staged only by a command that changes state",
     "        if !self.frame().is_some_and(Frame::mutating) {",
     "        if self.frame().is_none() {"),
    ("kernel_host_store", "write", "a delete where nothing is stages nothing",
     "        if after.is_none() && current.is_none() {",
     "        if false {"),
    ("kernel_host_store", "page", "a page holds no more keys than the catalog allows",
     "        let limit = limit.clamp(1, KERNEL.store_list_max);",
     "        let limit = limit.max(1);"),
    ("kernel_host_store", "bounded", "a key is held to its bounds",
     "    if (least..=most).contains(&text.len()) {",
     "    if true {"),
    ("kernel_host_store", "payload", "a value is held to its bound",
     "    if u64::try_from(value.len()).unwrap_or(u64::MAX) > KERNEL.store_value_bytes {",
     "    if u64::try_from(value.len()).unwrap_or(u64::MAX) > u64::MAX {"),
    ("kernel_supervisor", "restart", "a plugin that comes back declaring otherwise is stopped",
     "            Ok(activated) if activated.declaration == declaration => {",
     "            Ok(activated) => {"),
    ("kernel_host", "leave", "a refusal is handed to the log once",
     "            .skip(self.logged)",
     "            .skip(0)"),
    ("kernel_dispatch", "detached", "a tree runs to its end whoever stops waiting",
     "tokio::spawn(tree).await",
     "Ok(tree.await)"),
    # -- the decision point: a reversal the store refused ----------------------
    ("kernel_units", "revert", "a reversal the store refused is not one that landed",
     "        .map_err(|refused| commit_refused(&refused))",
     "        .or(Ok(()))"),
    ("kernel_units", "close", "a reversal the store refused is logged",
     "            shared.log.record(&nc_log::events::KernelRevertFailed {\n                unit,\n                code: refused.code(),\n            });\n            format!(",
     "            drop(&nc_log::events::KernelRevertFailed {\n                unit,\n                code: refused.code(),\n            });\n            format!("),
    ("kernel_units", "close", "a close the journal took before its answer was lost is kept",
     "        Ok(Some(Outcome::UnitCommitted)) => {",
     "        Ok(Some(Outcome::UnitAborted)) => {"),
    ("kernel_units", "close", "a journal that cannot say how a unit closed is said to be unsure",
     "        Err(_) => true,",
     "        Err(_) => false,"),
    ("kernel_units", "staged_as", "a unit stages no more bytes than the catalog allows",
     "        if bytes > KERNEL.unit_bytes {",
     "        if bytes > u64::MAX {"),
    ("kernel_units", "bytes_with", "a value written over gives back the room it took",
     "                .saturating_sub(staged.after.as_deref().map_or(0, size))",
     "                .saturating_sub(0)"),
    ("kernel_units", "bytes_with", "a key a unit stages counts against its bytes",
     "                .saturating_add(size(&at.1))",
     "                .saturating_add(0)"),
    ("kernel_dispatch", "journaled", "a read the journal did not take is withheld",
     "    if let Err(error) = entered {",
     "    if let Err(error) = entered && asked.command.listed().mutating {"),
    ("kernel_dispatch", "decision", "the directory's grant is asked under a deadline",
     "within(began, shared.directory.grant(realm, &entity))",
     "shared.directory.grant(realm, &entity)"),
    ("kernel_dispatch", "decision", "the directory's slice is asked under a deadline",
     "    let slice = within(\n        began,\n        shared.directory.slice(realm, &entity, resource, holding),\n    )",
     "    let slice = shared.directory.slice(realm, &entity, resource, holding)"),
    ("kernel_dispatch", "executed", "a trap dooms the unit it ran in though it wrote nothing",
     "            if failure.kills || wrote {",
     "            if wrote {"),
    ("loader", "check_kernel", "one write's key and value fit in a unit",
     "                    .saturating_add(kernel.store_key_bytes),",
     "                    .saturating_add(kernel.store_key_bytes - 1),"),
    ("reference_behaviour", "execute", "a trap traps before it reaches the world",
     "        \"abuse.trap\" => panic!(\"`{name}` trapped at once, as it is written to\"),",
     "        \"abuse.trap\" => Ok(String::new()),"),
    # -- the decision point: a tree's spent count ------------------------------
    ("kernel_dispatch", "spend", "a dispatch past the tree's count traps the plugin that asked",
     "                refusal.trapping()\n",
     "                refusal\n"),
    ("kernel_dispatch", "told_of_the_count", "a plugin is told the tree's count is spent before it is trapped for asking on",
     "        self.told.push(slot);\n        false\n",
     "        self.told.push(slot);\n        true\n"),
    ("kernel_host", "refused", "a refusal that traps is a trap and not a value",
     "    if refusal.traps() {",
     "    if false {"),
    ("kernel_dispatch", "spend", "a dispatch past the deadline counts against its tree",
     "        self.dispatches = self.dispatches.saturating_add(1);",
     "        self.dispatches = self.dispatches.saturating_add(u32::from(now_ms < self.deadline_ms));"),
    ("kernel_dispatch", "spend", "a dispatch refused for its depth counts against its tree",
     "        self.dispatches = self.dispatches.saturating_add(1);",
     "        self.dispatches = self.dispatches.saturating_add(u32::from(depth <= "
     "usize::try_from(KERNEL.dispatch_depth).unwrap_or(usize::MAX)));"),
    ("kernel_dispatch", "journaled", "a nested entry the journal did not take dooms the unit",
     "            unit.doom(refusal.clone());\n",
     "            let _ = unit;\n"),
    ("kernel_dispatch", "decision", "the epoch decided under is the directory's",
     "        policy_epoch: facts_of_the_realm.epoch,",
     "        policy_epoch: facts_of_the_realm.epoch.max(1),"),
    ("kernel_dispatch", "turned_away", "a denial's entry names the grant it rested on",
     "        grant_used: turned.decided.and_then(|decided| decided.grant),",
     "        grant_used: None,"),
    ("kernel_dispatch", "denial", "a denial's entry names the entity it was asked for",
     "        Reason::Denied,\n        resource_of(&asked.name, Some(&asked.resource), None),",
     "        Reason::Denied,\n        resource_of(&asked.name, None, None),"),
    ("kernel_dispatch", "executed", "a nested failure that wrote nothing leaves the unit to its caller",
     "            if failure.kills || wrote {",
     "            if true {"),
    ("kernel_host_store", "page", "a page reads past the keys the unit deleted",
     "        let wider = limit.saturating_add(u32::try_from(deleted.len()).unwrap_or(u32::MAX));",
     "        let wider = limit;"),
    ("kernel_dispatch", "refused_in_the_log", "a refusal is logged saying whether it would have changed state",
     "        mutating,\n    });",
     "        mutating: mutating && false,\n    });"),
    ("reference_behaviour", "relayed", "a relay told to go on through its refusals goes on",
     "                if !through {",
     "                if true {"),
    # -- the decision point: the kernel's own answers --------------------------
    ("kernel_builtin", "verified", "a chain that does not verify is answered broken",
     "            Ok(Value::object([(\n                \"broken\",\n",
     "            Ok(Value::object([(\n                \"verified\",\n"),
    ("kernel_builtin", "verified", "a chain that does not verify is logged as broken",
     "            shared.log.record(&nc_log::events::JournalBroken {",
     "            drop(&nc_log::events::JournalBroken {"),
    ("kernel_builtin", "verified", "a verification starts where it is asked to",
     "        .get(FROM)\n        .and_then(serde_json::Value::as_u64)",
     "        .get(FROM)\n        .map(|_| 0)"),
    ("kernel_builtin", "verified", "a journal that cannot be read is unavailable to verify",
     "                RefusalKind::Unavailable,\n                error.code(),\n"
     "                \"the journal could not be read, so it was not verified\"",
     "                RefusalKind::Internal,\n                error.code(),\n"
     "                \"the journal could not be read, so it was not verified\""),
    ("kernel_dispatch", "refused_in_the_log", "a refusal is logged with its own code",
     "        code: refusal.code(),\n        reason: reason.variant(),",
     "        code: ErrorCode::PolicyDenied,\n        reason: reason.variant(),"),
    ("kernel_units", "abort", "an aborted unit is logged as the unit it was",
     "        unit,\n        cause: cause.variant(),",
     "        unit: shared.journal.lock().await.mint_unit().unwrap_or(unit),\n"
     "        cause: cause.variant(),"),
    ("kernel_supervisor", "killed", "a killed plugin is logged by its slot",
     "KernelPluginKilled {\n        slot: logged_slot(slot),",
     "KernelPluginKilled {\n        slot: logged_slot(slot).saturating_add(7),"),
    ("kernel_supervisor", "killed", "a killed plugin is logged with its kill's code",
     "        cause: kill.variant(),\n        code: kill.code(),",
     "        cause: kill.variant(),\n        code: ErrorCode::PluginQuarantined,"),
    ("kernel_supervisor", "restart", "a restarted plugin is logged by its slot",
     "KernelPluginRestarted {\n            slot: logged_slot(slot),",
     "KernelPluginRestarted {\n            slot: logged_slot(slot).saturating_add(7),"),
    ("kernel_supervisor", "logged_as_quarantined", "a quarantined plugin is logged by its slot",
     "KernelPluginQuarantined {\n        slot: logged_slot(slot),",
     "KernelPluginQuarantined {\n        slot: logged_slot(slot).saturating_add(7),"),
    ("kernel_supervisor", "logged_as_quarantined", "a quarantined plugin is logged with its own code",
     "        code: ErrorCode::PluginQuarantined,\n    });",
     "        code: ErrorCode::PluginTrapped,\n    });"),
    ("kernel_dispatch", "turned_away", "a refusal's entry is recorded when it was refused",
     "        work_unit: unit,\n        recorded_at: shared.clock.now_ms(),",
     "        work_unit: unit,\n        recorded_at: 0,"),
    ("kernel_dispatch", "closed", "a doomed unit is refused in the case of what doomed it",
     "                doomed.kind(),",
     "                RefusalKind::Internal,"),
    ("kernel_logging", "LogSink for std::sync::Mutex", "the central log's sink ships what it is given",
     "        self.lock()\n            .unwrap_or_else(std::sync::PoisonError::into_inner)\n"
     "            .record(event);",
     "        let _ = (self, event);"),
    ("kernel_units", "reversal", "a reversal is labelled as the kernel's",
     "            label: REVERSAL_LABEL.to_owned(),",
     "            label: REVERSAL_LABEL.replace('.', \"-\"),"),
    # -- isolation and supervision: why a plugin is killed, and what then ------
    ("kernel_supervisor", "of", "a memory growth refused is told apart from a trap",
     "        if error.downcast_ref::<MemoryRefused>().is_some() {",
     "        if false {"),
    ("kernel_supervisor", "of", "a host call past its deadline is told apart from a trap",
     "        if error.downcast_ref::<HostCallDeadline>().is_some() {",
     "        if false {"),
    ("kernel_supervisor", "of", "a plugin out of fuel is told apart from one past its epoch",
     "            Some(wasmtime::Trap::OutOfFuel) => Self::Fuel,",
     "            Some(wasmtime::Trap::OutOfFuel) => Self::Epoch,"),
    ("kernel_supervisor", "of", "a plugin past its epoch is told apart from one out of fuel",
     "            Some(wasmtime::Trap::Interrupt) => Self::Epoch,",
     "            Some(wasmtime::Trap::Interrupt) => Self::Fuel,"),
    ("kernel_supervisor", "code", "a kill for fuel carries the fuel's code",
     "            Self::Fuel => ErrorCode::PluginOutOfFuel,",
     "            Self::Fuel => ErrorCode::PluginTrapped,"),
    ("kernel_supervisor", "code", "a kill for the host call's deadline carries its own code",
     "            Self::HostCallDeadline => ErrorCode::PluginHostCallDeadline,",
     "            Self::HostCallDeadline => ErrorCode::PluginTrapped,"),
    ("kernel_supervisor", "counted", "a kill older than the window is forgotten",
     "            .is_some_and(|at| now_ms.saturating_sub(*at) >= KERNEL.restart_window_ms)",
     "            .is_some_and(|_| false)"),
    ("kernel_supervisor", "counted", "the budget's last kill inside the window spends it",
     "        self.recent.len() >= usize::try_from(KERNEL.restart_budget).unwrap_or(usize::MAX)",
     "        self.recent.len() > usize::try_from(KERNEL.restart_budget).unwrap_or(usize::MAX)"),
    ("kernel_supervisor", "forgiven", "a release forgets the kills inside the window",
     "        self.recent.clear();\n",
     ""),
    ("kernel_supervisor", "forgiven", "a release forgives the kills it counts",
     "        self.crashes = 0;\n",
     ""),
    ("kernel_supervisor", "killed", "a kill that spends the budget quarantines the plugin",
     "        if spent {\n            loaded.slot = Slot::Stopped;\n        }",
     "        if false {\n            loaded.slot = Slot::Stopped;\n        }"),
    ("kernel_supervisor", "killed", "a killed plugin is logged with its cause",
     "        cause: kill.variant(),",
     "        cause: Kill::Trap.variant(),"),
    ("kernel_supervisor", "restart", "a plugin that comes back is the next generation of itself",
     "loaded.supervision.generation.saturating_add(1);",
     "loaded.supervision.generation.saturating_add(0);"),
    ("kernel_supervisor", "release", "a release forgives every kill",
     "        loaded.supervision.forgiven();\n",
     ""),
    ("kernel_supervisor", "release", "a release that does not bring the plugin back is refused",
     "    if restart(shared, slot, None).await == Back::Restarted || shared.status(slot).is_none() {",
     "    if restart(shared, slot, None).await == Back::Restarted || true {"),
    ("kernel_supervisor", "in_time", "a host call is held to the catalog's deadline",
     "    let own = Duration::from_millis(KERNEL.host_call_deadline_ms);",
     "    let own = Duration::from_millis(KERNEL.host_call_deadline_ms * 2);"),
    # **A host call is held to what its tree has left**: otherwise a fetch in a
    # tree whose time is up is sent, and waited on for as long as the host
    # call's own deadline allows, measured.
    ("kernel_supervisor", "in_time", "a host call is held to what its tree has left",
     "    let tree = left.filter(|left| *left <= own);",
     "    let tree = left.filter(|_| false);"),
    ("kernel_supervisor", "in_time", "a host call in a tree with no time left is never started",
     "    if tree.is_some_and(|left| left.is_zero()) {",
     "    if false {"),
    # **A release runs on a task of its own**: awaited in place, one its caller
    # gives up on leaves the slot claimed as starting, for good.
    ("kernel_supervisor", "released", "a release runs to its end whoever stops waiting",
     "    tokio::spawn(async move { release(&shared, slot).await })\n        .await\n",
     "    Ok::<_, tokio::task::JoinError>(release(&shared, slot).await)\n"),
    ("kernel_supervisor", "began", "a call is given the ticks its plugin is held to",
     "            share: tick().saturating_mul(u32::try_from(ticks).unwrap_or(u32::MAX)),",
     "            share: tick(),"),
    ("kernel_supervisor", "crossed", "the time spent in the host is counted apart",
     "            self.entered = Some(Instant::now());",
     "            self.entered = None;"),
    ("kernel_supervisor", "crossed", "the plugin's allocator inside a host call leaves the wait to the host",
     "        if hook.entering_host() {\n            self.entered = Some(Instant::now());\n"
     "        } else if let Some(entered) = self.entered.take() {\n"
     "            self.waited = self.waited.saturating_add(entered.elapsed());\n        }",
     "        match hook {\n"
     "            wasmtime::CallHook::CallingHost => self.entered = Some(Instant::now()),\n"
     "            wasmtime::CallHook::ReturningFromHost => {\n"
     "                if let Some(entered) = self.entered.take() {\n"
     "                    self.waited = self.waited.saturating_add(entered.elapsed());\n"
     "                }\n            }\n"
     "            wasmtime::CallHook::CallingWasm | wasmtime::CallHook::ReturningFromWasm => {}\n"
     "        }"),
    ("kernel_supervisor", "reached", "the epoch counts the plugin's own running",
     "            began.elapsed().saturating_sub(waited),",
     "            began.elapsed(),"),
    ("kernel_supervisor", "reached", "a call past its tree's deadline is stopped as the tree's",
     "            Verdict::Whole => Err(wasmtime::Error::new(TreeDeadline)),",
     "            Verdict::Whole => Ok(wasmtime::UpdateDeadline::Interrupt),"),
    ("kernel_supervisor", "verdict", "no call outlives a call tree's deadline",
     "    if tree >= whole {\n        return Verdict::Whole;\n    }\n",
     ""),
    ("kernel_supervisor", "began", "a call begun late in its tree is given what the tree has left",
     "            now.saturating_duration_since(tree),",
     "            Duration::ZERO,"),
    ("kernel_supervisor", "of", "a call past its tree's deadline is told apart from a trap",
     "        if error.downcast_ref::<TreeDeadline>().is_some() {",
     "        if false {"),
    ("kernel_supervisor", "code", "a kill past its tree's deadline carries the tree's code",
     "            Self::TreeDeadline => ErrorCode::KernelDeadline,",
     "            Self::TreeDeadline => ErrorCode::PluginEpochDeadline,"),
    ("kernel_supervisor", "counted", "a kill at its tree's deadline spends no budget",
     "        if kill == Kill::TreeDeadline {",
     "        if false {"),
    ("kernel_supervisor", "word", "a kill for the host call's deadline is journaled in its own word",
     "            Self::HostCallDeadline => \"host-call-deadline\",",
     "            Self::HostCallDeadline => \"trapped\","),
    ("kernel_supervisor", "said", "a kill for the host call's deadline is said as a wait on the host",
     "            Self::HostCallDeadline => \"waited on a host call past its deadline\",",
     "            Self::HostCallDeadline => \"trapped\","),
    ("kernel_supervisor", "word", "a kill past its tree's deadline is journaled in its own word",
     "            Self::TreeDeadline => \"tree-deadline\",",
     "            Self::TreeDeadline => \"trapped\","),
    ("kernel_supervisor", "said", "a kill past its tree's deadline is said as the tree's",
     "            Self::TreeDeadline => \"ran past its call tree's deadline\",",
     "            Self::TreeDeadline => \"trapped\","),
    ("kernel_supervisor", "variant", "a kill past its tree's deadline is logged as the tree's",
     "            Self::TreeDeadline => nc_log::variant!(\"tree\"),",
     "            Self::TreeDeadline => nc_log::variant!(\"trap\"),"),
    ("kernel_supervisor", "restart", "a restarted plugin holds only the grants its manifest names",
     "            let state =\n                crate::host::HostState::for_plugin(manifest, shared, Arc::clone(&loaded.log))\n                    .within(within);\n",
     "            let state = crate::host::HostState::new(\n                crate::capability::GrantSet::every(),\n                (shared.realm.clone(), manifest.id().to_owned()),\n                manifest.domains().to_vec(),\n                Arc::clone(&shared.store),\n                Arc::clone(&shared.blobs),\n                Arc::clone(&shared.transport),\n                crate::limits::MemoryGuard::new(manifest.limits()),\n                Arc::clone(&loaded.log),\n            )\n            .within(within);\n"),
    ("kernel_supervisor", "restart", "a restarted plugin's memory is held to its manifest's bound",
     "            let state =\n                crate::host::HostState::for_plugin(manifest, shared, Arc::clone(&loaded.log))\n                    .within(within);\n",
     "            let state = crate::host::HostState::new(\n                manifest.grants().clone(),\n                (shared.realm.clone(), manifest.id().to_owned()),\n                manifest.domains().to_vec(),\n                Arc::clone(&shared.store),\n                Arc::clone(&shared.blobs),\n                Arc::clone(&shared.transport),\n                crate::limits::MemoryGuard::new(&crate::limits::Limits::catalog()),\n                Arc::clone(&loaded.log),\n            )\n            .within(within);\n"),
    ("kernel_supervisor", "restart", "an instance that never came up is no new generation",
     "            Ok(_) | Err(_) => {\n                loaded.slot = Slot::Stopped;\n",
     "            Ok(_) | Err(_) => {\n                loaded.slot = Slot::Stopped;\n"
     "                loaded.supervision.generation = loaded.supervision.generation.saturating_add(1);\n"),
    ("kernel_engine", "configuration", "a plugin is held to the catalog's stack",
     "    config.max_wasm_stack(usize::try_from(KERNEL.wasm_stack_bytes).unwrap_or(usize::MAX));\n",
     ""),
    ("kernel_engine", "ticker", "the ticker advances the epoch",
     "                    engine.increment_epoch();\n",
     ""),
    ("kernel_bindings", "store_for", "the epoch deadline asks what the plugin ran",
     "    store.epoch_deadline_callback(|mut store| store.data_mut().running().reached());",
     "    store.epoch_deadline_trap();"),
    ("kernel_bindings", "store_for", "every crossing into the host is told",
     "        store.data_mut().running().crossed(hook);",
     "        let _ = (&mut store, hook);"),
    ("kernel_bindings", "begun", "a call begins its count of the epoch",
     "    let first = store.data_mut().running().began(ticks, tree);",
     "    let first = {\n        let _ = tree;\n        ticks\n    };"),
    ("kernel_bindings", "begun", "a call in a tree is held from the tree's start",
     "    let tree = store.data().tree_began();",
     "    let tree = None;"),
    # **A release claims the slot it starts the plugin in.** One that leaves it
    # stopped while the plugin is instantiated lets a second release start a
    # second instance and put it over the one a command has taken out to run,
    # which lets that command dispatch into itself.
    ("kernel_supervisor", "release", "a release claims the plugin before it starts it again",
     "        loaded.slot = Slot::Starting;\n",
     ""),
    ("kernel_supervisor", "stopped", "a plugin a release is starting is not run",
     "                matches!(loaded.slot, Slot::Stopped | Slot::Starting | Slot::Unloaded)\n",
     "                matches!(loaded.slot, Slot::Stopped | Slot::Unloaded)\n"),
    ("kernel_supervisor", "status", "a plugin a release is starting is quarantined still",
     "            quarantined: matches!(loaded.slot, Slot::Stopped | Slot::Starting),",
     "            quarantined: matches!(loaded.slot, Slot::Stopped),"),
    # The decision point asks whether a plugin is stopped before it checks one
    # out, so only the supervisor's own test reaches a check-out of a slot with
    # no store in it; without that test, a put-back here can be deleted with
    # every suite green.
    ("kernel_supervisor", "check_out", "a check-out leaves a slot that holds no store as it was",
     "            Slot::Stopped | Slot::Starting | Slot::Unloaded => return Err(Out::Stopped),\n",
     "            Slot::Stopped | Slot::Starting | Slot::Unloaded => {}\n"),
    # **The manifest's own bounds, read back.** Handed the catalog's fuel or
    # ticks instead, the plugin that burns still ran out of fuel and the one
    # that activates still came up, so only what the store was given tells.
    ("kernel_bindings", "dispatched", "a dispatch runs on the fuel its manifest lowers",
     "        limits.fuel_per_dispatch(),\n",
     "        nc_catalog::KERNEL.fuel_per_dispatch,\n"),
    ("kernel_bindings", "activated", "activate runs on the epoch ticks its manifest lowers",
     "        limits.epoch_deadline_ticks(),\n    )\n    .map_err(NotActivated::Failed)?;",
     "        nc_catalog::KERNEL.epoch_deadline_ticks,\n    )\n    .map_err(NotActivated::Failed)?;"),
    ("kernel_bindings", "on_timer", "a timer's callback is given what one dispatch is",
     "    dispatched(instance, limits)?;\n    instance\n        .plugin\n        .noctua_kernel_guest()\n        .call_on_timer(",
     "    let _ = limits;\n    instance\n        .plugin\n        .noctua_kernel_guest()\n        .call_on_timer("),
    ("kernel_bindings", "on_event", "an event's callback is given what one dispatch is",
     "    dispatched(instance, limits)?;\n    instance\n        .plugin\n        .noctua_kernel_guest()\n        .call_on_event(",
     "    let _ = limits;\n    instance\n        .plugin\n        .noctua_kernel_guest()\n        .call_on_event("),
    ("kernel_dispatch", "plugin", "what stopped a plugin is what it is killed for",
     "            let kill = supervisor::Kill::of(&stopped);",
     "            let kill = supervisor::Kill::Trap;"),
    ("kernel_dispatch", "killed", "a plugin killed and restarted is refused in its kill's code",
     "        supervisor::Back::Restarted => (kill.code(), \"was killed and restarted\"),",
     "        supervisor::Back::Restarted => (ErrorCode::PluginTrapped, \"was killed and restarted\"),"),
    ("kernel_dispatch", "killed", "a killed command is journaled with its kill's word",
     "        word: kill.word(),",
     "        word: \"trapped\","),
    ("kernel_host_store", "get", "a read of the store is held to the host call's deadline",
     "        let held = in_time(\"store#get\", self.left(), self.current(&key)).await?;",
     "        let held = self.current(&key).await?;"),
    ("kernel_host_store", "put", "a write's read is held to the host call's deadline",
     "        let written = in_time(\n            \"store#put\",\n            self.left(),\n            self.write(\"store#put\", &key, Some(value), expected.as_deref()),\n        )\n        .await?;",
     "        let written = self\n            .write(\"store#put\", &key, Some(value), expected.as_deref())\n            .await?;"),
    ("kernel_host_store", "list", "a listing is held to the host call's deadline",
     "        let page = in_time(\n            \"store#list\",\n            self.left(),\n            self.page(&prefix, limit, cursor.as_deref()),\n        )\n        .await?;",
     "        let page = self.page(&prefix, limit, cursor.as_deref()).await?;"),
    ("kernel_host_store", "delete", "a delete's read is held to the host call's deadline",
     "        in_time(\n            \"store#delete\",\n            self.left(),\n            self.write(\"store#delete\", &key, None, expected.as_deref()),\n        )\n        .await\n        .map(drop)",
     "        self.write(\"store#delete\", &key, None, expected.as_deref())\n            .await\n            .map(drop)"),
    ("kernel_host_store", "write", "a write where nothing changes is recorded as refused there",
     "        if !self.frame().is_some_and(Frame::mutating) {\n            return Err(self.refused_here(function, &read_only()));",
     "        if !self.frame().is_some_and(Frame::mutating) {\n            return Err(refused(&read_only()));"),
    ("reference_behaviour", "reentered", "a dispatch into its own plugin names its own namespace",
     "    let namespace = name\n        .split_once('.')\n        .map_or(name, |(namespace, _)| namespace);",
     "    let namespace = \"reference\";"),
    ("reference_behaviour", "abused", "a pair that traps dispatches the peer's trap",
     "        \"pair.put-then-trap\" => paired(args, \"note.put-then-trap\", imports),",
     "        \"pair.put-then-trap\" => paired(args, \"note.put\", imports),"),
    ("kernel_blobs", "parse", "a hash is lowercase hexadecimal",
     "                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));",
     "                .all(|byte| byte.is_ascii_hexdigit());"),
    ("kernel_blobs", "parse", "a hash is sixty-four characters and no more",
     "        let shaped = text.len() == 64",
     "        let shaped = text.len() >= 64"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "bytes the store holds already are not written again",
     "let held_before = file_of(&root, &hash)?.is_some_and(|(_, held)| held == size);",
     "let held_before = false;"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "bytes under a hash that are not its bytes are replaced",
     "let held_before = file_of(&root, &hash)?.is_some_and(|(_, held)| held == size);",
     "let held_before = file_of(&root, &hash)?.is_some_and(|(_, held)| held <= size);"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "a writing is created private",
     "            let file = nc_platform::create_private(&path)",
     "            let file = std::fs::File::create_new(&path)"),
    ("kernel_blobs", "Drop for Abandoned", "an unfinished writing is deleted with its writer",
     "        let _left_for_the_sweep = std::fs::remove_file(&self.path);\n",
     ""),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "a writing a writer holds is not forgotten",
     "if !name.starts_with(WRITING) || held(&writing).contains(&name) {",
     "if !name.starts_with(WRITING) {"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "what is not a writing is not the store's to delete",
     "if !name.starts_with(WRITING) || held(&writing).contains(&name) {",
     "if held(&writing).contains(&name) {"),
    ("kernel_blobs", "file_of", "a name that is not a regular file is not a blob's bytes",
     "        Ok(found) if found.file_type().is_file() => Ok(Some((path, found.len()))),",
     "        Ok(found) => Ok(Some((path, found.len()))),"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "a read stops at what it asked for",
     "                .min(max);",
     "                .min(usize::MAX);"),
    ("kernel_blobs", "BlobStore for MemoryBlobs", "bytes held already are not written again in memory",
     "            .is_some_and(|bytes| bytes.len() == writer.bytes.len());",
     "            .is_some_and(|_| false);"),
    ("kernel_blobs", "BlobStore for MemoryBlobs", "a read in memory stops at what it asked for",
     "            let to = from.saturating_add(max).min(bytes.len());",
     "            let to = bytes.len();"),
    ("kernel_blobs", "unpinned_by", "a blob held before is not unpinned by its unit's reversal",
     "        if let Some((hash, false)) = created(written)? {",
     "        if let Some((hash, _)) = created(written)? {"),
    ("kernel_blobs", "created", "a blob's note with a field beside its own is refused",
     "= (hash, pinned_before, fields.len()) else {",
     "= (hash, pinned_before, 1) else {"),
    ("kernel_blobs", "noted", "a note says whether the blob was pinned before",
     "        (\"pinned_before\", nc_canon::Value::Bool(pinned_before)),",
     "        (\"pinned_before\", nc_canon::Value::Bool(false)),"),
    ("kernel_blobs", "let_go", "bytes held before a unit are not deleted when it is let go",
     "    for blob in made.iter().filter(|blob| !blob.kept_before) {",
     "    for blob in made {"),
    ("kernel_blobs", "let_go", "bytes a row names are not deleted when a unit is let go",
     "        if let Ok(None) = store.blob(blob.hash.as_str()).await {",
     "        if let Ok(_) = store.blob(blob.hash.as_str()).await {"),
    ("kernel_blobs", "sweep", "a sweep forgets only what is past its grace",
     "    let before = now_ms.saturating_sub(KERNEL.blob_orphan_grace_ms);",
     "    let before = now_ms;"),
    ("kernel_blobs", "sweep", "a sweep reads every page",
     "        if !forgot || due.len() < usize::try_from(SWEPT_AT_ONCE).unwrap_or(usize::MAX) {",
     "        if true {"),
    ("kernel_blobs", "sweep", "bytes a row names are not orphans",
     "        if store.blob(hash.as_str()).await.map_err(read)?.is_none()",
     "        if store.blob(hash.as_str()).await.map_err(read)?.is_some()"),
    ("kernel_blobs", "sweep", "a sweep forgets the writings no writer holds",
     "    swept.unfinished = blobs.forget_unfinished().await.map_err(bytes)?;",
     "    swept.unfinished = 0;"),
    ("kernel_blobs", "code", "a failed disk carries the blobs' own code",
     "            Self::Io(_) => ErrorCode::BlobIo,",
     "            Self::Io(_) => ErrorCode::DbUnreachable,"),
    ("kernel_blobs", "refusal", "a failed disk is unavailable, not internal",
     "            Self::Io(why) => Refusal::new(RefusalKind::Unavailable, ErrorCode::BlobIo, why.clone()),",
     "            Self::Io(why) => Refusal::new(RefusalKind::Internal, ErrorCode::BlobIo, why.clone()),"),
    ("kernel_blobs", "BlobStore for Arc", "a shared store removes what it is asked to",
     "        (**self).remove(hash)",
     "        core::future::ready(Ok(false))"),
    ("kernel_host_blobs", "locate", "a blob is never said to be elsewhere",
     "        } else {\n            world::Availability::Unavailable\n        })",
     "        } else {\n            world::Availability::Remote\n        })"),
    ("kernel_host_blobs", "located", "a blob the tree's own unit made is on this machine for it",
     "        if !self.made_here(hash) {",
     "        if true {"),
    ("kernel_host_blobs", "located", "a row no committed unit pins is not a blob on this machine",
     "            if !row.is_some_and(|row| row.pinned) {",
     "            if row.is_none() {"),
    ("kernel_host_blobs", "read", "a read answers at most a chunk",
     "            .min(KERNEL.blob_read_chunk_bytes)\n",
     ""),
    ("kernel_host_blobs", "read", "a read moves its reader on",
     "            open.at = at.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));",
     "            open.at = at;"),
    ("kernel_host_blobs", "seek", "a seek past the end is refused",
     "        if offset > size {",
     "        if false {"),
    ("kernel_host_blobs", "writes_here", "a blob is written only where a command changes state",
     ".is_some_and(|frame| frame.mutating() && frame.tree().unit().is_some());",
     ".is_some_and(|frame| frame.tree().unit().is_some());"),
    ("kernel_host_blobs", "write", "a blob past its bound is refused",
     "        if after > KERNEL.blob_bytes {",
     "        if false {"),
    ("kernel_host_blobs", "write", "a blob may be exactly its bound",
     "        if after > KERNEL.blob_bytes {",
     "        if after >= KERNEL.blob_bytes {"),
    ("kernel_host_blobs", "finish", "a finished writer takes nothing more",
     ".insert(writer.rep(), Writing::Finished(hash.clone()));",
     ".insert(writer.rep(), Writing::Open { writer: None, size: 0 });"),
    ("kernel_host_blobs", "room", "a plugin holds no more blob handles than its bound",
     "< u64::from(KERNEL.blob_handles) {",
     "<= u64::from(KERNEL.blob_handles) {"),
    ("kernel_host_blobs", "HostReader for HostState", "a reader let go is room for another",
     "        self.handles.readers.remove(&reader.rep());\n",
     ""),
    ("kernel_host_blobs", "HostWriter for HostState", "a writer let go is room for another",
     "        self.handles.writers.remove(&writer.rep());\n",
     ""),
    ("kernel_host_blobs", "finish", "a finished blob is its unit's",
     "            unit.make(made);\n",
     ""),
    ("kernel_host_blobs", "finish", "bytes held before a unit stay when it is let go",
     "                kept_before: finished.held_before || row.is_some(),",
     "                kept_before: row.is_some(),"),
    ("kernel_host_blobs", "finish", "a blob pinned before is noted so",
     "                pinned_before: row.is_some_and(|row| row.pinned),",
     "                pinned_before: false,"),
    ("kernel_host_blobs", "announce", "an announcement says no peer heard it",
     "            ErrorCode::BlobNoPeers,",
     "            ErrorCode::BlobNotLocal,"),
    ("kernel_units", "make", "a blob made twice is made once",
     "        if !self.made_blob(&made.hash) {",
     "        if true {"),
    ("kernel_units", "room_for_a_blob", "a unit makes no more blobs than it may note",
     "        if self.notes() < bound {",
     "        if self.notes() <= bound {"),
    ("kernel_units", "changes", "a blob made is a change",
     "            .saturating_add(u64::try_from(self.made.len()).unwrap_or(u64::MAX))\n",
     ""),
    ("kernel_units", "changes", "an event, a timer or a cancel held for later is a change",
     "            .saturating_add(self.later.held)\n",
     ""),
    ("kernel_units", "pinned_by", "a unit's blobs are recorded with it",
     "        .map(|made| BlobCreated {",
     "        .filter(|_| false)\n        .map(|made| BlobCreated {"),
    ("kernel_units", "notes_kept", "a unit notes each blob it made",
     "        let mut notes: Vec<String> = self.made.iter().map(blobs::note).collect();",
     "        let mut notes: Vec<String> = Vec::new();"),
    ("kernel_units", "close", "a unit the store refused lets its blobs go",
     "        let_go(shared, &ambient, Cause::Conflict).await;",
     "        abort(shared, unit, Cause::Conflict, ambient.waits).await;"),
    ("kernel_units", "reversal", "a reversal takes off the pins its unit put on",
     "    let mut unpinned = blobs::unpinned_by(&detail.notes)?;",
     "    let mut unpinned = Vec::new();"),
    ("kernel_dispatch", "closed", "a unit that failed lets its blobs go",
     "            units::let_go(shared, &ambient, cause).await;",
     "            units::abort(shared, unit, cause, asked.waits).await;"),
    ("kernel_dispatch", "closed", "a unit a nested failure doomed lets its blobs go",
     "            units::let_go(shared, &ambient, doomed_for).await;",
     "            units::abort(shared, unit, doomed_for, asked.waits).await;"),
    ("kernel_backend", "pinned", "a pin is taken off in memory as in the file",
     "        row.pinned = false;",
     "        row.pinned = row.pinned;"),
    ("kernel_backend", "shaped", "a blob pinned and unpinned by one unit is refused in memory",
     "        if hashes.contains(&hash.as_str()) || unpinned.contains(&hash.as_str()) {",
     "        if unpinned.contains(&hash.as_str()) {"),
    ("local_store", "commit_with", "a pin taken off a blob the store does not hold is refused",
     "            if !unpin(&mut transaction, hash, shaped.committed_at).await? {",
     "            if false {"),
    ("local_store", "commit_with", "a pin is taken off as of the unit's own time",
     "            if !unpin(&mut transaction, hash, shaped.committed_at).await? {",
     "            if !unpin(&mut transaction, hash, 0).await? {"),
    ("local_store", "of", "a blob pinned and unpinned by one unit is refused",
     "            if hashes.contains(&hash.as_str()) || unpinned.contains(&hash.as_str()) {",
     "            if unpinned.contains(&hash.as_str()) {"),
    ("local_store", "of", "a pin taken off what is not a hash is refused",
     "            hash_shaped(hash)?;\n",
     ""),
    ("local_open", "blob_directory", "the blobs' directory is judged",
     "    judged_directory(directory, Holding::Blobs).map(drop)",
     "    Ok(())"),
    ("local_open", "blob_directory", "the blobs' directory is refused in its own words",
     "    judged_directory(directory, Holding::Blobs).map(drop)",
     "    judged_directory(directory, Holding::Database).map(drop)"),
    ("reference_behaviour", "blob_put", "a blob is written once when nothing says how often",
     "        None => 1,",
     "        None => 2,"),
    ("reference_behaviour", "blob_put", "a blob told to fail refuses",
     "        Some(word) if word == \"fail\" => Some(Then::Fail),",
     "        Some(word) if word == \"fail\" => None,"),
    ("reference_behaviour", "blob_put", "a blob told to ask where it is asks",
     "        None if located => Ok(answer([",
     "        None if false => Ok(answer(["),
    ("reference_behaviour", "shown", "bytes past what an answer repeats are not repeated",
     "    if bytes.len() > SHOWN {",
     "    if false {"),
    ("reference_adapter", "get_blob", "a blob read back says how much each read answered",
     "            chunks.push(u64::try_from(chunk.len()).unwrap_or(u64::MAX));",
     "            chunks.push(0);"),
    ("reference_adapter", "put_blob", "every piece of a blob is written",
     "        for _ in 0..times {",
     "        for _ in 0..times.min(1) {"),
    ("kernel", "sweep_blobs", "a sweep waits for the call tree that is running",
     "        let _between_trees = self.shared.bus.lock().await;\n",
     ""),
    ("kernel_blobs", "open", "the blobs' directory is judged when the store is opened",
     "        nc_local::blob_directory(root).map_err(BlobError::Directory)?;\n",
     ""),
    ("kernel_blobs", "sweep", "a sweep forgets nothing before its whole grace has passed",
     "    let before = now_ms.saturating_sub(KERNEL.blob_orphan_grace_ms);",
     "    let before = now_ms.saturating_sub(KERNEL.blob_orphan_grace_ms / 2);"),
    ("kernel_host_blobs", "read", "a reader whose bytes are gone is told so, not given an end",
     "        let Some(bytes) = bytes else {",
     "        let Some(bytes) = bytes.or_else(|| Some(Vec::new())) else {"),
    ("kernel_blobs", "names_in", "a directory is no blob and no writing to list",
     "        if entry.file_type().map_err(listing)?.is_dir() {",
     "        if false {"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "a directory under a blob's name is not deleted as its bytes",
     "            if std::fs::symlink_metadata(&path).is_ok_and(|found| found.is_dir()) {",
     "            if false {"),
    ("kernel_blobs", "Drop for DirectoryWriter", "a writing let go is deleted on a thread of its own",
     "        if let Ok(runtime) = tokio::runtime::Handle::try_current() {",
     "        if let Some(runtime) = None::<tokio::runtime::Handle> {"),
    ("kernel_host_blobs", "finish", "a row no committed unit pins is not noted as pinned",
     "is_some_and(|row| row.pinned),\n                hash: finished.hash,",
     "is_some_and(|_| true),\n                hash: finished.hash,"),
    ("kernel_blobs", "Drop for DirectoryWriter", "an ended writing is no longer held",
     "            held(&self.writing).remove(&self.name);\n            return;",
     "            return;"),
    ("kernel_blobs", "Drop for Abandoned", "a writing let go is no longer held once it is deleted",
     "        held(&self.writing).remove(&self.name);\n    }\n}",
     "    }\n}"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "a directory that refuses its sync refuses the finish",
     "                    .map_err(|error| failed(&format!(\"syncing `{}`\", root.display()), &error))?;",
     "                    .or_else(|_| Ok::<(), BlobError>(()))?;"),
    # **The two awaits of a finish, in their order: the row first.** Named
    # first, a row refused or never answered leaves bytes no row names and the
    # unit let go does not know of.
    ("kernel_host_blobs", "finish", "a finish reads its row before its bytes are named",
     "            let row = store\n"
     "                .blob(named.as_str())\n"
     "                .await\n"
     "                .map_err(|failed| refused(&units::store_failed(&failed)))?;\n"
     "            let finished = blobs\n"
     "                .finish(taken)\n"
     "                .await\n"
     "                .map_err(|failed| refused(&failed.refusal()))?;\n",
     "            let finished = blobs\n"
     "                .finish(taken)\n"
     "                .await\n"
     "                .map_err(|failed| refused(&failed.refusal()))?;\n"
     "            let row = store\n"
     "                .blob(named.as_str())\n"
     "                .await\n"
     "                .map_err(|failed| refused(&units::store_failed(&failed)))?;\n"),
    ("kernel_host_blobs", "create", "a blob begun where nothing changes is recorded as refused there",
     "self.refused_here(\"blobs#create\", &refusal)",
     "refused(&refusal)"),
    ("kernel_host_blobs", "write", "a blob written where nothing changes is recorded as refused there",
     "self.refused_here(\"blobs#[method]writer.write\", &refusal)",
     "refused(&refusal)"),
    ("kernel_host_blobs", "finish", "a blob finished where nothing changes is recorded as refused there",
     "self.refused_here(\"blobs#[method]writer.finish\", &refusal)",
     "refused(&refusal)"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "a writer's hash so far is the one its finish names",
     "        Hash(writer.hasher.finalize().to_hex().to_string())\n    }",
     "        Hash::of(&[])\n    }"),
    # --- step 13: the network a plugin reaches ---
    ("kernel_net", "judged", "a request over anything but https is refused",
     "    if parsed.scheme() != \"https\" {",
     "    if parsed.scheme() == \"ftp\" {"),
    ("kernel_net", "host_of", "an address where a name belongs is refused",
     "        Some(Host::Domain(name)) => name.to_owned(),\n",
     "        Some(Host::Domain(name)) => name.to_owned(),\n        Some(Host::Ipv4(address)) => address.to_string(),\n"),
    ("kernel_net", "judged", "a user without a password is refused",
     "    if !parsed.username().is_empty() || parsed.password().is_some() {",
     "    if parsed.password().is_some() {"),
    ("kernel_net", "judged", "a port is refused",
     "    if let Some(port) = parsed.port() {",
     "    if let Some(port) = parsed.port().filter(|_| false) {"),
    ("kernel_net", "judged", "a name the manifest does not declare is refused",
     "    if !declares(domains, &host) {",
     "    if false {"),
    ("kernel_net", "judged", "a fragment is never sent",
     "    parsed.set_fragment(None);\n",
     ""),
    ("kernel_net", "is_localhost", "every name under this machine's is refused",
     "    name == \"localhost\" || name.ends_with(\".localhost\")",
     "    name == \"localhost\""),
    ("kernel_net", "is_localhost", "this machine's name written whole is refused",
     "    let name = host.strip_suffix('.').unwrap_or(host);",
     "    let name = host;"),
    ("kernel_net", "declares", "a wildcard declares no name made of a dot alone",
     "|below| below.len() > 1",
     "|below| !below.is_empty()"),
    ("kernel_net", "declares", "a wildcard declares names below it, not names ending in its letters",
     " && below.ends_with('.'))",
     ")"),
    ("kernel_net", "declares", "a name is declared by itself",
     "            None => declared == host,",
     "            None => false,"),
    ("kernel_net", "sendable", "a method that leaves a mark is sent only from a change",
     "    if method.leaves_a_mark() && !in_a_change {",
     "    if false {"),
    ("kernel_net", "leaves_a_mark", "a delete leaves a mark",
     "        matches!(self, Self::Post | Self::Put | Self::Patch | Self::Delete)",
     "        matches!(self, Self::Post | Self::Put | Self::Patch)"),
    ("kernel_net", "named", "a method is named exactly as HTTP writes it",
     "method.as_str() == word)",
     "method.as_str().eq_ignore_ascii_case(word))"),
    ("kernel_net", "not_sent", "every proxy header is refused, not one named so",
     "|_| lower.starts_with(refused))",
     "|_| lower == *refused)"),
    ("kernel_net", "not_sent", "a header the transport writes is refused in any case",
     "    let lower = name.to_ascii_lowercase();",
     "    let lower = name.to_owned();"),
    ("kernel_net", "is_a_token", "a header's name is a token",
     "b\"!#$%&'*+-.^_`|~\".contains(&byte)",
     "b\"!#$%&'*+-.^_`|~: \".contains(&byte)"),
    ("kernel_net", "is_a_value", "a header's value holds no line break",
     "(b' '..=b'~').contains(&byte)",
     "(b'\\n'..=b'~').contains(&byte)"),
    ("kernel_net", "held_to_the_rules", "a request carries no more headers than the catalog allows",
     "    if headers.len() > bound {",
     "    if headers.len() > bound + 1 {"),
    ("kernel_net", "carried", "a get carries no body",
     "    if matches!(method, Method::Get | Method::Head) {",
     "    if matches!(method, Method::Head) {"),
    ("kernel_net", "carried", "a request's body is held to the catalog's bound",
     "unwrap_or(u64::MAX) > KERNEL.net_request_body_bytes {",
     "unwrap_or(u64::MAX) > KERNEL.net_request_body_bytes.saturating_add(1) {"),
    ("kernel_net", "answered", "an answer's body is held to the catalog's bound",
     "unwrap_or(u64::MAX) > KERNEL.net_response_body_bytes {",
     "unwrap_or(u64::MAX) > KERNEL.net_response_body_bytes.saturating_add(1) {"),
    ("kernel_net", "answered", "an answer carries no more headers than the catalog allows",
     "    if response.headers.len() > headers {",
     "    if response.headers.len() > headers + 1 {"),
    ("kernel_net", "failed", "a name resolving to no public address is denied",
     "            Reason::PrivateAddress,\n            RefusalKind::Denied,",
     "            Reason::PrivateAddress,\n            RefusalKind::Unavailable,"),
    ("kernel_net", "note", "what left the machine is noted by its method",
     "            nc_canon::Value::text(request.method.as_str().to_owned()),",
     "            nc_canon::Value::text(\"POST\".to_owned()),"),
    ("kernel_net", "is_public", "a NAT64 address is as public as what it carries",
     "            v4_is_public(Ipv4Addr::new(a, b, c, d))",
     "            true"),
    ("kernel_net", "v4_is_public", "every IPv4 block is refused",
     "    !V4_NON_PUBLIC.iter().any(|block| v4_in(ip, *block))",
     "    !V4_NON_PUBLIC.iter().skip(1).any(|block| v4_in(ip, *block))"),
    ("kernel_net", "v6_in", "an IPv6 block is as wide as its prefix",
     "    let mask = u128::MAX.checked_shl(128 - u32::from(len)).unwrap_or(0);",
     "    let mask = u128::MAX.checked_shl(127 - u32::from(len)).unwrap_or(0);"),
    ("kernel_net_ureq", "configured", "the environment's proxy is never read",
     "        .proxy(None)\n",
     ""),
    ("kernel_net_ureq", "configured", "a redirect is never followed",
     "        .max_redirects(0)\n",
     ""),
    ("kernel_net_ureq", "configured", "the transport sends https alone",
     "        .https_only(true)\n",
     ""),
    ("kernel_net_ureq", "configured", "a 4xx is an answer, not an error",
     "        .http_status_as_error(false)\n",
     ""),
    ("kernel_net_ureq", "configured", "every timeout of a request is set",
     "        .timeout_recv_body(budget)\n",
     ""),
    ("kernel_net_ureq", "configured", "a request names the kernel and no version",
     "        .user_agent(USER_AGENT)\n",
     ""),
    ("kernel_net_ureq", "new", "the transport dials only public addresses",
     "        Self::resolving_with(PublicOnly::<DefaultResolver>::default())",
     "        Self::resolving_with(DefaultResolver::default())"),
    ("kernel_net_ureq", "resolve", "an address off the public internet is never dialled",
     "resolved.iter().filter(|address| is_public(address.ip()))",
     "resolved.iter().filter(|_| true)"),
    ("kernel_net_ureq", "unanswered", "a name resolving to no public address is told so",
     "        return TransportError::PrivateAddress {\n            host: refused.host.clone(),\n        };",
     "        return TransportError::Unavailable(refused.host.clone());"),
    ("kernel_net_ureq", "unanswered", "a body past its bound is too large",
     "        ::ureq::Error::BodyExceedsLimit(_) => TransportError::TooLarge(format!(",
     "        ::ureq::Error::BodyExceedsLimit(_) => TransportError::Unavailable(format!("),
    ("kernel_net_ureq", "unanswered", "a head past its bound is too large",
     "        ::ureq::Error::LargeResponseHeader(_, _) => TransportError::TooLarge(format!(",
     "        ::ureq::Error::LargeResponseHeader(_, _) => TransportError::Unavailable(format!("),
    ("kernel_host_net", "fetch", "a transport that stops is a request that did not complete",
     "                            \"the transport stopped before it answered\".to_owned(),",
     "                            panic!(\"the transport stopped\"),"),
    ("kernel_net_ureq", "answer_of", "a body of exactly the bound is read whole",
     "        .limit(KERNEL.net_response_body_bytes.saturating_add(1))",
     "        .limit(KERNEL.net_response_body_bytes)"),
    ("kernel_net_ureq", "head_of", "a request is sent with the headers the plugin set",
     "        |head, (name, value)| head.header(name.as_str(), value.as_str()),",
     "        |head, _| head,"),
    ("kernel_host_net", "fetch", "a fetch is held to the host call's deadline",
     "        let sent = in_time(\"net#fetch\", self.left(), sending).await?;",
     "        let sent = sending.await?;"),
    ("kernel_host_net", "fetch", "a request that leaves a mark is noted before it is sent",
     "            self.noted(&request).map_err(|refusal| refused(&refusal))?;\n",
     ""),
    ("kernel_host_net", "fetch", "an answer is held to the catalog's bounds again",
     "            .and_then(net::answered)\n",
     ""),
    ("kernel_host_net", "in_a_change", "only a command that changes state sends what leaves a mark",
     "frame.mutating() && frame.tree().unit().is_some()",
     "frame.tree().unit().is_some()"),
    ("kernel_host", "for_plugin", "a plugin fetches from the domains its manifest declares",
     "            manifest.domains().to_vec(),\n",
     "            Vec::new(),\n"),
    ("kernel_host", "refused_fetch", "a refused fetch is recorded for the log",
     "            self.fetches_refused.push((reason, refused.refusal.code()));\n",
     ""),
    ("kernel_host", "fetches_refused", "a refused fetch is handed to the log once",
     "            .skip(self.fetches_logged)",
     "            .skip(0)"),
    ("kernel_units", "room_for", "a unit sends no more requests than it may note",
     "        if self.notes() >= bound {",
     "        if self.notes() > bound {"),
    ("kernel_units", "notes", "a request that left the machine takes a note's room",
     "            .saturating_add(self.left.len())",
     "            .saturating_add(0)"),
    ("kernel_units", "notes_kept", "a unit keeps the notes of what left the machine",
     "        notes.extend(self.left.iter().cloned());\n",
     ""),
    ("kernel_dispatch", "left", "a refused fetch is logged by its plugin's slot",
     "        shared.log.record(&nc_log::events::NetRefused {",
     "        drop(&nc_log::events::NetRefused {"),
    # --- step 13: what a test has to hold ---
    ("kernel_net_ureq", "resolve", "of a mixed answer only the public addresses are dialled",
     "resolved.iter().filter(|address| is_public(address.ip()))",
     "resolved.iter().filter(|_| resolved.iter().any(|any| is_public(any.ip())))"),
    ("kernel_net_ureq", "answer_of", "an answer's body is read no further than the catalog's bound",
     "        .limit(KERNEL.net_response_body_bytes.saturating_add(1))",
     "        .limit(u64::MAX - 1)"),
    ("kernel_net_ureq", "sent", "a request is handed on with the body the plugin gave",
     "    head_of(request).body(request.body().map(<[u8]>::to_vec))",
     "    head_of(request).body(None)"),
    ("kernel_net_ureq", "fetch", "a request's body reaches the wire",
     "            Some(body) => self.agent.run(http::Request::from_parts(head, body)),",
     "            Some(_) => self.agent.run(http::Request::from_parts(head, ())),"),
    # --- step 13: what the transport trusts, and the head ---
    ("kernel_net_ureq", "configured", "a server's certificate is always verified",
     "                .disable_verification(false)\n",
     "                .disable_verification(true)\n"),
    ("kernel_net_ureq", "configured", "only the roots compiled in are trusted",
     "                .root_certs(RootCerts::WebPki)\n",
     "                .root_certs(RootCerts::new_with_certs(&[]))\n"),
    ("kernel_net_ureq", "configured", "the name a request asks for is sent to its host",
     "                .use_sni(true)\n",
     "                .use_sni(false)\n"),
    ("kernel_net_ureq", "fetch", "a request http cannot write is told by its part, not as the network",
     "            .map_err(|refused| unwritable(&refused))?\n",
     "            .map_err(|refused| TransportError::Unavailable(refused.to_string()))?\n"),
    ("kernel_net_ureq", "unwritable", "a URL http cannot write is told as the URL",
     "    if refused.is::<http::uri::InvalidUri>() {",
     "    if false {"),
    ("kernel_net_ureq", "unwritable", "a header's value http cannot write is told as a header",
     "        || refused.is::<http::header::InvalidHeaderValue>()\n",
     "        || false\n"),
    ("kernel_net", "within_the_head", "a request's head is held to the catalog's bound",
     "    if head <= bound {",
     "    if head <= bound + 1 {"),
    ("kernel_net", "within_the_head", "a header's name counts toward the head",
     "head.saturating_add(name.len()).saturating_add(value.len())",
     "head.saturating_add(value.len())"),
    ("kernel_net", "within_the_head", "a URL past the bound alone is refused as the URL",
     "    let reason = if sent > bound {",
     "    let reason = if sent > bound.saturating_mul(2) {"),
    ("kernel_net", "judged", "a fragment is not counted in the head",
     "    parsed.set_fragment(None);\n    within_the_head(&parsed, &headers)?;\n",
     "    within_the_head(&parsed, &headers)?;\n    parsed.set_fragment(None);\n"),
    ("kernel_net", "failed", "a URL the transport cannot write is the plugin's to change",
     "            refused(Reason::Url, RefusalKind::Invalid, error.code(), sentence)",
     "            refused(Reason::Url, RefusalKind::Unavailable, error.code(), sentence)"),
    ("kernel_net", "failed", "a header the transport cannot write is the plugin's to change",
     "            refused(Reason::Header, RefusalKind::Invalid, error.code(), sentence)",
     "            refused(Reason::Header, RefusalKind::Unavailable, error.code(), sentence)"),
    ("kernel_net", "code", "a request the transport cannot write is refused, not unavailable",
     "            Self::PrivateAddress { .. } | Self::UnwritableUrl(_) | Self::UnwritableHeader(_) => {\n                ErrorCode::PluginRequestRefused\n",
     "            Self::PrivateAddress { .. } => ErrorCode::PluginRequestRefused,\n            Self::UnwritableUrl(_) | Self::UnwritableHeader(_) => {\n                ErrorCode::PluginNetworkUnavailable\n"),
    # --- step 13: the answer's headers, the reasons' words, a read's note ---
    ("kernel_net_ureq", "unanswered", "an answer with more headers than ureq parses is too large",
     "            TransportError::TooLarge(format!(\n                \"an answer carries at most {} header(s), and this one carried more\",",
     "            TransportError::Unavailable(format!(\n                \"an answer carries at most {} header(s), and this one carried more\","),
    ("kernel_net", "variant", "each reason is logged by its own word",
     "            Self::PrivateAddress => nc_log::variant!(\"private_address\"),\n            Self::TooLarge => nc_log::variant!(\"too_large\"),",
     "            Self::PrivateAddress => nc_log::variant!(\"too_large\"),\n            Self::TooLarge => nc_log::variant!(\"private_address\"),"),
    ("kernel_host_net", "fetch", "a read sent from a change leaves no note",
     "        let marked = request\n            .method()\n            .leaves_a_mark()\n            .then(|| net::note(&request));",
     "        let marked = (request.method().leaves_a_mark() || self.in_a_change())\n            .then(|| net::note(&request));"),
    ("kernel_host_net", "fetch", "a request the transport refused unwritten is taken back out of its unit",
     "            self.unnoted(note);\n",
     ""),
    ("kernel_host_net", "fetch", "a request answered too large keeps its note",
     "            && error.sent_nothing()\n",
     "            && !matches!(error, net::TransportError::Unavailable(_))\n"),
    # --- step 14: units of work, undo, redo and compensation ---
    ("kernel_history", "undo", "an undo reaches the newest step still committed",
     "        .find(|row| row.meta.kind == UnitKind::Step && row.state == UnitState::Committed)",
     "        .find(|row| row.meta.kind == UnitKind::Step)"),
    ("kernel_history", "undo", "a step that left the machine with nothing to set it right is a barrier",
     "    if notes.left > 0 && notes.compensations.is_empty() {",
     "    if false {"),
    ("kernel_history", "undo", "a step that left the machine with a compensation is no barrier",
     "    if notes.left > 0 && notes.compensations.is_empty() {",
     "    if notes.left > 0 {"),
    ("kernel_history", "recorded", "only a step the journal records as kept is reversed",
     "            Ok(Some(Outcome::UnitCommitted)) => {}",
     "            Ok(_) => {}"),
    ("kernel_history", "recorded", "a journal that cannot say how a unit closed refuses the history",
     "        match journal.store().unit_closing(&shared.realm, *unit).await {",
     "        match journal\n            .store()\n            .unit_closing(&shared.realm, *unit)\n            .await\n            .or(Ok::<_, nc_journal::Error>(Some(Outcome::UnitCommitted)))\n        {"),
    ("kernel_history", "authorized_now", "an undo authorizes again every action its step was allowed",
     "    for (action, entity) in authorized {",
     "    for (action, entity) in authorized.iter().take(0) {"),
    ("kernel_history", "held_as_left", "a key changed since is a conflict before anything is staged",
     "        if held.as_ref().map(|entry| entry.value.as_str()) != expected {",
     "        if held.is_none() && expected.is_some() && false {"),
    ("kernel_history", "staged", "a key is put back to what it held before",
     "            value_of(image.after.as_ref()),\n            value_of(image.before.as_ref()),\n        );",
     "            value_of(image.after.as_ref()),\n            value_of(image.after.as_ref()),\n        );"),
    ("kernel_history", "unpinned", "a pin put on before the step is not its undo's to take off",
     "        if *pinned_before && holds(shared, row.created_unit).await? {",
     "        if false {"),
    ("kernel_history", "unpinned", "a blob another step holds keeps its pin",
     "        if held_elsewhere {",
     "        if false {"),
    ("kernel_history", "undo", "the compensation registered last runs first",
     "    for (command, args, _, _) in notes.compensations.iter().rev() {",
     "    for (command, args, _, _) in &notes.compensations {"),
    ("kernel_history", "newest_undo", "new work ends the way back",
     "            UnitKind::Step if row.state == UnitState::Committed => return Ok(None),",
     "            UnitKind::Step if false => return Ok(None),"),
    ("kernel_history", "newest_undo", "the kernel's own reversal is no undo a redo puts back",
     "                if row.state == UnitState::Committed && row.meta.label != units::REVERSAL_LABEL =>",
     "                if row.state == UnitState::Committed =>"),
    ("kernel_history", "redo", "a redo after compensations is refused",
     "    if undone.compensated {",
     "    if false {"),
    ("kernel_history", "redo", "a redo asks the journal about the undo it reverses",
     "    recorded(shared, &[step.row.unit, undo.row.unit]).await?;",
     "    recorded(shared, &[step.row.unit]).await?;"),
    ("kernel_history", "repinned", "a redo whose blob lost its bytes is refused",
     "        if row.is_none() || bytes.is_none() {",
     "        if row.is_none() {"),
    ("kernel_history", "repinned", "a redo whose blob lost its row is refused",
     "        if row.is_none() || bytes.is_none() {",
     "        if bytes.is_none() {"),
    ("kernel_history", "recorded_of", "a redo counts the blobs it pinned again",
     "    let blobs = if reversing.undoes() {",
     "    let blobs = if true {"),
    ("kernel_history", "read", "a note with a field beside its own is not the kernel's",
     "            Some(note) if fields.is_empty() => Ok(note),",
     "            Some(note) => Ok(note),"),
    ("kernel_history", "leaves_the_machine", "a command that cannot be undone leaves the machine",
     "    command.reversibility == Reversibility::Irreversible\n        ||",
     "    false\n        ||"),
    ("kernel_history", "leaves_the_machine", "only the kernel's own change leaves as a command",
     "        || (command.owner == crate::registry::Owner::Kernel\n            && command.mutating",
     "        || (true\n            && command.mutating"),
    ("kernel_history", "leaves_the_machine", "a read the kernel runs leaves nothing",
     "            && command.mutating\n            && !is_history(&command.name))",
     "            && !is_history(&command.name))"),
    ("kernel_history", "leaves_the_machine", "undo and redo do not leave the machine",
     "            && !is_history(&command.name))",
     "            && true)"),
    ("kernel_history", "compensation", "a compensation that changes nothing is refused",
     "    if !command.listed().mutating {",
     "    if false {"),
    ("kernel_history", "compensation", "a compensation that cannot be undone is refused",
     "    if command.listed().reversibility == Reversibility::Irreversible {",
     "    if false {"),
    ("kernel_history", "compensation", "undo and redo compensate nothing",
     "    if is_history(name) {",
     "    if false {"),
    ("kernel_history", "compensation", "a compensation's arguments are held to its command's schema",
     "    let (canonical, _, _) = dispatch::understood(shared, &command, name, args)?;",
     "    let canonical = args.to_owned();"),
    ("kernel_units", "authorized", "an action allowed is noted once",
     "        if self.authorized.contains(&note) {\n            return Ok(());\n        }\n",
     ""),
    ("kernel_units", "compensate", "compensations are kept in the order registered",
     "        self.compensations.push(note);",
     "        self.compensations.insert(0, note);"),
    ("kernel_units", "notes_kept", "a unit keeps the actions it was allowed",
     "        notes.extend(self.authorized.iter().cloned());\n",
     ""),
    ("kernel_units", "notes_kept", "a unit keeps the compensations registered in it",
     "        notes.extend(self.compensations.iter().cloned());\n",
     ""),
    ("kernel_units", "recorded", "an undo that dispatched compensations says so",
     "        if self.compensations > 0 {",
     "        if false {"),
    ("kernel_units", "recorded", "an undo notes the pins it took off",
     "            .unpinned\n            .iter()\n            .map(|(hash, size)| {",
     "            .unpinned\n            .iter()\n            .take(0)\n            .map(|(hash, size)| {"),
    ("kernel_units", "recorded", "a redo notes the pins it put back",
     "        notes.extend(self.repinned.iter().map(|blob| {",
     "        notes.extend(self.repinned.iter().take(0).map(|blob| {"),
    ("kernel_units", "close", "an undo is kept as an undo and a redo as a redo",
     "            kind: reversing.map_or(UnitKind::Step, |reversing| reversing.kind),",
     "            kind: UnitKind::Step,"),
    ("kernel_units", "close", "an undo moves its step's state in its own transaction",
     "        marks: reversing.map_or_else(Vec::new, |reversing| reversing.marks.clone()),",
     "        marks: Vec::new(),"),
    ("kernel_units", "close", "an undo takes the pins off in its own transaction",
     "            .flat_map(|reversing| reversing.unpinned.iter().map(|(hash, _)| hash.clone()))",
     "            .flat_map(|reversing| reversing.unpinned.iter().take(0).map(|(hash, _)| hash.clone()))"),
    ("kernel_units", "pinned_by", "a redo pins its blobs again",
     "            .flat_map(|reversing| reversing.repinned.clone()),",
     "            .flat_map(|reversing| reversing.repinned.iter().take(0).cloned()),"),
    ("kernel_units", "refused_commit", "a key changed under an undo is the history's conflict",
     "        CommitError::Conflict { plugin, key } if reversing => Refusal::new(",
     "        CommitError::Conflict { plugin, key } if false => Refusal::new("),
    ("kernel_units", "reversed_in_the_log", "a redo is logged as a redo",
     "        direction: if reversing.undoes() {",
     "        direction: if true {"),
    ("kernel_units", "close", "a step reversed is logged once it is kept",
     "    let Some((error, said)) = closed_in_the_journal(shared, unit, waits).await else {\n        released(shared, unit).await;\n        reversed_in_the_log(shared, reversing);\n",
     "    let Some((error, said)) = closed_in_the_journal(shared, unit, waits).await else {\n        released(shared, unit).await;\n"),
    ("kernel_units", "reversal", "the kernel's reversal puts back the states its unit moved",
     "    marks.extend(restored);",
     "    marks.extend(restored.into_iter().take(0));"),
    ("kernel_units", "reversal", "the kernel's reversal of an undo pins back what it took off",
     "        blobs: moved.unpinned,",
     "        blobs: Vec::new(),"),
    ("kernel_units", "reversal", "the kernel's reversal of a redo takes off the pins it put back",
     "    unpinned.extend(moved.repinned);",
     "    unpinned.extend(moved.repinned.into_iter().take(0));"),
    ("kernel_units", "restored", "the kernel's reversal of an undo commits its step again",
     "        UnitKind::Undo { target } => Ok(vec![(target, UnitState::Committed)]),",
     "        UnitKind::Undo { .. } => Ok(Vec::new()),"),
    ("kernel_units", "restored", "the kernel's reversal of a redo undoes its step again by the same undo",
     "                    (step, UnitState::Undone { by: undo }),\n",
     ""),
    ("kernel_units", "restore", "a key put back is held to the value the step left",
     "        self.staged_as(unit, (plugin, key), Expect::Value(holding), replaced, after)",
     "        self.staged_as(unit, (plugin, key), Expect::Anything, replaced, after)"),
    ("kernel_units", "doom_for", "a unit is doomed for the cause first given",
     "        if self.doomed.is_none() {",
     "        if true {"),
    ("kernel_dispatch", "registered", "undo and redo are refused inside another command",
     "    if depth > 0 && history::is_history(name) {",
     "    if false {"),
    ("kernel_units", "leaving", "a command that cannot be undone is noted as leaving the machine",
     "        if crate::history::leaves_the_machine(command) {",
     "        if false {"),
    ("kernel_dispatch", "executed", "a command that ran is noted with what it was allowed",
     "                Executed::Ran(Ok(_)) | Executed::Entered(Ok(_)) if mutating => {",
     "                Executed::Ran(Ok(_)) | Executed::Entered(Ok(_)) if false => {"),
    ("kernel_dispatch", "journaled", "an undo's entry names its step and what it put back",
     "    let reversed = if ran.is_ok() && history::is_history(&asked.name) {",
     "    let reversed = if false {"),
    ("kernel_dispatch", "closed", "a unit a plugin let go is logged for that",
     "            units::let_go(shared, &ambient, doomed_for).await;",
     "            units::let_go(shared, &ambient, Cause::Nested).await;"),
    ("kernel_dispatch", "reauthorized", "an action denied now is journaled",
     "    Err(turned_away(shared, tree, turned, Waits::TOP).await)",
     "    Err(turned.refusal)"),
    ("kernel_dispatch", "authorize", "an undo is decided with what its caller was issued",
     "        decided,\n        held,\n    })",
     "        decided,\n        held: history::Held::default(),\n    })"),
    ("kernel_host_commands", "begins_here", "a unit is begun only in a command that changes state",
     "            Some(frame) if frame.mutating() && frame.tree().unit().is_some() => {",
     "            Some(frame) if frame.tree().unit().is_some() => {"),
    ("kernel_host_commands", "begun", "a plugin holds no more units than the catalog allows",
     "        if u64::try_from(self.held.len()).unwrap_or(u64::MAX) >= u64::from(KERNEL.unit_handles) {",
     "        if u64::try_from(self.held.len()).unwrap_or(u64::MAX) >= u64::MAX {"),
    ("kernel_host_commands", "found", "a unit that ended is told so, not trapped",
     "            None if handle < self.next => Found::Ended,",
     "            None if false => Found::Ended,"),
    ("kernel_host_commands", "ended", "a unit committed and let go dooms nothing",
     "        self.held.remove(&handle) == Some(Held::Open)",
     "        self.held.remove(&handle).is_some()"),
    ("kernel_host_commands", "ended_with_the_command", "a command returning with its units committed dooms nothing",
     "        let open = self.held.values().any(|held| *held == Held::Open);",
     "        let open = !self.held.is_empty();"),
    ("kernel_host_commands", "commit", "a unit committed is not let go uncommitted",
     "                self.units.held.insert(unit.rep(), Held::Committed);\n",
     ""),
    ("kernel_host_commands", "abort", "an abort is logged as one",
     "                Cause::Explicit,",
     "                Cause::Dropped,"),
    ("kernel_host_commands", "drop", "a unit let go without its commit dooms the tree",
     "        self.release(Capability::CommandsBegin)?;\n        if self.units.ended(unit.rep()) {",
     "        self.release(Capability::CommandsBegin)?;\n        if false {"),
    ("kernel_host_commands", "compensate_with", "a compensation is kept in the tree's unit",
     ".map(|unit| unit.compensate(note))",
     ".map(|_| Ok(drop(note)))"),
    ("kernel_host", "leave", "a unit left open when its command returns dooms the tree",
     "        if self.units.ended_with_the_command() {",
     "        if false {"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a unit's own note is not another's in memory",
     "            detail.row.unit != except",
     "            true"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "an undo still committed holds its notes in memory",
     "                && detail.row.state == UnitState::Committed\n",
     "                && detail.row.meta.kind == nc_local::UnitKind::Step\n                && detail.row.state == UnitState::Committed\n"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a step undone holds no note in memory",
     "                && detail.row.state == UnitState::Committed\n",
     ""),
    ("local_store", "noted_elsewhere", "a step undone holds no note in the file",
     "             AND holder.state = 'committed')",
     "             AND 1 = 1)"),
    ("local_store", "noted_elsewhere", "an undo still committed holds its notes in the file",
     "             AND holder.state = 'committed')",
     "             AND holder.kind = 'step' AND holder.state = 'committed')"),
    ("reference_behaviour", "compensated", "a request's compensation is its plugin's own note",
     "        .unwrap_or_else(|| format!(\"{namespace}.note.put\"));",
     "        .unwrap_or_else(|| format!(\"{namespace}.note.get\"));"),
    ("reference_behaviour", "compensated", "a second compensation is registered when one is told",
     "        registered = imports.compensate_with(unit, &compensation, &then);",
     "        registered = Ok(drop(then));"),
    ("reference_behaviour", "in_a_unit", "a unit told to commit is committed",
     "        \"unit.commit\" => imports.commit(unit)?,",
     "        \"unit.commit\" => imports.release(unit),"),
    ("kernel_units", "labelled", "a step is shown as the first label a plugin gave it",
     "        if self.label.is_some() {\n            return;\n        }\n",
     ""),
    ("kernel_units", "labelled", "a label's characters that show nothing are replaced",
     "            let character = if nc_schema::unseen(character) {",
     "            let character = if false {"),
    ("kernel_units", "labelled", "a label is cut to its bound",
     "            if shown.len() + character.len_utf8() > LABEL_BYTES {",
     "            if shown.len() > usize::MAX - 8 {"),
    ("kernel_units", "labelled", "an empty label names nothing",
     "        if !shown.trim().is_empty() {",
     "        if true {"),
    ("kernel_units", "shown_as", "an undo is shown as one whatever its compensations labelled",
     "        (None, Some(label)) => label.to_owned(),",
     "        (_, Some(label)) => label.to_owned(),"),
    ("kernel_units", "close", "a step keeps the label its plugin gave it",
     "            label: shown_as(&ambient, label),",
     "            label: label.to_owned(),"),
    ("kernel_host_commands", "begin", "a unit begun gives the step its label",
     "            unit.labelled(&label);\n",
     ""),
    # --- step 14: what an undo and a redo decide again, and before what ---
    ("kernel_dispatch", "executed", "a nested command that ran is noted with what it was allowed",
     "                Executed::Ran(Ok(_)) | Executed::Entered(Ok(_)) if mutating => {",
     "                Executed::Ran(Ok(_)) | Executed::Entered(Ok(_)) if mutating && asked.depth == 0 => {"),
    ("kernel_history", "redo", "a redo authorizes again every action its step was allowed",
     "    authorized_now(shared, tree, REDO, principal, &notes.authorized, held).await?;",
     "    let _ = (&notes.authorized, held);"),
    ("kernel_history", "redo", "a redo holds every key to what its undo left before anything is staged",
     "    held_as_left(shared, &undo.images).await?;",
     "    let _ = &undo.images;"),
    # --- step 14: what a compensation may name ---
    ("kernel_host_commands", "compensate_with", "a compensation naming another's command asks for the grant to dispatch",
     "        if kernel.foreign_to(&name, &self.plugin) {",
     "        if false {"),
    ("kernel", "foreign_to", "another plugin's command is foreign to a plugin",
     "                    .is_none_or(|loaded| loaded.manifest.id() != plugin),",
     "                    .is_none(),"),
    ("kernel", "foreign_to", "a plugin's own command is not foreign to it",
     "                    .is_none_or(|loaded| loaded.manifest.id() != plugin),",
     "                    .is_some(),"),
    ("kernel", "foreign_to", "the kernel's command is foreign to every plugin",
     "                registry::Owner::Kernel => true,",
     "                registry::Owner::Kernel => false,"),
    ("kernel_history", "compensation", "a compensation of the kernel's own is refused",
     "    if command.listed().owner == crate::registry::Owner::Kernel {",
     "    if false {"),
    ("kernel_history", "compensation", "a compensation that deletes is refused",
     "    if command.listed().action == DELETE {",
     "    if false {"),
    # --- step 14: how far back an undo looks ---
    ("kernel_history", "undo", "an undo looks through the person's steps, not every unit",
     "    let rows = steps_of(shared, principal).await?;",
     "    let rows = units_of(shared, principal).await?;"),
    ("local_store", "steps", "a person's steps in the file are the rows of a step's kind",
     "AND principal = ?2 AND kind = 'step'",
     "AND principal = ?2"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a person's steps in memory are the units of a step's kind",
     "                    && detail.row.meta.principal == *principal\n                    && detail.row.meta.kind == UnitKind::Step\n",
     "                    && detail.row.meta.principal == *principal\n"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "a person's steps are listed newest first in memory",
     "        let rows: Vec<UnitRow> = held\n            .units\n            .values()\n            .rev()",
     "        let rows: Vec<UnitRow> = held\n            .units\n            .values()\n            .skip(0)"),
    # --- step 14: who holds a blob, the undo a compensation let go, what an abort names ---
    ("kernel_history", "redo", "a redo pins again every blob its step made, held before it or not",
     "    let repinned = repinned(shared, &notes.made).await?;",
     "    let repinned = repinned(\n        shared,\n        &notes\n            .made\n            .iter()\n            .filter(|(_, pinned_before)| !pinned_before)\n            .cloned()\n            .collect::<Vec<_>>(),\n    )\n    .await?;"),
    ("kernel_history", "repinned", "a redo records as pinned again only a blob no unit held when it ran",
     "        if let Some(row) = row.filter(|row| !row.pinned) {",
     "        if let Some(row) = row {"),
    ("kernel_history", "unpinned", "a blob held before the step loses its pin once its holders are undone",
     "        if *pinned_before && holds(shared, row.created_unit).await? {",
     "        if *pinned_before {"),
    ("kernel_history", "holds", "only a unit still committed holds the row it first made",
     "        .is_none_or(|held| held.row.state == UnitState::Committed))",
     "        .is_none_or(|_| true))"),
    ("kernel_history", "held_elsewhere", "a blob noted as held before is held as much as one noted as not",
     "    for noted in [blobs::noted(hash, false), blobs::noted(hash, true)] {",
     "    for noted in [blobs::noted(hash, false)] {"),
    ("kernel_history", "undo", "a compensation that let the undo's unit go stops the ones after it",
     "        if let Some(doomed) = doomed {",
     "        if let Some(doomed) = doomed.filter(|_| false) {"),
    ("kernel_units", "reversal", "a redo's reversal leaves the pin another unit holds",
     "    unpinned.retain(|hash| !held.contains(hash));",
     "    unpinned.retain(|_| true);"),
    ("kernel_units", "still_held", "a redo's reversal asks who holds a blob besides the step it undoes again",
     "        .find_map(|(unit, state)| matches!(state, UnitState::Undone { .. }).then_some(*unit));",
     "        .find_map(|(unit, state)| matches!(state, UnitState::Committed).then_some(*unit));"),
    ("kernel_units", "not_run", "a unit not kept names the compensations registered in it",
     "    if registered.is_empty() {",
     "    if true {"),
    ("kernel_units", "close", "a unit the store refused names its compensations",
     "        return Err(not_run(\n            refused_commit(&refused, reversing.is_some()),\n            &ambient,\n        ));",
     "        return Err(refused_commit(&refused, reversing.is_some()));"),
    ("kernel_units", "close", "a unit the journal did not take names its compensations",
     "    Err(not_run(\n        Refusal::new(RefusalKind::Unavailable, error.code(), sentence),\n        &ambient,\n    ))",
     "    Err(Refusal::new(RefusalKind::Unavailable, error.code(), sentence))"),
    ("kernel_dispatch", "closed", "a unit that failed names its compensations",
     "            units::let_go(shared, &ambient, cause).await;\n            Err(units::not_run(refusal, &ambient))",
     "            units::let_go(shared, &ambient, cause).await;\n            Err(refusal)"),
    ("kernel_dispatch", "closed", "a unit a nested failure doomed names its compensations",
     "            units::let_go(shared, &ambient, doomed_for).await;\n            Err(units::not_run(refusal, &ambient))",
     "            units::let_go(shared, &ambient, doomed_for).await;\n            Err(refusal)"),
    # --- step 14: a step held to what its undo stages, and the notes no plugin may crowd out ---
    ("kernel_units", "stage", "a key first written counts what it held before against the unit's bytes",
     "        let replaced = committed.map_or(0, |entry| bytes_of(&entry.value));",
     "        let replaced = 0;"),
    ("kernel_units", "restore", "a key put back counts what it holds against the unit's bytes",
     "        let replaced = holding.as_deref().map_or(0, bytes_of);",
     "        let replaced = 0;"),
    ("kernel_units", "ran", "a unit with no room to note what a command was allowed is doomed",
     "            self.doom(refusal);",
     "            drop(refusal);"),
    ("kernel_units", "leaving", "a command that leaves the machine is refused by a unit with no room to note it",
     "            self.left_the_machine(crate::history::left_by(&command.name))",
     "            self.left_the_machine(crate::history::left_by(&command.name)).or(Ok(()))"),
    ("kernel_dispatch", "executed", "a command is asked whether it leaves the machine before it runs",
     "            .map_or(Ok(()), |unit| unit.leaving(asked.command.listed()));",
     "            .map_or(Ok(()), |_| Ok(()));"),
    ("pages", "what_undoing_costs", "a write command the kernel runs on the realm is published as one undo stops at",
     "        crate::model::Reversibility::Write if the_history => {",
     "        crate::model::Reversibility::Write if true => {"),
    ("loader", "check_kernel", "a unit holds one write over the largest value, both values counted",
     "                    .saturating_mul(2)",
     "                    .saturating_mul(1)"),
    ("reference_behaviour", "in_parts", "a note in parts is written to every part",
     "    for part in 0..parts {",
     "    for part in 0..parts.min(1) {"),
    # --- step 15: events, timers, surfaces, the context, the log ring and the pump ---
    ("loader", "check_kernel", "an event queue holds one payload under the longest topic",
     "u64::try_from(nc_schema::descriptor::NAME_MAX).unwrap_or(u64::MAX),",
     "0,"),
    ("kernel_events", "topic_is_well_formed", "a topic is no longer than a command's name",
     "    topic.len() <= TOPIC_MAX\n",
     "    true\n"),
    ("kernel_events", "topic_is_well_formed", "a topic is two words at least",
     "        && topic.split('.').count() >= 2\n",
     "        && topic.split('.').count() >= 1\n"),
    ("kernel_events", "hears", "a prefix hears only the topics below it",
     ".is_some_and(|rest| rest.len() > 1 && rest.starts_with('.'))",
     ".is_some()"),
    ("kernel_events", "judged", "a topic is published only inside its plugin's namespace",
     "        .is_some_and(|rest| rest.starts_with('.'))\n",
     "        .is_some()\n"),
    ("kernel_events", "payload", "an event's data is held to a payload's bound",
     "    if u64::try_from(data.len()).unwrap_or(u64::MAX) > KERNEL.payload_bytes {",
     "    if u64::try_from(data.len()).unwrap_or(u64::MAX) > KERNEL.payload_bytes.saturating_mul(2) {"),
    ("kernel_events", "room", "the queue holds no more events than its bound",
     ">= u64::from(KERNEL.event_queue) {",
     "> u64::from(KERNEL.event_queue) {"),
    ("kernel_events", "room", "the events a unit holds count against the queue",
     "        let events = self.waiting.len().saturating_add(count);",
     "        let events = self.waiting.len();"),
    ("kernel_events", "room", "the bytes a unit holds count against the queue",
     "(count.saturating_add(1), bytes.saturating_add(held.bytes()))",
     "(count.saturating_add(1), bytes)"),
    ("kernel_events", "room", "the queue holds no more bytes than its bound",
     "        if bytes > KERNEL.event_queue_bytes {",
     "        if false {"),
    ("kernel_events", "next", "an event delivered gives back the bytes it held",
     "        self.bytes = self.bytes.saturating_sub(event.bytes());\n",
     ""),
    ("kernel_events", "publish", "an event published in a unit is held until the unit is kept",
     "    tree.later_mut().publish(event);",
     "    crate::dispatch::lock(&shared.events).push(event);"),
    ("kernel_events", "forget", "an unloaded plugin's events are let go",
     "        self.waiting.retain(|event| event.publisher != slot);\n",
     ""),
    ("kernel_timers", "judged", "a repeating timer waits at least the catalog's interval",
     "        KERNEL.timer_min_interval_ms\n",
     "        1\n"),
    ("kernel_timers", "judged", "a timer is set no further ahead than the catalog's delay",
     "    if !(least..=KERNEL.timer_max_delay_ms).contains(&ms) {",
     "    if !(least..=u64::MAX).contains(&ms) {"),
    ("kernel_timers", "judged", "a timer's tag is held to its bound",
     "    if tag.len() > TAG_BYTES {",
     "    if false {"),
    ("kernel_timers", "mint", "a timer's identifier is never handed out twice",
     "        self.minted = self.minted.saturating_add(1);",
     "        self.minted = 1;"),
    ("kernel_timers", "armed_for", "a timer is its owner's alone",
     "            .is_some_and(|timer| timer.owner == owner)\n    }",
     "            .is_some()\n    }"),
    ("kernel_timers", "due", "a timer is due at its millisecond and not after",
     "            .filter(|(_, timer)| timer.due_ms <= now_ms)",
     "            .filter(|(_, timer)| timer.due_ms < now_ms)"),
    ("kernel_timers", "due", "what is due comes oldest due first",
     "        due.sort_unstable();\n",
     ""),
    ("kernel_timers", "fire", "a repeating timer moves on from when it fired",
     "            Some(every) => timer.due_ms = now_ms.saturating_add(every),",
     "            Some(every) => timer.due_ms = timer.due_ms.saturating_add(every),"),
    ("kernel_timers", "fire", "a one-shot timer fires once",
     "                self.armed.remove(&id);\n",
     ""),
    ("kernel_timers", "forget", "an unloaded plugin's timers are cancelled",
     "        self.armed.retain(|_, timer| timer.owner != owner);\n",
     ""),
    ("kernel_timers", "set", "a plugin holds no more timers than the catalog allows",
     "    if u64::try_from(held).unwrap_or(u64::MAX) >= u64::from(KERNEL.timers_per_plugin) {",
     "    if u64::try_from(held).unwrap_or(u64::MAX) > u64::from(KERNEL.timers_per_plugin) {"),
    ("kernel_timers", "set", "the timers a unit holds count against the bound",
     "        .saturating_add(tree.timers_held_for(owner));",
     "        .saturating_add(0);"),
    ("kernel_timers", "set", "a timer set in a unit is armed only when the unit is kept",
     "    tree.later_mut().set_timer(id, timer);",
     "    timers.arm(id, timer);"),
    ("kernel_timers", "set", "a timer set where only reading is allowed fires where only reading is",
     "        may_change: !tree.reading(),",
     "        may_change: true,"),
    ("kernel_timers", "cancel", "a timer cancelled in a unit is cancelled only when the unit is kept",
     "    let timers = lock(&shared.timers);\n    let mut tree = frame.tree();\n    let armed = timers.armed_for(id, owner);\n    if tree.timer_taken_back(id, owner) || (armed && tree.later_mut().cancel_timer(id)) {",
     "    let mut timers = lock(&shared.timers);\n    let mut tree = frame.tree();\n    let armed = timers.armed_for(id, owner);\n    if tree.timer_taken_back(id, owner) || (armed && timers.cancel(id)) {"),
    ("kernel_timers", "cancel", "a cancel naming no timer the plugin holds is not found",
     "    } else {\n        Err(not_held(id))\n    }",
     "    } else {\n        Ok(())\n    }"),
    ("kernel_units", "released", "a unit kept queues the events it held",
     "        queue.push(event);\n",
     "        drop(event);\n"),
    ("kernel_units", "released", "a unit kept arms the timers it held",
     "        timers.arm(id, timer);\n",
     "        drop((id, timer));\n"),
    ("kernel_units", "released", "a unit kept cancels the timers it cancelled",
     "        timers.cancel(id);\n",
     "        drop(id);\n"),
    ("kernel_units", "timers_of", "a unit counts only its plugin's timers",
     "            .filter(|(_, timer)| timer.owner == owner)\n",
     ""),
    ("kernel_units", "unset_timer", "a unit takes back only its plugin's timer",
     ".retain(|(held, timer)| !(*held == id && timer.owner == owner));",
     ".retain(|(held, _)| *held != id);"),
    ("kernel_units", "cancel_timer", "a unit holds each cancel once",
     "        if self.cancelled.contains(&id) {",
     "        if false {"),
    ("kernel_units", "publish", "an event published in a unit is one of its changes",
     "        self.published.push(event);\n        self.held = self.held.saturating_add(1);\n",
     "        self.published.push(event);\n"),
    ("kernel_units", "set_timer", "a timer set in a unit is one of its changes",
     "        self.timers.push((id, timer));\n        self.held = self.held.saturating_add(1);\n",
     "        self.timers.push((id, timer));\n"),
    ("kernel_units", "unset_timer", "a timer taken back from a unit is one of its changes",
     "self.held.saturating_add(u64::from(unset))",
     "self.held.saturating_add(0)"),
    ("kernel_units", "cancel_timer", "a cancel held in a unit is one of its changes",
     "        self.cancelled.push(id);\n        self.held = self.held.saturating_add(1);\n",
     "        self.cancelled.push(id);\n"),
    ("kernel_ui", "node", "a node holds no key its kind does not name",
     "            if key != \"node\"\n                && !required.contains(&key.as_str())",
     "            if false\n                && !required.contains(&key.as_str())"),
    ("kernel_ui", "node", "a node holds every key its kind requires",
     "            if !fields.contains_key(*key) {",
     "            if false {"),
    ("kernel_ui", "KINDS", "a button may be disabled",
     "        &[\"args\", \"bind\", \"disabled\"],",
     "        &[\"args\", \"bind\"],"),
    ("kernel_ui", "key", "a heading's level is one at the least",
     "(\"level\", Value::Integer(level)) if (1..=3).contains(level)",
     "(\"level\", Value::Integer(level)) if (0..=3).contains(level)"),
    ("kernel_ui", "key", "a heading's level is three at the most",
     "(\"level\", Value::Integer(level)) if (1..=3).contains(level)",
     "(\"level\", Value::Integer(level)) if (1..=2).contains(level)"),
    ("kernel_ui", "node", "a tree holds no more nodes than its bound",
     "        if self.nodes > u64::from(KERNEL.ui_tree_nodes) {",
     "        if false {"),
    ("kernel_ui", "node", "a tree nests no deeper than its bound",
     "        if depth > u64::from(KERNEL.ui_tree_depth) {",
     "        if false {"),
    ("kernel_ui", "node", "an interaction is held to the registry",
     "            self.interaction(fields, name, path)?;\n",
     ""),
    ("kernel_ui", "text", "a text is held to its bound",
     "    if u64::try_from(text.len()).unwrap_or(u64::MAX) > KERNEL.ui_text_bytes {",
     "    if false {"),
    ("kernel_ui", "text", "a text shows every character",
     "        .any(|character| character != '\\n' && nc_schema::unseen(character))",
     "        .any(|_| false)"),
    ("kernel_ui", "text", "a line break is a character a text may hold",
     "        .any(|character| character != '\\n' && nc_schema::unseen(character))",
     "        .any(nc_schema::unseen)"),
    ("kernel_ui", "interaction", "an interaction gives only the arguments its command takes",
     "            if !properties.contains(key) {",
     "            if false {"),
    ("kernel_ui", "interaction", "an interaction binds only the arguments its command takes",
     "            if !properties.contains(&bind.as_str()) {",
     "            if false {"),
    ("kernel_ui", "interaction", "an interaction never gives and binds one argument",
     "            if given.contains(&bind.as_str()) {",
     "            if false {"),
    ("kernel_ui", "bound", "a name an interaction binds is an input of the tree",
     "            if !self.inputs.contains(bind) {",
     "            if false {"),
    ("kernel_ui", "input", "each input is named once",
     "        if !self.inputs.insert(name.to_owned()) {",
     "        if !self.inputs.insert(name.to_owned()) && false {"),
    ("kernel_ui", "input", "an input's name is held to its bound",
     "        let shaped = name.len() <= INPUT_NAME_MAX",
     "        let shaped = true"),
    ("kernel_ui", "open", "a plugin holds no more surfaces than the catalog allows",
     "        if u64::try_from(held).unwrap_or(u64::MAX) >= u64::from(KERNEL.surfaces_per_plugin) {",
     "        if u64::try_from(held).unwrap_or(u64::MAX) > u64::from(KERNEL.surfaces_per_plugin) {"),
    ("kernel_ui", "open", "a plugin's surfaces are counted apart from another's",
     "            .filter(|surface| surface.owner == owner)\n",
     ""),
    ("kernel_ui", "holds", "a surface is its owner's alone",
     "            .is_some_and(|surface| surface.owner == owner)",
     "            .is_some()"),
    ("kernel_ui", "render", "a tree is kept only by its surface's owner",
     "            Some(surface) if surface.owner == owner => {",
     "            Some(surface) => {"),
    ("kernel_ui", "forget", "an unloaded plugin's surfaces are closed",
     "        self.open.retain(|_, surface| surface.owner != owner);\n",
     ""),
    ("kernel_ui", "titled", "a surface is opened under a title that is not blank",
     "    if title.trim().is_empty() {",
     "    if false {"),
    ("kernel_ui", "titled", "a surface's title is held to its bound",
     "    if title.chars().count() > most {",
     "    if false {"),
    ("kernel_ui", "titled", "a surface's title shows every character",
     "    if title.chars().any(nc_schema::unseen) {",
     "    if false {"),
    ("kernel_ui", "segment", "a key a refusal names is written as a pointer writes it",
     "            '/' => shown.push_str(\"~1\"),",
     "            '/' => shown.push('/'),"),
    ("kernel_ui", "validated", "a tree is held to a payload's bound",
     "    if u64::try_from(tree.len()).unwrap_or(u64::MAX) > KERNEL.payload_bytes {",
     "    if false {"),
    ("kernel_pump", "fired", "a quarantined plugin's due timers are held",
     "        if shared.stopped(owner) {",
     "        if false {"),
    ("kernel_pump", "fired", "a timer is decided again before it fires",
     "        if again.is_err() {",
     "        if false {"),
    ("kernel_pump", "fired", "a timer refused when it fires is cancelled",
     "            lock(&shared.timers).cancel(id);\n",
     ""),
    ("kernel_pump", "fired", "a timer is decided again under the action it was set under",
     "            &timer.authority,",
     "            &Authority { action: \"Read\".to_owned(), ..timer.authority.clone() },"),
    ("kernel_pump", "pumped", "a pump delivers only the events queued when it began",
     "        let queued = lock(&shared.events).len();",
     "        let queued = usize::MAX;"),
    ("kernel_pump", "pumped", "the timers fire before the events are delivered",
     "        fired(&shared, &mut pumped).await;\n        delivered(&shared, queued, &mut pumped).await;\n",
     "        delivered(&shared, queued, &mut pumped).await;\n        fired(&shared, &mut pumped).await;\n"),
    ("kernel_pump", "delivered", "an event is decided again before it is delivered",
     "        if decided.is_err() {",
     "        if false {"),
    ("kernel_pump", "delivered", "an event is decided again under the action it was published under",
     "            &event.authority,\n            Held::default(),",
     "            &Authority { action: \"Read\".to_owned(), ..event.authority.clone() },\n            Held::default(),"),
    ("kernel_pump", "delivered", "an event a quarantined plugin would hear is dropped",
     "            if shared.stopped(subscriber) {",
     "            if false {"),
    ("kernel_pump", "pruned", "the pump says how many rows the pruner removed",
     "            pumped.pruned = tables.iter().map(|table| table.removed).sum();",
     "            pumped.pruned = 0;"),
    ("kernel_pump", "pruned", "the pump says which code the pruner refused a table with",
     "            pumped.prune_refused = tables.iter().find_map(nc_prune::Pruned::code);",
     "            pumped.prune_refused = None;"),
    ("kernel_pump", "code_of", "a refusal of the pruner is never read as none",
     "        .unwrap_or(ErrorCode::DbPruneRefused)",
     "        .unwrap_or(ErrorCode::DbUnreachable)"),
    ("kernel_pump", "called_back", "a callback's call tree is beneath its frame",
     "    lock(&tree).calls_back(call);\n",
     ""),
    ("kernel_pump", "called_back", "a callback is never confirmed",
     "        authority.clone(),\n        false,\n",
     "        authority.clone(),\n        true,\n"),
    ("kernel_pump", "called_back", "the unit a timer's callback began is kept when it returns",
     "    let unit = lock(&tree).unit_taken();",
     "    let unit: Option<units::Ambient> = None;"),
    ("kernel_pump", "called_back", "a timer's callback may change state only where its timer was set to",
     "        Callback::Timer { may_change, .. } => Call::Timer { may_change },",
     "        Callback::Timer { .. } => Call::Timer { may_change: true },"),
    ("kernel_pump", "called_back", "a timer's callback killed lets the unit it began go",
     "        (false, _) => {\n            units::let_go(shared, &ambient, Cause::Failed).await;\n            false\n        }",
     "        (false, _) => false,"),
    ("kernel_pump", "called_back", "a callback's frame is taken back and what it was refused is logged",
     "    dispatch::left(shared, slot, &mut instance);\n",
     ""),
    ("kernel_dispatch", "lets_changes_beneath", "no command beneath an event's callback changes state",
     "            Self::Event => false,",
     "            Self::Event => true,"),
    ("kernel_dispatch", "lets_changes_beneath", "no command beneath a timer set where only reading was allowed changes state",
     "            Self::Timer { may_change } => may_change,",
     "            Self::Timer { .. } => true,"),
    ("kernel_dispatch", "plugin", "a command's frame is confirmed when its dispatch redeemed a confirmation",
     "        authorized.held.confirmed,",
     "        false,"),
    ("kernel_dispatch", "mutating", "only a command declared to change state may write",
     "        matches!(self.call, Call::Command { mutating: true })",
     "        matches!(self.call, Call::Command { .. })"),
    ("kernel_host_context", "principal", "the principal read is the tree's",
     "        Ok(frame.principal().id().as_str().to_owned())",
     "        Ok(String::new())"),
    ("kernel_host_context", "actor_chain", "the actor chain is empty",
     "        Ok(Vec::new())",
     "        Ok(vec![String::new()])"),
    ("kernel_host_context", "confirmed_by_human", "a confirmation read is the frame's",
     "        Ok(self.in_a_tree(\"context#confirmed-by-human\")?.confirmed())",
     "        Ok(self.in_a_tree(\"context#confirmed-by-human\").is_ok())"),
    ("kernel_host_events", "publish", "a topic is judged before it is published",
     "        crate::events::judged(&topic, &namespace)\n            .map_err(|refusal| self.refused_here(\"events#publish\", &refusal))?;\n",
     ""),
    ("kernel_host_events", "publish", "an event's data is judged before it is published",
     "        let data = crate::events::payload(&data).map_err(|refusal| refused(&refusal))?;\n",
     ""),
    ("kernel_host_events", "publish", "a topic not the plugin's is recorded as refused",
     "self.refused_here(\"events#publish\", &refusal)",
     "refused(&refusal)"),
    ("kernel_host_timers", "every", "a repeating timer is set as one",
     "        self.timer(\"timers#every\", ms, true, tag)",
     "        self.timer(\"timers#every\", ms, false, tag)"),
    ("kernel_host_ui", "render", "a surface another plugin holds is not found whatever is rendered into it",
     "        if !holds {",
     "        if false {"),
    ("kernel_host_ui", "open_surface", "a surface a tree opens is held until the tree releases it",
     "        let id = opened.id().to_owned();\n        tree.later_mut().show(Shown::Opened(opened));",
     "        let id = opened.id().to_owned();\n        lock(&kernel.surfaces).shown(Shown::Opened(opened));"),
    ("kernel_host", "in_a_tree", "a host function acting for a person is refused outside a call tree with its own code",
     "                ErrorCode::PluginOutsideACallTree,",
     "                ErrorCode::PluginReadOnlyFrame,"),
    ("kernel_host", "in_a_tree", "an act for a person where no tree runs is recorded as refused",
     "            self.refused_here(function, &Refusal::new(\n",
     "            refused(&Refusal::new(\n"),
    ("kernel_host", "refused_here", "a call refused for where it ran is recorded once",
     "        if !refused.contains(&seen) {\n",
     "        if seen.0 == seen.0 {\n"),
    ("kernel_host_commands", "begins_here", "a timer's callback begins a unit of its own",
     "                    Ok(Begun::Owned)",
     "                    Ok(Begun::Flattened)"),
    ("kernel_host_commands", "begins_here", "a timer's callback that holds a unit begins into it",
     "                if frame.tree().unit().is_some() {",
     "                if false {"),
    ("kernel_host_commands", "begins_here", "a unit is begun in a timer's callback",
     "            Some(frame) if matches!(frame.call(), Call::Timer { may_change: true }) => {",
     "            Some(frame) if false => {"),
    ("kernel_host_commands", "begins_here", "an event's callback begins no unit",
     "            Some(frame) if matches!(frame.call(), Call::Timer { may_change: true }) => {",
     "            Some(frame) if matches!(frame.call(), Call::Timer { may_change: true } | Call::Event) => {"),
    ("kernel_host_commands", "begins_here", "a timer set where only reading was allowed begins no unit when it fires",
     "            Some(frame) if matches!(frame.call(), Call::Timer { may_change: true }) => {",
     "            Some(frame) if matches!(frame.call(), Call::Timer { .. }) => {"),
    ("kernel_host_commands", "owned", "the unit a timer's callback begins is its call tree's",
     "        frame.tree().begins(crate::units::Ambient::new(unit));\n",
     "        drop(unit);\n"),
    ("kernel", "subscribers", "a plugin hears only the topics it subscribed to",
     "                        .any(|pattern| events::hears(pattern, topic))",
     "                        .any(|_| true)"),
    ("kernel", "subscribers", "an unloaded plugin hears nothing",
     "                !loaded.unloaded()\n                    && loaded",
     "                loaded"),
    ("kernel", "unload", "an unloaded plugin's commands leave the registry",
     "                    plugins.registry.forget(slot);\n",
     ""),
    ("kernel", "unload", "an unloaded plugin's timers are cancelled with it",
     "            dispatch::lock(&self.shared.timers).forget(slot);\n",
     ""),
    ("kernel", "unload", "an unloaded plugin's surfaces are closed with it",
     "            dispatch::lock(&self.shared.surfaces).forget(slot);\n",
     ""),
    ("kernel", "unload", "an unloaded plugin's events are let go with it",
     "            dispatch::lock(&self.shared.events).forget(slot);\n",
     ""),
    ("kernel", "plugin_log", "a plugin's log is read from its ring",
     "        let read = dispatch::lock(&loaded.log).read();",
     "        let read = host::log::Ring::default().read();"),
    ("kernel", "vacant", "an identity unloaded is free again",
     "        let mut held = self.loaded.iter().filter(|loaded| !loaded.unloaded());",
     "        let mut held = self.loaded.iter().filter(|_| true);"),
    ("kernel_supervisor", "status", "an unloaded plugin has no status",
     "            .filter(|loaded| !matches!(loaded.slot, Slot::Unloaded))?;",
     "            .filter(|_| true)?;"),
    ("kernel_supervisor", "restart", "an instance a release started after an unload is dropped",
     "            _ if matches!(loaded.slot, Slot::Unloaded) => return Back::Quarantined,\n",
     ""),
    ("kernel_supervisor", "release", "a release of a plugin unloaded while it started answers as released",
     "    if restart(shared, slot, None).await == Back::Restarted || shared.status(slot).is_none() {",
     "    if restart(shared, slot, None).await == Back::Restarted {"),
    ("kernel_supervisor", "restart", "a restarted plugin writes to the ring its slot keeps",
     "                crate::host::HostState::for_plugin(manifest, shared, Arc::clone(&loaded.log))\n",
     "                crate::host::HostState::for_plugin(manifest, shared, crate::host::log::SharedRing::default())\n"),
    ("kernel_registry", "forget", "a plugin forgotten takes its own commands and no other",
     "            .retain(|_, command| command.listed.owner != Owner::Plugin(slot));",
     "            .retain(|_, _| false);"),
    ("kernel_backend", "StoreBackend for LocalStore", "the local database is pruned by the one pruner",
     "        Some(nc_prune::sqlite::Sqlite::new(self.pool().clone()))",
     "        None"),
    # -- the kernel's own commands, and the recovery on open ------------------
    ("kernel_builtin", "name", "the verification is the catalog's journal.verify",
     "            Self::Verify => VERIFY,",
     "            Self::Verify => PRUNE,"),
    ("kernel_builtin", "named", "every handler is found by its name",
     "        Self::ALL.into_iter().find(",
     "        Self::ALL.into_iter().take(3).find("),
    ("kernel_builtin", "pruned", "a prune is asked for the one table named",
     "    let asked = table.map_or_else(nc_prune::Request::every, nc_prune::Request::just);",
     "    let asked = nc_prune::Request::every();"),
    ("kernel_builtin", "pruned", "a dry run removes nothing",
     "        Ok(tables) if dry_run => Ok(Prune::Answered(answered(&tables))),",
     "        Ok(tables) if false => Ok(Prune::Answered(answered(&tables))),"),
    ("kernel_builtin", "pruned", "a table no catalog bounds is refused before the pruner is asked",
     "    if let Some(refusal) = table.and_then(unbounded) {",
     "    if let Some(refusal) = None::<Failure> {"),
    ("kernel_builtin", "pruned", "a store held in memory is answered with no table",
     "        return Ok(Prune::Answered(answered(&[])));",
     "        return Err(refused(\"\"));"),
    ("kernel_builtin", "unbounded", "a table the catalog bounds is the pruner's to judge",
     "    if nc_catalog::TABLES.iter().any(|known| known.name == table) {",
     "    if nc_catalog::TABLES.iter().any(|known| known.name != table) {"),
    ("kernel_builtin", "unbounded", "a name no catalog bounds is repeated cut to its bound",
     "                crate::registry::quoted_up_to(table, NAMED)",
     "                crate::registry::quoted_up_to(table, usize::MAX)"),
    ("kernel_builtin", "refused", "a table not the local database's is the caller's mistake",
     "        ErrorCode::DbTableNotBounded => RefusalKind::Invalid,",
     "        ErrorCode::DbTableNotBounded => RefusalKind::Internal,"),
    ("kernel_builtin", "refused", "a lock another writer held may pass",
     "        ErrorCode::DbUnreachable | ErrorCode::DbPruneFailed => RefusalKind::Unavailable,",
     "        ErrorCode::DbUnreachable => RefusalKind::Unavailable,"),
    ("kernel_builtin", "answered", "a table left alone says why",
     "            if let Some(why) = &pruned.left_alone {",
     "            if let Some(why) = None::<&nc_prune::Untouched> {"),
    ("kernel_builtin", "answered", "a table left alone carries the code of its refusal",
     "            if let Some(code) = pruned.code() {",
     "            if let Some(code) = None::<ErrorCode> {"),
    ("kernel_builtin", "answered", "a prune says what it removed apart from what it would",
     "                (\"removed\", count(pruned.removed)),",
     "                (\"removed\", count(pruned.would_remove)),"),
    ("kernel_builtin", "left_alone", "a table its writer bounds is said to be",
     "        nc_prune::Untouched::ItsWriterRefusesAtTheLimit => \"bounded_by_its_writer\",",
     "        nc_prune::Untouched::ItsWriterRefusesAtTheLimit => \"left_alone\","),
    ("kernel_dispatch", "builtin", "a prune runs the pruner",
     "            return pruned_once_journaled(shared, tree, asked, authorized).await;",
     "            return Executed::Ran(builtin::verified(shared, &asked.json, asked.waits).await);"),
    ("kernel", "open", "a kernel opens once what was left open is recovered",
     "        units::recover(&kernel.shared)\n            .await\n            .map_err(OpenError::Unrecovered)?;",
     "        drop(units::recover(&kernel.shared));"),
    ("kernel", "open", "a unit left open that was not recovered keeps the kernel from opening",
     "        units::recover(&kernel.shared)\n            .await\n            .map_err(OpenError::Unrecovered)?;",
     "        let _unrecovered = units::recover(&kernel.shared).await;"),
    ("kernel", "load", "what activate was refused is logged once the plugin is loaded",
     "        dispatch::left(&self.shared, slot, &mut activated.instance);\n",
     "        let _ = &mut activated.instance;\n"),
    ("kernel", "grants", "a plugin unloaded grants nothing",
     "            !loaded.unloaded()\n                && loaded.manifest.id() == plugin\n",
     "            loaded.manifest.id() == plugin\n                && loaded.manifest.id() == plugin\n"),
    ("kernel_units", "recover", "recovery asks the journal which of this realm's units are open",
     "        .open_units(&shared.realm)\n        .await\n        .map_err(",
     "        .open_units(&shared.realm)\n        .await\n        .map(|_| Vec::new())\n        .map_err("),
    ("kernel_units", "recover", "a unit the journal shows closed is not recovered",
     "        if closing.is_some() {\n            continue;\n        }",
     "        if false {\n            continue;\n        }"),
    ("kernel_units", "recover", "what the store kept of a unit left open is reverted",
     "        let reverted = held.is_some_and(|detail| detail.row.state == UnitState::Committed);",
     "        let reverted = false;"),
    ("kernel_units", "recover", "a unit the store holds reversed already is only closed",
     "        let reverted = held.is_some_and(|detail| detail.row.state == UnitState::Committed);",
     "        let reverted = held.is_some();"),
    ("kernel_units", "recover", "a reversal the store refused keeps the kernel from opening",
     "        if reverted && let Err(refused) = revert(shared, unit).await {",
     "        if reverted && let Err(refused) = revert(shared, unit).await.or(Ok::<(), Refusal>(())) {"),
    ("kernel_units", "recover", "a reversal the store refused on opening is logged",
     "            shared.log.record(&nc_log::events::KernelRevertFailed {\n                unit,\n                code: refused.code(),\n            });\n            return Err(",
     "            drop(&nc_log::events::KernelRevertFailed {\n                unit,\n                code: refused.code(),\n            });\n            return Err("),
    ("kernel_units", "recover", "a unit left open is closed as aborted",
     "            .close_unit(unit, Closing::Aborted)\n            .await;\n        if let Err(error) = closed",
     "            .close_unit(unit, Closing::Committed)\n            .await;\n        if let Err(error) = closed"),
    ("kernel_units", "recover", "a close the journal did not take keeps the kernel from opening",
     "        if let Err(error) = closed\n            && !matches!(error, nc_journal::Error::UnitClosed { .. })",
     "        if let Err(error) = closed\n            && false"),
    ("kernel_units", "recover", "a unit recovered is logged with its code",
     "        shared.log.record(&nc_log::events::KernelRecovered {",
     "        drop(&nc_log::events::KernelRecovered {"),
    ("memory", "open_units", "a unit another realm opened is not this realm's to recover",
     "            .filter(|entry| &entry.record.realm == realm)\n",
     "            .filter(|_| true)\n"),
    ("memory", "open_units", "an entry that is not a bracket opens nothing",
     "            if record.action == crate::unit::OPENS\n                && record.result == crate::Outcome::UnitBegun\n",
     "            if record.result == crate::Outcome::UnitBegun\n"),
    ("memory", "open_units", "a begin that was not begun opens nothing",
     "            if record.action == crate::unit::OPENS\n                && record.result == crate::Outcome::UnitBegun\n",
     "            if record.action == crate::unit::OPENS\n"),
    ("memory", "open_units", "a unit is named once, at its first opening",
     "                && !opened.contains(&unit)\n            {\n                opened.push(unit);",
     "            {\n                opened.push(unit);"),
    ("memory", "open_units", "a unit closed as aborted is closed",
     "            if record.action == crate::unit::CLOSES\n                && matches!(\n                    record.result,\n                    crate::Outcome::UnitCommitted | crate::Outcome::UnitAborted\n                )",
     "            if record.action == crate::unit::CLOSES\n                && matches!(\n                    record.result,\n                    crate::Outcome::UnitCommitted\n                )"),
    ("memory", "open_units", "only a closing bracket closes a unit",
     "            if record.action == crate::unit::CLOSES\n                && matches!(",
     "            if matches!("),
    ("memory", "open_units", "a unit closed is not left open",
     "        opened.retain(|unit| !closed.contains(unit));",
     "        opened.retain(|_| true);"),
    ("journal_store", "open_units", "PostgreSQL names the open units of the realm asked about alone",
     "WHERE opening.realm = $1 AND opening.action = $2",
     "WHERE $1 = $1 AND opening.action = $2"),
    ("journal_store", "open_units", "PostgreSQL leaves a unit closed out of the open ones",
     "AND closing.action = $4 AND closing.result = ANY($5))",
     "AND closing.action = $4 AND closing.result = ANY($5) AND FALSE)"),
    ("journal_store", "open_units", "PostgreSQL refuses an opening naming its unit as this code never writes one",
     "                    Ok(read) if read.to_crockford() == unit => Ok(read),",
     "                    Ok(read) => Ok(read),"),
    ("local_journal", "append", "an entry lands only where the next one goes",
     "?1 - 1 = coalesce((SELECT max(sequence) FROM main.local_journal_entries), -1)",
     "?1 - 1 = ?1 - 1"),
    ("local_journal", "append", "no entry lands in a unit this realm has closed",
     "AND work_unit = ?12 AND action = ?16 AND result IN (?17, ?18))",
     "AND work_unit = ?12 AND ?16 = ?16 AND result IN (?17, ?18))"),
    ("local_journal", "append", "an append something silenced is refused",
     "        crate::store::write_exactly(Ok(written), 1, \"appending an entry to the journal\")\n            .map_err(from_local)?;\n",
     "        let _ = written;\n"),
    ("local_journal", "append", "a temporary object on the connection stops an append",
     "        crate::schema::no_temporary_object(&mut transaction)\n            .await\n            .map_err(from_local)?;\n",
     "        let _ = &mut transaction;\n"),
    ("local_journal", "why_nothing_landed", "a link that does not follow the tail is told as one",
     "    if entry.previous.map(|hash| nc_canon::to_hex(&hash)) != head {",
     "    if false && entry.previous.map(|hash| nc_canon::to_hex(&hash)) != head {"),
    ("local_journal", "entry_from", "a row whose stored hash is not its contents' is refused",
     "    if entry.hash == stored {",
     "    if true || entry.hash == stored {"),
    ("local_journal", "open_units", "the local journal names the open units of the realm asked about alone",
     "WHERE opening.realm = ?1 AND opening.action = ?2",
     "WHERE ?1 = ?1 AND opening.action = ?2"),
    ("local_journal", "open_units", "an opening naming its unit as this code never writes one is refused, never skipped",
     "        rows.into_iter()\n            .map(|(sequence, unit)| {\n                unit_of(&unit).map_err(",
     "        rows.into_iter()\n            .filter(|(_, unit)| unit_of(unit).is_ok())\n            .map(|(sequence, unit)| {\n                unit_of(&unit).map_err("),
    # -- what the recovery and a prune owe a unit the journal holds open ------
    ("kernel_units", "close", "a unit whose reversal the store refused is held from the pruner",
     "            crate::dispatch::lock(&shared.left_committed).insert(unit);",
     "            let _ = unit;"),
    ("kernel_units", "reverted_again", "a unit the journal cannot say the close of stays held",
     "            })?;\n        if closing.is_none() {",
     "            }).unwrap_or(None);\n        if closing.is_none() {"),
    ("kernel_units", "reverted_again", "a unit the journal says it closed is not reverted",
     "        if closing.is_none() {",
     "        if true {"),
    ("kernel_units", "reverted_again", "a unit reverted again is closed as aborted",
     "            abort(shared, unit, Cause::Journal, Waits::TOP).await;",
     "            let _ = (Cause::Journal, Waits::TOP);"),
    ("kernel_units", "reverted_again", "a store that cannot read a held unit keeps it held",
     "                .map_err(|failed| store_failed(&failed))?;\n            if kept.is_some_and(",
     "                .unwrap_or(None);\n            if kept.is_some_and("),
    ("kernel_units", "reverted_again", "the units held from the pruner are reverted the newest first",
     "        .iter()\n        .rev()\n        .copied()\n        .collect();\n    for unit in held {",
     "        .iter()\n        .copied()\n        .collect();\n    for unit in held {"),
    ("kernel_units", "recover", "a store that cannot read a unit left open keeps the kernel from opening",
     "            .unit(unit)\n            .await\n            .map_err(|failed| store_failed(&failed))?;",
     "            .unit(unit)\n            .await\n            .unwrap_or(None);"),
    ("kernel_units", "recover", "a journal that cannot say how a unit left open closed keeps the kernel from opening",
     "            })?;\n        if closing.is_some() {\n            continue;",
     "            }).unwrap_or(None);\n        if closing.is_some() {\n            continue;"),
    ("kernel_units", "recover", "the units left open are reverted the newest first",
     "    for unit in open.into_iter().rev() {",
     "    for unit in open.into_iter() {"),
    ("kernel_units", "recover", "every unit a stopped process left open is recovered",
     "    for unit in open.into_iter().rev() {",
     "    for unit in open.into_iter().rev().skip(1) {"),
    ("kernel_units", "recover", "a close the journal answers as closed already is the unit's close",
     "        if let Err(error) = closed\n            && !matches!(error, nc_journal::Error::UnitClosed { .. })",
     "        if let Err(error) = closed\n            && true"),
    ("kernel_builtin", "pruned", "a prune that reaches the units waits for every unit the journal holds open",
     "    if reaches_the_units(table)\n        && let Err(held) = crate::units::reverted_again(shared).await",
     "    if false\n        && let Err(held) = crate::units::reverted_again(shared).await"),
    ("kernel_builtin", "pruned", "a dry run of the units waits for the units the journal holds open too",
     "    if reaches_the_units(table)\n        && let Err(held)",
     "    if reaches_the_units(table)\n        && !dry_run\n        && let Err(held)"),
    ("kernel_builtin", "reaches_the_units", "a prune of the units table reaches the units",
     "    table.is_none_or(|name| name == UNITS)",
     "    table.is_none()"),
    ("kernel_builtin", "reaches_the_units", "a prune of another table does not wait on the units",
     "    table.is_none_or(|name| name == UNITS)",
     "    table.is_none_or(|_| true)"),
    ("kernel_builtin", "held_back", "a prune held back carries the code of what refused the reversal",
     "            held.code(),",
     "            ErrorCode::DbPruneRefused,"),
    ("kernel_builtin", "pruned", "a prune is rehearsed before its entry, deleting nothing",
     "    match nc_prune::run(&mut target, asked.dry()).await {",
     "    match nc_prune::run(&mut target, asked).await {"),
    ("kernel_pump", "pruned", "the pump prunes no unit the journal holds open",
     "    if let Err(held) = units::reverted_again(shared).await {",
     "    if let Err(held) = Ok::<(), crate::dispatch::Refusal>(()) {"),
    ("kernel_pump", "pruned", "a pump held back says what refused the reversal",
     "        pumped.prune_refused = Some(held.code());",
     "        pumped.prune_refused = Some(ErrorCode::DbPruneRefused);"),
    # **Clauses each written with the test that kills it first**: a timer's
    # `begin` held to its deadline, the
    # directory to what a tree has left, a confirmation to its whole
    # principal and to a decided dispatch, a refusal logged as its command
    # declares, compensations to what an undo can dispatch, a revision to its
    # key inside a unit, the pins a reversal and an undo leave on, a redo past
    # the pruner, and a host to what a DNS name is.
    ("kernel_host_commands", "owned", "a timer's begin is held to the host call's deadline",
     "        let unit = crate::supervisor::in_time(\"commands#begin\", self.left(), opening).await?;",
     "        let unit = opening.await?;"),
    ("kernel_host_commands", "commit", "a commit on a handle that ended is refused under a code of its own",
     "            Found::Ended => Err(ended(\"[method]unit.commit\")),",
     "            Found::Ended => Err(refused(&Refusal::new(RefusalKind::Invalid, ErrorCode::KernelUnitAborted, String::new()))),"),
    ("kernel_host_commands", "begin", "a unit begun where none may be is recorded as refused there",
     "self.refused_here(\"commands#begin\", &refusal)",
     "refused(&refusal)"),
    ("kernel_host_commands", "dispatch", "a dispatch from activate is recorded under its own name",
     "            return Err(self.refused_here(\n                \"commands#dispatch\",\n",
     "            return Err(self.refused_here(\n                \"commands#begin\",\n"),
    ("kernel_dispatch", "within", "a question of the directory is held to what its tree has left",
     "    let deadline =\n        Duration::from_millis(KERNEL.host_call_deadline_ms).min(supervisor::tree_left(began));\n    tokio::time::timeout(deadline, asked)",
     "    let deadline =\n        Duration::from_millis(KERNEL.host_call_deadline_ms).min(supervisor::tree_left(began) * 1000);\n    tokio::time::timeout(deadline, asked)"),
    ("kernel_dispatch", "bound", "a confirmation is bound to the whole principal",
     "            && binding.principal == *principal",
     "            && binding.principal.id() == principal.id()"),
    ("kernel_dispatch", "reauthorized", "a refusal decided again is logged as its command declares",
     "            .map_or(action != \"Read\", |admitted| admitted.listed().mutating),",
     "            .map_or(action != \"Read\", |_| true),"),
    ("kernel_dispatch", "decision", "a confirmation is redeemed only once the directory has answered",
     "    let realm = shared.realm.as_str();\n    let facts_of_the_realm = within(began, shared.directory.realm(realm))\n        .await\n        .map_err(unavailable)?;\n    let decided = |grant| Decided {\n        pack: pdp.pack().to_owned(),\n        epoch: facts_of_the_realm.epoch,\n        grant,\n    };\n    if !pdp.carries(&facts_of_the_realm) {\n        return Err(Denial {\n            code: ErrorCode::PolicyPackNotCarried,\n            deciding: sentinel::PACK_NOT_CARRIED.to_owned(),\n            why: \"the realm is held to a pack this kernel does not carry\",\n            decided: Some(decided(None)),\n        });\n    }\n    let entity = principal.entity();\n    let grant = within(began, shared.directory.grant(realm, &entity))\n        .await\n        .map_err(unavailable)?;\n    let holding = grant.as_ref().map(|grant| &grant.entity);\n    let slice = within(\n        began,\n        shared.directory.slice(realm, &entity, resource, holding),\n    )\n    .await\n    .map_err(unavailable)?;\n    let (held, bound) = held();\n",
     "    let realm = shared.realm.as_str();\n    let (held, bound) = held();\n    for serial in &bound {\n        lock(&shared.issued).used(*serial);\n    }\n    let facts_of_the_realm = within(began, shared.directory.realm(realm))\n        .await\n        .map_err(unavailable)?;\n    let decided = |grant| Decided {\n        pack: pdp.pack().to_owned(),\n        epoch: facts_of_the_realm.epoch,\n        grant,\n    };\n    if !pdp.carries(&facts_of_the_realm) {\n        return Err(Denial {\n            code: ErrorCode::PolicyPackNotCarried,\n            deciding: sentinel::PACK_NOT_CARRIED.to_owned(),\n            why: \"the realm is held to a pack this kernel does not carry\",\n            decided: Some(decided(None)),\n        });\n    }\n    let entity = principal.entity();\n    let grant = within(began, shared.directory.grant(realm, &entity))\n        .await\n        .map_err(unavailable)?;\n    let holding = grant.as_ref().map(|grant| &grant.entity);\n    let slice = within(\n        began,\n        shared.directory.slice(realm, &entity, resource, holding),\n    )\n    .await\n    .map_err(unavailable)?;\n"),
    ("kernel_dispatch", "decision", "a confirmation is spent only on a decision that was evaluated",
     "    if decision.code != Some(ErrorCode::PolicyEvaluationFailed) {",
     "    if decision.code != Some(ErrorCode::KernelUnconfirmed) {"),
    ("kernel_dispatch", "put_off_taken", "what a tree put off goes with an entry the journal refused",
     "        (!self.entry_refused).then_some(put_off)",
     "        Some(put_off)"),
    ("kernel_dispatch", "top", "a plugin a tree killed is started again before the next tree runs",
     "    let _one_tree_at_a_time = shared.bus.lock().await;\n    supervisor::started_again(shared).await;\n",
     "    let _one_tree_at_a_time = shared.bus.lock().await;\n"),
    ("kernel_dispatch", "pruned_once_journaled", "a prune deletes only once its entry has landed",
     "            match journaled(shared, tree, asked, authorized, Ok(String::new())).await {\n                Ok(_) => Executed::Entered(crate::builtin::deleted(target, run).await),",
     "            let ran = crate::builtin::deleted(target, run).await;\n            match journaled(shared, tree, asked, authorized, Ok(String::new())).await {\n                Ok(_) => Executed::Entered(ran),"),
    ("kernel_dispatch", "on", "a nested dispatch's wait is held to what its tree has left",
     "            Duration::from_millis(KERNEL.host_call_deadline_ms).min(supervisor::tree_left(began));\n        tokio::time::timeout(deadline, wait)",
     "            Duration::from_millis(KERNEL.host_call_deadline_ms).max(supervisor::tree_left(began).min(Duration::ZERO));\n        tokio::time::timeout(deadline, wait)"),
    ("kernel_dispatch", "on", "a dispatch nested in a tree waits on the journal no longer than a host call",
     "        let Some(began) = self.nested_in else {\n            return wait.await;\n        };",
     "        let Some(began) = self.nested_in.filter(|_| false) else {\n            return wait.await;\n        };"),
    ("kernel_dispatch", "bound", "a token is redeemed only for the plugin that held its command when it was issued",
     "            && binding.holder.as_deref() == holder\n",
     ""),
    ("kernel_dispatch", "left", "a call refused for where it ran is logged",
     "    for code in instance.calls_refused() {\n",
     "    for code in instance.calls_refused().into_iter().take(0) {\n"),
    ("kernel_dispatch", "asked", "a write refused for the tree's budget is logged as a write",
     "        turned.mutating = nc_schema::descriptor::command_name_is_well_formed(name)\n",
     "        turned.mutating = !nc_schema::descriptor::command_name_is_well_formed(name)\n"),
    ("kernel_dispatch", "asked", "a read refused for the tree's budget is logged as a read",
     "                .is_some_and(|command| command.listed().mutating);\n        return Err(turned_away(shared, tree, turned, waits).await);",
     "                .is_some_and(|command| command.listed().mutating || name.len() > 0);\n        return Err(turned_away(shared, tree, turned, waits).await);"),
    ("kernel_pump", "pumped", "the pump starts again a plugin a tree killed with no time left",
     "        let _between_trees = shared.bus.lock().await;\n        supervisor::started_again(&shared).await;\n",
     "        let _between_trees = shared.bus.lock().await;\n"),
    ("kernel_pump", "fired", "a plugin a callback's tree killed is started again before the next callback",
     "        };\n        supervisor::started_again(shared).await;\n        if shared.stopped(owner) {",
     "        };\n        if shared.stopped(owner) {"),
    ("kernel_pump", "fired", "a timer's plugin is asked whether it is stopped only once it is started",
     "        supervisor::started_again(shared).await;\n        if shared.stopped(owner) {\n            pumped.suspended = pumped.suspended.saturating_add(1);\n            continue;\n        }\n",
     "        if shared.stopped(owner) {\n            pumped.suspended = pumped.suspended.saturating_add(1);\n            continue;\n        }\n        supervisor::started_again(shared).await;\n"),
    ("kernel_pump", "delivered", "a subscriber a callback's tree killed is started again before it hears an event",
     "            supervisor::started_again(shared).await;\n            if shared.stopped(subscriber) {",
     "            if shared.stopped(subscriber) {"),
    ("kernel_pump", "delivered", "a subscriber is asked whether it is stopped only once it is started",
     "            supervisor::started_again(shared).await;\n            if shared.stopped(subscriber) {\n                pumped.dropped = pumped.dropped.saturating_add(1);\n                continue;\n            }\n",
     "            if shared.stopped(subscriber) {\n                pumped.dropped = pumped.dropped.saturating_add(1);\n                continue;\n            }\n            supervisor::started_again(shared).await;\n"),
    ("kernel_supervisor", "restart", "a restart inside a call tree is held to what the tree has left",
     "                    .within(within);",
     "                    .within(None);"),
    ("kernel_supervisor", "restart", "an activation the tree's deadline stops waits for the next tree and quarantines nothing",
     "                if within.is_some() && error.downcast_ref::<TreeDeadline>().is_some() =>",
     "                if false =>"),
    ("kernel_supervisor", "began", "a call begins with none of the host's time counted",
     "            waited: Duration::ZERO,\n            entered: None,\n",
     "            waited: self.waited,\n            entered: None,\n"),
    ("kernel_supervisor", "restart", "what a restart's activate was refused is logged",
     "        crate::dispatch::left(shared, slot, &mut back.instance);\n",
     "        let _ = &mut back.instance;\n"),
    ("kernel_units", "close", "a unit is committed held from the pruner until its close is heard",
     "    if let Err(refused) = shared.store.commit_held(&record, &quotas).await {",
     "    if let Err(refused) = shared.store.commit(&record, &quotas).await {"),
    ("kernel_units", "close", "a unit whose close is heard is let go of",
     "    let Some((error, said)) = closed_in_the_journal(shared, unit, waits).await else {\n        released(shared, unit).await;\n",
     "    let Some((error, said)) = closed_in_the_journal(shared, unit, waits).await else {\n"),
    ("kernel_units", "close", "a unit reverted at once is let go of",
     "            released(shared, unit).await;\n            abort(shared, unit, cause, waits).await;",
     "            abort(shared, unit, cause, waits).await;"),
    ("kernel_units", "let_go_of_what_closed", "an opening lets go of what its realm closed",
     "        if closing.is_some() {\n            released(shared, holder).await;\n        }",
     "        if false {\n            released(shared, holder).await;\n        }"),
    ("kernel_units", "reverted_again", "a unit reverted again before a prune is let go of",
     "        released(shared, unit).await;\n        crate::dispatch::lock(&shared.left_committed).remove(&unit);",
     "        crate::dispatch::lock(&shared.left_committed).remove(&unit);"),
    ("kernel_units", "revert", "a reversal pins again only a blob the store holds",
     "        if row.is_some() {\n            still_here.push(blob);\n        }",
     "        if true {\n            still_here.push(blob);\n        }"),
    ("kernel_blobs", "sweep", "the sweep leaves a blob a held unit's reversal pins again",
     "            if owed.contains(hash) || !store.forget_blob(hash).await.map_err(read)? {",
     "            if !store.forget_blob(hash).await.map_err(read)? {"),
    ("kernel_blobs", "BlobStore for DirectoryBlobs", "bytes held already are named again by the finish that finds them",
     "                nc_platform::set_modified(&named, std::time::SystemTime::now())",
     "                nc_platform::set_modified(&named, std::time::UNIX_EPOCH)"),
    ("kernel_blobs", "sweep", "a row's bytes go with it only once they are old",
     "            if let Some(hash) = Hash::parse(hash).filter(|hash| old.contains(hash)) {",
     "            if let Some(hash) = Hash::parse(hash) {"),
    ("kernel_blobs", "sweep", "a sweep takes no bytes named less than a grace ago",
     "        .hashes(std::time::Duration::from_millis(\n            KERNEL.blob_orphan_grace_ms,\n        ))",
     "        .hashes(std::time::Duration::ZERO)"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "the memory store holds what an undo marks",
     "            for at in core::iter::once(unit.unit).chain(marked) {",
     "            for at in core::iter::once(unit.unit).chain(marked.take(0)) {"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "the store in memory keeps a held unit as the file does",
     "        if held.holds.iter().any(|(unit, _)| beyond.contains(unit)) {\n",
     "        if held.holds.iter().take(0).any(|(unit, _)| beyond.contains(unit)) {\n"),
    ("kernel_backend", "StoreBackend for MemoryBackend", "the store in memory holds a unit once per holder as the file does",
     "                if !next.holds.insert((at, unit.unit)) {\n",
     "                if !next.holds.insert((at, unit.unit)) && unit.marks.len() > usize::MAX {\n"),
    ("prune_sqlite", "unheld", "a row another table holds is left to the next prune",
     "    Ok(if held.is_empty() {\n        String::new()\n    } else {",
     "    Ok(if held.is_empty() || true {\n        String::new()\n    } else {"),
    ("local_schema", "judge_against", "an object no migration builds is refused by its kind and its name",
     "            .any(|object| object.kind == held.kind && object.name == held.name)\n",
     "            .any(|object| object.name == held.name)\n"),
    ("local_schema", "written_by_this_build", "a migration that did not run to its end is refused before the migrator runs",
     "    if let Some(row) = recorded.iter().find(|row| !row.success) {",
     "    if let Some(row) = recorded.iter().find(|_| false) {"),
    ("local_schema", "written_by_this_build", "a migration edited after it ran is refused before the migrator runs",
     "        .find(|row| this_builds(row.version).is_some_and(|(_, checksum)| *checksum != row.checksum))",
     "        .find(|row| this_builds(row.version).is_some_and(|_| false))"),
    ("local_store", "open", "an open waits for its profile's lock off the threads that poll it",
     "        let (file, _one_open_at_a_time) = crate::open::off_the_runtime(directory).await?;",
     "        let (file, _one_open_at_a_time) = crate::open::private_file_in(directory)?;"),
    ("kernel_net_ureq", "unanswered", "a request that did not complete is told in one sentence of the kernel's",
     "        _ => TransportError::Unavailable(UNANSWERED.to_owned()),",
     "        other => TransportError::Unavailable(format!(\"{UNANSWERED}: {other}\")),"),
    ("kernel_net_ureq", "HEAD_BYTES", "an answer's head is held to its bound on the wire",
     "const HEAD_BYTES: usize = 65_536;",
     "const HEAD_BYTES: usize = 1_048_576;"),
    ("kernel_net_ureq", "INPUT_BYTES", "an answer's head is read inside ureq's input buffer",
     "const INPUT_BYTES: usize = 131_072;",
     "const INPUT_BYTES: usize = 32_768;"),
    ("kernel_net", "a_dns_name", "a host is at most 253 bytes",
     "    labels.len() <= 253\n",
     "    labels.len() <= 254\n"),
    ("kernel_net", "a_dns_name", "no label of a host ends with a hyphen",
     "                && !label.ends_with('-')\n",
     ""),
    ("kernel_units", "compensate", "a unit registers no more compensations than its undo dispatches",
     "        if self.compensations.len() >= most {",
     "        if false {"),
    ("kernel_units", "compensations", "an undo's own dispatch is the first of its tree",
     "    usize::try_from(KERNEL.dispatches_per_tree.saturating_sub(1)).unwrap_or(usize::MAX)",
     "    usize::try_from(KERNEL.dispatches_per_tree.saturating_sub(0)).unwrap_or(usize::MAX)"),
    ("kernel_units", "staged_as", "inside a unit a key keeps the one revision it commits with",
     "            staged.after = after;\n            staged.position\n",
     "            staged.after = after;\n            staged.position + 1\n"),
    ("kernel_units", "holds_what_it_made", "an undo does not unpin a blob its own unit made",
     "        if let Some(reversing) = &mut self.reversing {\n            reversing\n                .unpinned\n",
     "        if let Some(reversing) = self.reversing.as_mut().filter(|_| made.is_empty()) {\n            reversing\n                .unpinned\n"),
    ("kernel_units", "still_held", "a reversal keeps the pin of a blob another unit holds now",
     "        .map(|hash| (hash, detail.row.unit))",
     "        .filter(|_| false)\n        .map(|hash| (hash, detail.row.unit))"),
    ("kernel_units", "recover", "a recovery reverts only a unit of its own realm",
     "        let held = held.filter(|detail| detail.row.meta.realm == shared.realm);\n",
     ""),
    ("kernel_units", "notes", "a reversal's notes count against its unit's bound",
     "            .saturating_add(self.reversing.as_ref().map_or(0, Reversing::noted));",
     "            .saturating_add(self.reversing.as_ref().map_or(0, |_| 0));"),
    ("kernel_units", "KEPT_FOR_ITS_UNDO", "a step keeps room for the notes its undo adds",
     "const KEPT_FOR_ITS_UNDO: u64 = 2;",
     "const KEPT_FOR_ITS_UNDO: u64 = 0;"),
    ("kernel_units", "expected", "a text that is no revision is refused with its own code",
     "            RefusalKind::Invalid,\n            ErrorCode::PluginNotARevision,",
     "            RefusalKind::Invalid,\n            ErrorCode::PluginStoreConflict,"),
    ("kernel_units", "released", "what a tree did to the surfaces is shown once it is released",
     "        for shown in self.shown {\n            surfaces.shown(shown);\n        }",
     "        for shown in self.shown.into_iter().filter(|_| false) {\n            surfaces.shown(shown);\n        }"),
    ("kernel_units", "close", "a unit the journal kept before its answer was lost is let go from the pruner",
     "        Ok(Some(Outcome::UnitCommitted)) => {\n            released(shared, unit).await;\n",
     "        Ok(Some(Outcome::UnitCommitted)) => {\n"),
    ("kernel_units", "close", "an undo the journal kept before its answer was lost is logged as reversing its step",
     "            reversed_in_the_log(shared, reversing);\n            kept(shared, &mut ambient);\n            return Ok(());\n        }\n        Ok(_) => false,",
     "            kept(shared, &mut ambient);\n            return Ok(());\n        }\n        Ok(_) => false,"),
    ("kernel_units", "close", "what a unit the journal kept before its answer was lost put off is released",
     "            kept(shared, &mut ambient);\n            return Ok(());\n        }\n        Ok(_) => false,",
     "            return Ok(());\n        }\n        Ok(_) => false,"),
    ("kernel_units", "quotas", "a plugin's store is held to the catalog's bound on its entries",
     "        entries_per_plugin: max_rows(\"store_entries\"),\n",
     "        entries_per_plugin: max_rows(\"store_entries\").saturating_mul(2),\n"),
    ("kernel_units", "quotas", "a plugin's lowered quota is its own",
     "        lowered,\n        blobs: max_rows(\"blobs\"),\n",
     "        lowered: BTreeMap::new(),\n        blobs: max_rows(\"blobs\"),\n"),
    ("kernel_units", "quotas", "the blobs a unit makes are held to the catalog's bound on the blobs table",
     "        blobs: max_rows(\"blobs\"),\n",
     "        blobs: max_rows(\"blobs\").saturating_mul(2),\n"),
    ("kernel_units", "quotas", "a unit's notes are held to the catalog's bound on the notes table",
     "        notes_per_unit: max_rows(\"store_notes\"),\n",
     "        notes_per_unit: max_rows(\"store_notes\").saturating_mul(2),\n"),
    ("kernel_units", "writes_per_unit", "a unit's writes are held to the catalog's bound on the images table",
     "    max_rows(\"store_writes\")\n}",
     "    max_rows(\"store_writes\").saturating_mul(2)\n}"),
    ("kernel_units", "show", "a render held takes the place of its surface's last",
     "            self.shown.retain(|held| !held.renders_into(*owner, id));\n",
     "            let _ = (owner, id);\n"),
    ("kernel_units", "show", "a close takes away the render held for its surface",
     "        if let Shown::Rendered { owner, id, .. } | Shown::Closed { owner, id } = &shown {\n",
     "        if let Shown::Rendered { owner, id, .. } = &shown {\n"),
    ("kernel_units", "holds_what_it_made", "a reversal keeps the pin of a blob its unit made",
     "                .retain(|(hash, _)| !made.iter().any(|made| made.hash.as_str() == hash));\n",
     "                .retain(|(hash, _)| made.len() < usize::MAX || hash.is_empty());\n"),
    ("kernel_units", "not_sent", "only the newest note of a request refused unwritten is taken back",
     "        if let Some(at) = self.left.iter().rposition(|left| left == note) {\n",
     "        while let Some(at) = self.left.iter().rposition(|left| left == note) {\n"),
    ("kernel_history", "holds", "a maker the pruner took holds what it made for good",
     "        .is_none_or(|held| held.row.state == UnitState::Committed))",
     "        .is_some_and(|held| held.row.state == UnitState::Committed))"),
    ("kernel_history", "newest_undo", "a redo whose step the pruner took is nothing to redo",
     "                let Some(step) = shared\n                    .store\n                    .unit(target)\n                    .await\n                    .map_err(|failed| store_failed(&failed))?\n                else {\n                    return Ok(None);\n                };",
     "                let step = detail_of(shared, target).await?;"),
    ("kernel_history", "held_as_registered", "a compensation is held to the plugin that registered it, not to its name",
     "            .and_then(|admitted| owner_of(shared, &admitted))\n            .as_deref()\n            != Some(owner.as_str())",
     "            .and_then(|admitted| owner_of(shared, &admitted))\n            .as_deref()\n            .is_none_or(|_| owner.is_empty())"),
    ("kernel_history", "held_as_registered", "a compensation of another's is dispatched only for a plugin granted to",
     "    let ungranted = compensations.iter().find(|(_, _, owner, by)| {\n",
     "    let ungranted = compensations.iter().take(0).find(|(_, _, owner, by)| {\n"),
    ("kernel_history", "undo", "an undo says it kept the pin of a blob its compensation made",
     "            unit.holds_what_it_made();\n",
     "            let _ = &unit;\n"),
    ("kernel_net", "host_of", "a host below a wildcard is held to what a DNS name is",
     "    if !a_dns_name(&host) {",
     "    if !a_dns_name(&host) && false {"),
    ("kernel_net", "sent_nothing", "a request that may have reached its host keeps its note",
     "            Self::PrivateAddress { .. } | Self::UnwritableUrl(_) | Self::UnwritableHeader(_)\n        )",
     "            Self::PrivateAddress { .. } | Self::UnwritableUrl(_) | Self::UnwritableHeader(_) | Self::Unavailable(_)\n        )"),
    ("kernel_net", "sent_nothing", "an answer too large was sent for",
     "            Self::PrivateAddress { .. } | Self::UnwritableUrl(_) | Self::UnwritableHeader(_)\n        )",
     "            Self::PrivateAddress { .. } | Self::UnwritableUrl(_) | Self::UnwritableHeader(_) | Self::TooLarge(_)\n        )"),
    # And the local database's: a file judged on a connection that cannot
    # write it, one open of a profile at a time, a hot journal told as a file
    # this build did not leave, and the pin a commit takes off held to its rows.
    ("local_open", "for_judging", "the connection that judges the file is read-only",
     "    connection(file).read_only(true)\n",
     "    connection(file)\n"),
    ("platform_unix", "hold", "one open of a profile at a time",
     "    held.lock().map_err(|error| {",
     "    Ok::<(), std::io::Error>(()).map_err(|error| {"),
    # **The doors other crates' suites stage a path through.** A door that
    # stages nothing leaves the refusal it is there to provoke unprovoked, and
    # the suite that asked for it red.
    ("platform_testing", "open_to_another_user", "a path opened to another user is opened",
     "    imp::open_to_another_user(path)\n",
     "    Ok(path.display().to_string())\n"),
    ("local_store", "unread", "a hot journal is a file this build did not leave",
     ".is_some_and(|code| code == crate::open::HOT_JOURNAL);",
     ".is_some_and(|code| code == \"no such code\");"),
    ("local_store", "unpin", "a pin taken off is held to the rows it changed",
     "        \"unpinning a blob\",\n    )?;\n    Ok(held == 1)",
     "        \"unpinning a blob\",\n    )\n    .or_else(|error| match error {\n        LocalError::Unexpected(_) => Ok(()),\n        other => Err(other),\n    })?;\n    Ok(held == 1)"),
    ("local_store", "judged_before_migrating", "the file's history is held to the checksums this build carries",
     "        .map(|migration| (migration.version, migration.checksum.to_vec()))",
     "        .map(|migration| (migration.version, Vec::new()))"),
    ("local_store", "over", "the store migrates with what the build embedded",
     "        MIGRATOR\n            .run_direct(None, &mut *connection, false)\n            .await\n            .map_err(|error| migrated(&error))?;\n",
     "        let found = sqlx::migrate::Migrator::new(Path::new(\"../../local-migrations\")).await;\n        let migrator = found.map_err(|error| migrated(&error))?;\n        let ran = migrator.run_direct(None, &mut *connection, false).await;\n        ran.map_err(|error| migrated(&error))?;\n"),
    ("local_store", "release", "a release begins with the temporary-object check",
     "            .map_err(|error| said(\"beginning a release\", &error))?;\n        schema::no_temporary_object(&mut transaction).await?;\n",
     "            .map_err(|error| said(\"beginning a release\", &error))?;\n        let _ = &mut transaction;\n"),
    # And the gates': a test-only arm or field read as the one item it is,
    # and the connection's hardening held as a whole by the local SQL gate.
    ("gates", "outside_the_tests", "a test-only arm or field ends at its comma",
     "let ends = trimmed.ends_with('}') || trimmed.ends_with(';') || trimmed.ends_with(',');",
     "let ends = trimmed.ends_with('}') || trimmed.ends_with(';');"),
    ("gates", "outside_the_tests", "a comma inside brackets ends no test-only item",
     "                '(' | '[' => nested += 1,",
     "                '(' | '[' => {}"),
    ("local_gates", "HARDENED", "a journal kept in memory is refused",
     "        keeps: |value| value == \"WAL\",",
     "        keeps: |value| value == \"WAL\" || value == \"MEMORY\","),
    ("local_gates", "HARDENED", "a commit synced below FULL is refused",
     "            Some(level) => level == 2 || level == 3,",
     "            Some(level) => level >= 1,"),
    ("local_gates", "HARDENED", "a schema trusted is refused",
     "        name: \"TRUSTED_SCHEMA\",\n        keeps: falsy,",
     "        name: \"TRUSTED_SCHEMA\",\n        keeps: |value| value != \"ON\","),
    ("local_gates", "HARDENED", "foreign keys turned off are refused",
     "        name: \"FOREIGN_KEYS\",\n        keeps: truthy,",
     "        name: \"FOREIGN_KEYS\",\n        keeps: |value| value != \"OFF\","),
    ("local_gates", "HARDENED", "a commit synced by a word other than FULL or EXTRA is refused",
     "            None => value == \"FULL\" || value == \"EXTRA\",",
     "            None => !matches!(value, \"OFF\" | \"NORMAL\"),"),
    ("local_gates", "HARDENED", "a temporary store on disk is refused",
     "        keeps: |value| numeral(value).map_or(value == \"MEMORY\", |store| store == 2),",
     "        keeps: |_| true,"),
    ("local_gates", "settings", "the store's own synchronous setting is read as a pragma",
     "            \"journal_mode\" | \"synchronous\" | \"foreign_keys\"\n",
     "            \"journal_mode\" | \"foreign_keys\"\n"),
    ("local_gates", "unread_writers", "a member is read only through its whole source tree",
     "            .any(|named| *named == format!(\"{member}/src\"));",
     "            .any(|named| named.starts_with(&format!(\"{member}/\")));"),
    ("local_gates", "vacuums", "VACUUM is refused",
     "        .filter(|(word, _)| word == \"VACUUM\")",
     "        .filter(|_| false)"),
    ("local_gates", "WRITERS", "the pruner's sources are read whole",
     "    \"crates/nc-prune/src\",\n];",
     "    \"crates/nc-prune/src/sqlite.rs\",\n];"),
    ("local_gates", "names_a_directory", "a directory pragma is refused",
     "        .filter(|(word, _)| word == \"TEMP_STORE_DIRECTORY\" || word == \"DATA_STORE_DIRECTORY\")",
     "        .filter(|_| false)"),
    ("local_gates", "pragmas_in", "a sign is read as SQLite's parser reads it",
     "                Some((sign, _)) if sign == \"+\" => {",
     "                Some((sign, _)) if sign == \"+\" && false => {"),
    ("local_gates", "through_the_door", "a door key that is not one pragma is unread",
     "    if found.len() != 1 {\n        found.push(unread);",
     "    if false {\n        found.push(unread);"),
    ("local_gates", "settings", "a setter called as a path is read",
     "        if !(after_a_dot || after_a_path) || !(setter || word == \"pragma\") {",
     "        if !(after_a_dot || (after_a_path && word == \"pragma\")) || !(setter || word == \"pragma\") {"),
    ("local_gates", "settings", "a setter named and not called is unread",
     "        if !matches!(pieces.get(at + 1), Some((Piece::Mark('('), _))) {\n            found.push(PragmaSet {",
     "        if !matches!(pieces.get(at + 1), Some((Piece::Mark('('), _))) {\n            continue;\n        }\n        if false {\n            found.push(PragmaSet {"),
    ("local_gates", "written_out", "a setter's value is read only when it is written out",
     "        [.., Piece::Word(last)] if a_path => Some(last.to_ascii_uppercase()),",
     "        [.., Piece::Word(last)] => Some(last.to_ascii_uppercase()),"),
    ("local_gates", "through_the_door", "the pragma door is read as the statement sqlx sends",
     "    let mut found = pragmas_in(&sent, line);",
     "    let _ = &sent;\n    let mut found = vec![PragmaSet {\n        name: key.trim().to_ascii_uppercase(),\n        value: value.as_deref().map(|it| it.trim().to_ascii_uppercase()),\n        line,\n    }];"),
    ("local_gates", "falsy", "a pragma kept off is allowed only at a value that says off",
     "    matches!(value, \"OFF\" | \"NO\" | \"FALSE\")\n        || (!value.is_empty() && value.bytes().all(|digit| digit == b'0'))",
     "    !truthy(value)"),
    ("local_gates", "rewrites_the_header", "setting schema_version is refused as setting application_id is",
     ".filter(|setting| setting.name == \"APPLICATION_ID\" || setting.name == \"SCHEMA_VERSION\")",
     ".filter(|setting| setting.name == \"APPLICATION_ID\")"),
    ("local_gates", "rewrites_the_history", "an update of the migrations' history is refused as an insert is",
     "matches!(before(1), \"INTO\" | \"UPDATE\")",
     "matches!(before(1), \"INTO\")"),
    ("local_gates", "rewrites_the_history", "a delete from the migrations' history is refused",
     "|| (before(1) == \"FROM\" && before(2) == \"DELETE\")",
     "|| false"),
    ("local_gates", "REFUSED", "a migrator built when the store runs is refused",
     "        holds: migrates_from_elsewhere,\n",
     "        holds: |_, _| Vec::new(),\n"),
    ("local_gates", "migrates_from_elsewhere", "only the one embedded migrator is let through",
     "        .filter(|(_, text)| text.trim() == EMBEDDED)\n",
     "        .filter(|(_, text)| text.contains(\"MIGRATOR\"))\n"),
    # The gate's scratch, where it opens a SQLite file with
    # `create_if_missing`: made private by the call that makes it, and a
    # directory already there refused, never reused and never cleared. The
    # first two are killed by
    # `a_scratch_directory_that_already_exists_is_refused_and_nothing_is_opened_in_it`,
    # the third by `the_scratch_directory_the_gate_makes_is_this_users_alone`.
    ("local_gates", "at", "a scratch directory found is refused, not reused",
     "            Ok(()) => Ok(Self(root)),\n",
     "            Ok(()) => Ok(Self(root)),\n            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && root.is_dir() => Ok(Self(root)),\n"),
    ("local_gates", "at", "a scratch directory found is never cleared",
     "    fn at(root: PathBuf) -> Result<Self, String> {\n",
     "    fn at(root: PathBuf) -> Result<Self, String> {\n        let _ = std::fs::remove_dir_all(&root);\n"),
    ("local_gates", "at", "the gate's scratch directory is private to this user",
     "        match nc_platform::create_private_directory(&root) {",
     "        match std::fs::create_dir(&root) {"),
    ("gates", "pragmas_set", "the pragma door called as a path is read",
     "        let path = at >= 2 && code[at - 2..at] == [':', ':'];",
     "        let path = false;"),
    ("gates", "directly_under_a_member", "a canary directory at any member's root is one",
     "    inside_a_member(relative)\n        .map(|(_, inside)| inside)\n        .or_else(|| relative.strip_prefix(\"xtask/\"))",
     "    relative.strip_prefix(\"xtask/\")"),

    # -- channel: every name an install gives the system -------------------
    ("channel", "digest_of", "the digest starts from FNV-1a's offset basis",
     "    let mut hash: u32 = 0x811c_9dc5;",
     "    let mut hash: u32 = 0x811c_9dc4;"),
    ("channel", "digest_of", "the digest multiplies by FNV's prime",
     "        hash = hash.wrapping_mul(0x0100_0193);",
     "        hash = hash.wrapping_mul(0x0100_0195);"),
    ("channel", "fmt", "a checkout is written with its leading zeros",
     "        write!(formatter, \"{:08x}\", self.digest)",
     "        write!(formatter, \"{:x}\", self.digest)"),
    ("channel", "Checkout", "a checkout's path is canonicalized first",
     "        let path = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());",
     "        let path = root.to_path_buf();"),
    ("channel", "Checkout", "a checkout is eight digits, not at least eight",
     "        let digits = text.len() == 8",
     "        let digits = text.len() >= 8"),
    ("channel", "Checkout", "a checkout's digits are lower-case",
     "                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));",
     "                .all(|byte| byte.is_ascii_hexdigit());"),
    ("channel", "stack", "the log store's port is PostgreSQL's plus one",
     "            logs: postgres + 1,",
     "            logs: postgres + 2,"),
    ("channel", "pair_of", "the block wraps at its last pair",
     "    (digest % u32::from(PORT_PAIRS)) as u16",
     "    (digest % (u32::from(PORT_PAIRS) + 1)) as u16"),
    ("channel", "leader", "staging holds Control as well as Alt",
     "                modifiers: &[Modifier::Control, Modifier::Alt],",
     "                modifiers: &[Modifier::Alt],"),
    ("channel", "Namespace", "a namespace is bounded in length",
     "            && text.len() <= NAMESPACE_MAX",
     "            && text.len() <= NAMESPACE_MAX + 1"),
    ("channel", "Namespace", "a namespace holds no separator",
     "                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());",
     "                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');"),
    ("channel", "Namespace", "a namespace holds a digit",
     "        if !text.bytes().any(|byte| byte.is_ascii_digit()) {",
     "        if false {"),
    ("channel", "plain", "a base holds no control character",
     "        .any(|character| character.is_control() || matches!(character, '\\u{2028}' | '\\u{2029}'))\n",
     "        .any(|character| matches!(character, '\\u{2028}' | '\\u{2029}'))\n"),
    ("channel", "plain", "a base holds no line break that is not a control character",
     "        .any(|character| character.is_control() || matches!(character, '\\u{2028}' | '\\u{2029}'))\n",
     "        .any(|character| character.is_control())\n"),
    ("channel", "plain", "a base is absolute",
     "    if !path.is_absolute() {",
     "    if false {"),
    ("channel", "plain", "a base is spelled as its components spell it",
     "    if spelled.as_os_str() != path.as_os_str()\n",
     "    if false\n"),
    ("channel", "plain", "a base that climbs is refused",
     "            .any(|component| component == Component::ParentDir)",
     "            .any(|_| false)"),
    ("channel", "Install", "a key the manifest does not read is refused",
     "        only_known(&fields, &KEYS, \"\")?;\n",
     ""),
    ("channel", "Install", "a manifest of another format is refused",
     "        if format != FORMAT {",
     "        if false {"),
    ("channel", "Install", "a checkout that is not text is refused",
     "        let checkout = optional_text(&mut fields, \"checkout\")?;",
     "        let checkout = optional_text(&mut fields, \"checkout\").unwrap_or_default();"),
    ("channel", "Install", "a checkout outside a development install is refused",
     "            (\"production\" | \"staging\", Some(_)) => {",
     "            (\"production\" | \"staging\", Some(_)) if false => {"),
    ("channel", "Install", "a data base that is missing or not text is refused",
     "            base(\"data\")?,",
     "            base(\"data\").unwrap_or_default(),"),
    ("channel", "Install", "a configuration base that is missing or not text is refused",
     "            base(\"config\")?,",
     "            base(\"config\").unwrap_or_default(),"),
    ("channel", "Install", "a cache base that is missing or not text is refused",
     "            base(\"cache\")?,",
     "            base(\"cache\").unwrap_or_default(),"),
    ("channel", "Install", "a state base that is missing or not text is refused",
     "            base(\"state\")?,",
     "            base(\"state\").unwrap_or_default(),"),
    ("channel", "Install", "a log endpoint that is not text is refused",
     "        let log_endpoint = optional_text(&mut fields, \"log_endpoint\")?;",
     "        let log_endpoint = optional_text(&mut fields, \"log_endpoint\").unwrap_or_default();"),
    ("channel", "Install", "a development install writes its checkout",
     "            fields.push((\"checkout\", Value::text(checkout.to_string())));",
     "            let _ = checkout;"),
    ("channel", "integer", "a whole number written otherwise is refused",
     "        Some(_) => Err(unreadable(&format!(\"its `{key}` is not a whole number\"))),",
     "        Some(_) => Ok(FORMAT),"),
    ("channel", "quoted", "a refusal escapes what it quotes of a manifest",
     "        .map(nc_canon::shown)\n",
     "        .map(String::from)\n"),
    ("channel", "quoted", "a refusal quotes no more of a manifest than its bound",
     "        .take(QUOTED_AT_MOST)\n",
     "        .take(usize::MAX)\n"),
    ("channel", "layout", "a relocated install's data is its own",
     "    let data = install.bases.data.join(&leaf);",
     "    let data = install.bases.data.join(self::leaf(install.channel, None));"),
    ("channel", "layout", "a relocated install's configuration is its own",
     "    let config = install.bases.config.join(&leaf);",
     "    let config = install.bases.config.join(self::leaf(install.channel, None));"),
    ("channel", "layout", "a relocated install's cache is its own",
     "    let cache = install.bases.cache.join(&leaf);",
     "    let cache = install.bases.cache.join(self::leaf(install.channel, None));"),
    ("channel", "layout", "a relocated install's state is its own",
     "    let state = install.bases.state.join(&leaf);",
     "    let state = install.bases.state.join(self::leaf(install.channel, None));"),
    ("channel", "layout", "the pipe carries the user",
     "        pipe: format!(r\"\\\\.\\pipe\\{leaf}.{user}\"),",
     "        pipe: format!(r\"\\\\.\\pipe\\{leaf}\"),"),
    ("channel", "layout", "a relocated install follows no feed",
     "            Channel::Production | Channel::Staging if !relocated => Some(Update {",
     "            Channel::Production | Channel::Staging => Some(Update {"),
    ("channel", "layout", "a relocated install names no stack",
     "            Channel::Development(checkout) if !relocated => Some(checkout.stack()),",
     "            Channel::Development(checkout) => Some(checkout.stack()),"),
    ("channel", "leaf", "a namespace is a part of its own",
     "        leaf.push('-');",
     "        leaf.push('_');"),
    ("channel", "spelled", "a user keeps its hyphens",
     "        if character.is_ascii_alphanumeric() || character == '-' {",
     "        if character.is_ascii_alphanumeric() {"),
    ("channel", "spelled", "a spelled character opens with `_`",
     "            let _ = write!(out, \"_{:x}_\", u32::from(character));",
     "            let _ = write!(out, \"{:x}_\", u32::from(character));"),
    ("channel", "spelled", "a spelled character closes with `_`",
     "            let _ = write!(out, \"_{:x}_\", u32::from(character));",
     "            let _ = write!(out, \"_{:x}\", u32::from(character));"),
    ("channel", "spelled", "nobody is written as somebody",
     "        out.push('_');\n    }\n    out\n}",
     "    }\n    out\n}"),
]

TOUCH = ["find", "crates", "xtask", "-name", "*.rs", "-exec", "touch", "{}", "+"]
# **Two suites, not the corpus alone.** Measured, of the clauses in `limits.rs`
# that any test decides, **22** were decided only by `limits::tests` and **10**
# by `corpus::` — so with the corpus alone, two thirds of that file's covered
# clauses sit outside every mutation this script runs while it reports them as
# covered by nobody. The corpus is still where a *predicate* case belongs and the
# discipline above stands; a clause pinned by a unit test is not invisible here.
#
# **What a module suite costs, since the question comes up each time one is
# added.** Measured over a short list of sources: five module suites took one
# run from 0.8 seconds of tests to 21.5, which was forty minutes over that
# operator list. The shape of the trade is what survives; the numbers are not
# re-measured here, and the list of which suites are in is not written here
# either — a list in prose names suites as outside that sit in the table below
# it. `SUITES` below is the answer to *which*, and `--summary` prints it per
# file; a second copy in prose can only disagree.
# **And the suite is chosen by the source, not one command for all of them.**
# The paragraph above prices `check::` honestly: the whole of it is 26 seconds
# against 0.8, which over a hundred and thirty operators is an hour rather than
# eight minutes. But an operator is anchored to *one* file, and only the suite
# that decides that file's clauses can kill it — so running `check::` for the
# operators over `check.rs` and not for the rest costs the difference on twelve
# of them and nothing on the others. Measured rather than assumed, and the
# number is here so the next person can make the trade again.
TEST = ["cargo", "test", "-q", "-p", "xtask", "--bin", "xtask", "--"]
SUITES = {
    "check": ["corpus::", "limits::", "check::"],
    # **`gates::` too.**
    # `no_source_puts_the_lints_down`'s `#![` — the characters that tell an
    # inner attribute from a shebang — comes back SURVIVED without it against a
    # fixture that kills it, because that fixture asserts `findings.is_empty()`
    # in `gates::tests` and the corpus test beside it counts only findings of
    # kind "turns off". The weakening changes a *counter* finding instead, so no
    # number in `corpus::` moves however many rows it grows, and no operator
    # over this file can see a clause its own module decides under the default.
    # The same sentence is written below, for `dev` and for `policy`.
    "gates": ["corpus::", "limits::", "gates::"],
    # **And `determinism::`, a row of the same shape.** Judged by `corpus::` and
    # `limits::` alone, which select none of `determinism::tests`, every clause
    # of `mentions`, `conditional_on`, `what_this_line_reads` and the needle
    # census is decided by a module no command judging that file runs. The
    # static rule that makes another row of this shape impossible is in
    # `check.rs`'s meta gate; these rows stay because they are what it holds.
    "determinism": ["corpus::", "limits::", "determinism::"],
    # **`dev::` too, and its absence is measured.** The rule that every `docker`
    # invocation names its project — the one thing standing between a copy of
    # this checkout and the developer's own stack — is decided in `dev::tests`
    # and by nothing in `corpus::` or `limits::`. An exemption that asks only
    # which subcommand ran is satisfied by an `inspect` pointed at a container
    # *name*, and without this row the meta-gate could not weaken that clause
    # either way.
    "dev": ["corpus::", "limits::", "dev::"],
    # **`policy::` *and* `check::`, and the narrow command is measured wrong.**
    # Under the default suite, operators over this file come back SURVIVED
    # against a test that kills them — because that test
    # is in `policy::tests`, which `corpus:: limits::` does not run. `check::` is
    # here for the other half: two of these clauses are held by
    # `every_verdict_is_pinned_to_where_it_is_said`, which pins the source text
    # of the arm a sentence sits in, and that census lives in `check::`.
    #
    # A source carried in behind too narrow a command reports a hole in the
    # script as a hole in the tree. The order of work at the head of this file
    # exists to prevent exactly that, and the answer is to name the suite that
    # decides the file rather than to trust the default.
    "policy": ["corpus::", "limits::", "policy::", "check::"],
    # **`wall::` too, and its absence is measured.** The clauses of the caret
    # rule live in `wall.rs` and are decided by `wall::tests` — `corpus::` drives
    # `caret_uses` through a door written for it and reaches nothing else in the
    # file. An operator over `the_refusal_is_the_doors` under the default suite
    # comes back SURVIVED against a test that kills it: the same shape, a
    # source behind too narrow a command.
    "wall": ["corpus::", "limits::", "wall::"],
    # `database::`, whose tests need the stack up — like the two stores above,
    # and for the same reason: what this file decides, a database decides.
    "database": ["corpus::", "limits::", "database::"],
    # **`counts::` too.** The rotting-count reader's own fixtures live in
    # `counts::tests`, and this map's default runs only the corpus and the
    # limits suites — so without it a weakening of `claim` is reported as
    # surviving when what has happened is that nothing which decides it was run.
    "counts": ["corpus::", "limits::", "counts::"],
    "references": ["corpus::", "limits::", "references::"],
    "sections": ["corpus::", "limits::", "sections::"],
    "workflow": ["corpus::", "limits::", "workflow::"],
    "release": ["corpus::", "limits::", "release::"],
    "history": ["corpus::", "limits::", "history::"],
    "ui": ["corpus::", "limits::", "ui::"],
    "docs": ["corpus::", "limits::", "docs::"],
    # The plugin build's own module, which is where its rules are decided.
    "plugins": ["corpus::", "limits::", "plugins::"],
    # The local database's gates, decided by their own module: the schema
    # built from a scratch copy of `local-migrations/`, and the source rule's
    # fixtures.
    "local_gates": ["corpus::", "limits::", "local::"],
}
SUITE = ["corpus::", "limits::"]


# **A source whose judge is in another crate.** Every filter above narrows the
# same command, `-p xtask --bin xtask`, and `ledger.rs` is not reachable from it
# at all: the clauses that decide whether an error code can be renumbered are
# tested by `nc-catalog-compiler`'s own suite. A source added here without its
# command would report every operator as SURVIVED — a hole in this script read
# as a hole in the tree, which is the direction the order of work below exists
# to prevent.
ELSEWHERE = {
    "ledger": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    # The whole crate, both targets: the unit tests hold the writer and the
    # reader, and `crates/nc-canon/tests/properties.rs` holds the round trip
    # between them.
    # A weakening of either file can be decided by either, and running one
    # suite would report a clause as uncovered because the wrong half was
    # asked.
    "canon": ["cargo", "test", "-q", "-p", "nc-canon"],
    "reader": ["cargo", "test", "-q", "-p", "nc-canon"],
    # The whole crate, both targets: the unit tests hold the digest and the
    # user's spelling, and `crates/nc-channel/tests/the_names_are_disjoint.rs`
    # holds every name, the ports and the manifest. Neither spawns a process
    # or registers anything with a system.
    "channel": ["cargo", "test", "-q", "-p", "nc-channel"],
    # The whole crate, both targets: the unit tests hold the layout and the
    # alphabet, and `crates/nc-ids/tests/properties.rs` and
    # `crates/nc-ids/tests/against_denote.rs` hold the round trips. A weakening
    # of either file can be decided by any of them.
    "base32": ["cargo", "test", "-q", "-p", "nc-ids"],
    "snowflake": ["cargo", "test", "-q", "-p", "nc-ids"],
    # The whole crate again, for the reason the two above it are: the unit tests
    # hold the scheme and the layout, and `crates/nc-ids/tests/against_denote.rs`
    # holds this module against the Emacs package it transcribes.
    "naming": ["cargo", "test", "-q", "-p", "nc-ids"],
    # The journal's own crate, both targets: the unit tests hold the readers and
    # `tests/` holds the corpus that drives them.
    # **The whole package, which is what the comment above says.** Under
    # `--lib`, the suite that *is* acceptance criterion 3 —
    # `crates/nc-journal/tests/tampering.rs` — is never compiled by the command
    # that judges the sources it reads, while `check.rs`'s `NOT_MUTATED` census
    # excuses that suite from mutation **because** those sources are mutated.
    # Measured with `entry.rs`'s `DOMAIN` moved from `/v1` to `/v2` — every
    # entry already written becomes unverifiable — which `--lib` reports as
    # `46 passed` and the whole package refuses by name, at `tampering.rs:924`.
    "entry": ["cargo", "test", "-q", "-p", "nc-journal"],
    "memory": ["cargo", "test", "-q", "-p", "nc-journal"],
    "unit": ["cargo", "test", "-q", "-p", "nc-journal"],
    # The compiler's, whose gates live in
    # `crates/nc-catalog-compiler/tests/the_gates_fire.rs` and whose
    # readers are held by the unit tests beside them.
    "loader": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    "emitter": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    "pages": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    # The crate's own suite, which is every test these three files have: the
    # compiler's gates call the judge too, and are its second caller rather
    # than its decider.
    "schema": ["cargo", "test", "-q", "-p", "nc-schema"],
    "selector": ["cargo", "test", "-q", "-p", "nc-schema"],
    "descriptor": ["cargo", "test", "-q", "-p", "nc-schema"],
    # An integration test is a target of its own, and `--bin xtask` cannot
    # reach one.
    "registry": [
        "cargo",
        "test",
        "-q",
        "-p",
        "xtask",
        "--test",
        "the_registry_reaches_the_code",
    ],
    "flake": ["cargo", "test", "-q", "-p", "nc-ids"],
    # The whole crate: the unit tests hold the renderer and the shipper, and
    # `tests/` holds what a caller can reach. The wall's *types* are decided
    # elsewhere again — by `xtask`'s twelve compile-fail fixtures — and no
    # operator here can weaken those, which is the division of labour between
    # this script and criterion 2's own gate rather than a gap in either.
    "value": ["cargo", "test", "-q", "-p", "nc-log"],
    "ship": ["cargo", "test", "-q", "-p", "nc-log"],
    # **The whole crate, both targets — and the narrower command is measured
    # wrong.** Under `--test in_postgres`, on the reasoning that the SQL is what
    # the operators are over, three of `nc-prune`'s come back SURVIVED, and all
    # three are decided by that crate's *unit* tests, in the same file as the
    # code: `over_deleted` is asserted at four call sites in `postgres.rs`'s own
    # `mod tests`, which `--test in_postgres` does not run. That is a hole in
    # this script read as a hole in the tree, which is the direction the order of
    # work in this file exists to prevent.
    "journal_store": ["cargo", "test", "-q", "-p", "nc-journal"],
    "prune_store": ["cargo", "test", "-q", "-p", "nc-prune"],
    # The whole crate for the identifiers, as its siblings are; `nc-prune`'s root
    # is decided by that crate's unit tests, where the writer and the reader of a
    # refusal both live.
    "ids": ["cargo", "test", "-q", "-p", "nc-ids"],
    # **And no `--lib`.** The two rows above record `--lib` measured wrong — it
    # never compiles `tampering.rs`, which *is* criterion 3, and never compiles
    # the forged tail in `in_postgres.rs`. This row's own excuse, "decided by
    # that crate's unit tests, where the writer and the reader of a refusal both
    # live", is true of `refusal()` and `Pruned::code()` and not of the whole
    # file: `#[non_exhaustive]` is why `Request::every()` exists, for callers
    # outside the crate, and `in_postgres.rs` and `in_sqlite.rs` call it as one
    # does — two targets `--lib` never compiles. `--lib` is refused outright in
    # this table by the meta gate: each row above names a target it leaves out.
    "pruner": ["cargo", "test", "-q", "-p", "nc-prune"],
    # The whole crate for the pack, whose own suite is where every one of these
    # clauses is decided.
    # **The whole package, not `--lib`.** The two unit guards are decided by a
    # forged tail in `crates/nc-journal/tests/in_postgres.rs`, which `--lib` does
    # not run — the
    # same shape as `dev.rs` running against a suite that could not reach its own
    # gate.
    "journal": ["cargo", "test", "-q", "-p", "nc-journal"],
    "pack": ["cargo", "test", "-q", "-p", "nc-policy"],
    "problem": ["cargo", "test", "-q", "-p", "nc-policy"],
    "store": ["cargo", "test", "-q", "-p", "xtask", "--bin", "xtask"],
    "main": ["cargo", "test", "-q", "-p", "xtask", "--bin", "xtask"],
    "generator": ["cargo", "test", "-q", "-p", "xtask", "--bin", "xtask"],
    "members": ["cargo", "test", "-q", "-p", "xtask", "--bin", "xtask"],
    "compiler_problem": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    "compiler_messages": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    "pack_cases": ["cargo", "test", "-q", "-p", "nc-policy"],
    "pack_value": ["cargo", "test", "-q", "-p", "nc-policy"],
    "pack_manifest": ["cargo", "test", "-q", "-p", "nc-policy"],
    # The whole crate, both targets: the unit tests hold the calendar and the
    # embedded list, and `crates/nc-authz/tests/the_embedded_pack_decides.rs`
    # asks the embedded pack through the directory, the way the kernel will.
    "authz": ["cargo", "test", "-q", "-p", "nc-authz"],
    "authz_directory": ["cargo", "test", "-q", "-p", "nc-authz"],
    "authz_embedded": ["cargo", "test", "-q", "-p", "nc-authz"],
    # The kernel's whole package: the unit tests beside each rule, and the suite
    # that loads the component — which is why the plugins are built first.
    "kernel": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_world": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_bindings": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_engine": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_built": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_capability": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_limits": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_manifest": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_log": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_context": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_store": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_blobs": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_commands": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_events": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_ui": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_net": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_host_timers": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_backend": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_blobs": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_registry": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_dispatch": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_units": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_logging": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_supervisor": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_net": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_net_ureq": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_history": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_builtin": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_events": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_timers": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_ui": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "kernel_pump": ["cargo", "test", "-q", "-p", "nc-kernel"],
    # The local database's own crate, both targets: the unit tests beside each
    # rule, and the two suites that open real files and kill a child mid-commit.
    "local_lib": ["cargo", "test", "-q", "-p", "nc-local"],
    "local_open": ["cargo", "test", "-q", "-p", "nc-local"],
    "local_schema": ["cargo", "test", "-q", "-p", "nc-local"],
    "local_store": ["cargo", "test", "-q", "-p", "nc-local"],
    "local_journal": ["cargo", "test", "-q", "-p", "nc-local"],
    # Each system's privacy, by its own crate's tests first, and then — in
    # `ALSO_RUN` — by the local database's suites that stand on it.
    "platform": ["cargo", "test", "-q", "-p", "nc-platform"],
    "platform_unix": ["cargo", "test", "-q", "-p", "nc-platform"],
    "platform_testing": ["cargo", "test", "-q", "-p", "nc-platform"],
    # The pruner's crate, whose `in_sqlite.rs` needs no service.
    "prune_sqlite": ["cargo", "test", "-q", "-p", "nc-prune"],
    # **Built, not tested.** Each of these is compiled for `wasm32` alone, so
    # the command that decides it is the one that builds and encodes it —
    # `cargo xtask build --plugins`, spelled out so it names the package it
    # runs — and the kernel's suite, in `ALSO_RUN`, is what notices a component
    # that encodes and is wrong. Run in that order it cannot be vacuous: the
    # kernel refuses a component its sources no longer build, so the suite
    # alone would call every weakening here killed.
    "reference": ["cargo", "run", "-q", "--locked", "-p", "xtask", "--", "build", "--plugins"],
    "reference_adapter": [
        "cargo", "run", "-q", "--locked", "-p", "xtask", "--", "build", "--plugins",
    ],
    # The plugin's behaviour is plain Rust, decided by its own host tests.
    "reference_behaviour": ["cargo", "test", "-q", "-p", "nc-plugin-reference"],
    "log_root": ["cargo", "test", "-q", "-p", "nc-log"],
    "compiler_model": ["cargo", "test", "-q", "-p", "xtask", "--bin", "xtask"],
    # **Spelled out rather than left to the default, because it has a second
    # command beside it.** What this file's `lib.rs` is judged by is the corpus
    # in `xtask/src/corpus.rs` — measured, and the measurement is the reason the
    # entry below exists: all eighteen operators over this source are killed by
    # `corpus:: limits::` and **none** of them by `cargo test -p
    # nc-catalog-compiler`. They sit on the marker rule, whose only driver is
    # that corpus.
    "compiler": [
        "cargo",
        "test",
        "-q",
        "-p",
        "xtask",
        "--bin",
        "xtask",
        "--",
        "corpus::",
        "limits::",
    ],
}


# A second command, run beside the first, for a source one command cannot judge.
#
# **A source can be the subject of two suites in two packages, and a verdict
# needs both.** `crates/nc-catalog-compiler/src/lib.rs` is the case: the same
# circularity as one row up in `check.rs`'s `NOT_MUTATED` census, which excuses
# `crates/nc-catalog-compiler/tests/the_gates_fire.rs` from mutation **because**
# `lib.rs`, `load.rs` and `problem.rs` are edited here. Two of the three run
# `-p nc-catalog-compiler`, which builds that suite. `lib.rs` under the default
# `-p xtask --bin xtask` alone, which cannot build it at all, would leave the
# suite excused on the strength of a command that never compiles it.
#
# Measured: `if !problems.is_empty()` in
# `lib.rs` weakened to `if problems.len() > 1`, so a catalog with exactly one
# problem compiles and emits. Under the command above, `107 passed` and
# `SURVIVED`; under the compiler's own package, `46 passed; 59 failed`, the
# first of them `a_key_that_says_nothing_fails_the_build`.
#
# Neither command may replace the other: the corpus kills all eighteen operators
# and the package kills none of them, while only the package can notice a
# weakening of the verb the gates suite drives. So both are run, cheapest first,
# and the first non-zero exit decides.
ALSO_RUN = {
    "compiler": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    # **The drift gate cannot see the file's own tests.** `model.rs` is judged
    # by the comparison in the xtask binary — deliberately, and
    # `JUDGED_OUTSIDE_ITS_PACKAGE` excuses the package mismatch — but that row
    # asks only whether the excuse is written, never whether anything else
    # decides the file. `model.rs` holds three tests of its own, which the
    # drift comparison never runs. Its own doc block states the consequence in
    # advance: measured with four of the ten `FieldType` variants declared by
    # no field, those four arms of `label()` and `rust()` are dead to the drift
    # comparison and `cargo xtask check` is byte-identical with all four
    # corrupted. `I64` and `Id` are declared by no field in
    # `catalog/log-events.toml`, so that comparison cannot be the whole command.
    # The drift gate stays first because it is the cheaper of the two.
    "compiler_model": ["cargo", "test", "-q", "-p", "nc-catalog-compiler"],
    # **Cedar's parser, which `nc-schema` cannot reach.** The type-name rule is
    # written by hand in the crate the catalog compiler depends on, and the one
    # suite that holds it to what Cedar actually parses lives in `nc-policy`,
    # which runs Cedar. A reserved word dropped from the list leaves every test
    # of `nc-schema` green; this is the command that notices.
    "schema": [
        "cargo",
        "test",
        "-q",
        "-p",
        "nc-policy",
        "--test",
        "an_entity_type_is_a_name_cedar_parses",
    ],
    # **A caller in another crate, and the only one.** `Pack::ids_under` is how
    # `nc-authz`'s suite narrows the gate's whole tree to the core and one pack
    # before holding the embedded ids to it, and nothing in `nc-policy` calls
    # it: `self.effects_under(under)` made `effects_under("")` there left
    # `nc-policy` green and failed that suite, measured.
    "pack": ["cargo", "test", "-q", "-p", "nc-authz"],
    # **The suite that loads what the build wrote.** A weakening of the
    # plugin's root or its adapter can build and encode cleanly and still be a
    # different plugin; the kernel activating it is what says so.
    "reference": ["cargo", "test", "-q", "-p", "nc-kernel"],
    "reference_adapter": ["cargo", "test", "-q", "-p", "nc-kernel"],
    # **The callers that open real files.** `nc-platform`'s own tests judge
    # paths it made itself; the local database's suites open profiles with
    # every refusal staged through the test doors, which nothing in
    # `nc-platform` calls from outside.
    "platform": ["cargo", "test", "-q", "-p", "nc-local"],
    "platform_unix": ["cargo", "test", "-q", "-p", "nc-local"],
    "platform_testing": ["cargo", "test", "-q", "-p", "nc-local"],
}


def suite_for(source):
    """The test filter that can decide a weakening of `source`."""
    return SUITES.get(source, SUITE)


def command_for(source):
    """Every test command that has to be run to decide a weakening of `source`.

    A list of commands, not one: see `ALSO_RUN`. The verdict is the first
    non-zero exit, so the cheapest command is written first.
    """
    first = ELSEWHERE.get(source) or TEST + suite_for(source)
    extra = ALSO_RUN.get(source)
    return [first, extra] if extra else [first]


# **A mutant may hang, and a run that waits for one is a run nobody reads.**
# Deleting a guard can turn a loop into an endless one rather than a wrong one —
# measured: one operator over `names` removed the test that stops an empty
# subject matching at every position, and the suite sat on it for twenty-three
# hours before anybody looked. A weakening that never terminates is a verdict of
# its own, not a reason to stop the world.
#
# **Fifteen minutes, because five is not many times the slowest suite.** The
# kernel's command, `cargo test -q -p nc-kernel`, runs every target before the
# one that decides a blob, so a weakening only `where_a_blob_is.rs` kills
# spends the whole suite first: measured with `time` over `let_go` turned into
# `abort` in `units.rs`, four minutes on an idle machine, which a bound of five
# tallies as `did not finish` — an honest kill reported as a hole. The bound is
# a multiple of that again; a weakening that never terminates still costs one
# bounded wait, not the run.
LONGEST = 900


# **What a suite says when it stops before the mutant, and what fixes each.**
# Each first half is copied, word for word, from the suite that says it, and
# `check.rs` holds every copy to the tree. A failure is this file's word for
# `killed`, so a suite that fails before reading a line of the weakening is a
# kill that proved nothing — and `run_corpus` refuses the run instead.
#
# The first two are the database suites of `nc-journal` and `nc-prune`, which
# panic rather than stand aside; `run_corpus` records the incident they answer.
#
# **The third is the kernel's, and without it every run after the first is a
# hollow kill.** `cargo test -p nc-kernel` loads the reference plugin through
# `crates/nc-kernel/src/built.rs`, which panics on a component its sources no
# longer build, ending with that file's `REBUILD` sentence; this is its tail.
# Measured: an operator over `plugins/reference/src/adapter.rs` rebuilds the
# component from the weakening, a `restore()` that puts the source back
# without rebuilding leaves it stale, and the next run judges every operator
# over the kernel by a suite that panics on the stale component — `killed` for
# all of them, with no weakening on disk at all. `main` builds the plugins before the
# first edit, again after every operator whose command built them from a
# weakening, and `restore()` after the last, so this refusal is what is left
# for a staleness none of them can see.
NEVER_REACHED = (
    ("DATABASE_URL is set, so it has to connect", "Fix DATABASE_URL and run again."),
    ("DATABASE_URL is not set", "Fix DATABASE_URL and run again."),
    (
        "cargo xtask check does it before the tests",
        "Run `cargo xtask build --plugins`, so the component is built from the tree "
        "on disk, and run again.",
    ),
)


def run_corpus(source):
    subprocess.run(TOUCH, cwd=ROOT, check=False)
    # **Every command the source needs, and the first one that fails decides.**
    # A weakening is killed if *any* suite written for it goes red, so a run that
    # stops at the first non-zero exit reaches the same verdict as one that runs
    # them all — and pays for the second command only on a survivor.
    done = None
    for command in command_for(source):
        try:
            done = subprocess.run(
                command,
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=LONGEST,
            )
        except subprocess.TimeoutExpired:
            return "did not finish"
        if done.returncode != 0:
            break
    out = done.stdout + done.stderr
    # **A suite that never reached the mutant is not a kill.** The guard at the
    # top of this file asks the variable and asks the socket, and stops at the
    # SSLRequest reply — deliberately, so no session is opened and no
    # credentials are sent. What it therefore cannot ask is whether the *named*
    # database, user and password are ones the suites can use, and a URL that
    # names a live PostgreSQL with one character wrong in the database name is
    # accepted: every store-backed suite then panics on connect, the exit code
    # is non-zero, and without what follows that counts as `killed` — the
    # incident the guard exists for, eight files judged by suites that never
    # read a line of the mutant, reachable by one character.
    #
    # The suites say so themselves, in their own words, and this function
    # already has their output. A run that cannot reach its subject is refused
    # whole rather than reported as a pass. `NEVER_REACHED` holds the words.
    for said, fix in NEVER_REACHED:
        if said in out:
            print(
                f"\n  refused: a suite for `{source}` never reached the mutant and "
                f"said so — `{said}`.\n  Every verdict after this one would be a suite "
                f"that never read the mutant, counted as a kill. {fix}",
                flush=True,
            )
            raise SystemExit(2)
    # **The exit code, not the first `test result:` line in the output.**
    # Asking whether `test result: ok` appears *anywhere*, and answering
    # SURVIVED if it does, is wrong for a whole-crate command, which prints one
    # such line per target: a weakening killed by an integration target, with
    # the lib target passing above it, is then reported as a survivor.
    # Measured over twelve operators, eight of them over `nc-journal`'s store:
    # those eight and the one over `nc-prune`'s that a database decides came
    # back SURVIVED that way, while the same twelve, driven by the same commands
    # and judged by the exit code, were killed twelve out of twelve.
    #
    # What hides it is which target holds the kill: `nc-canon` and `nc-ids` are
    # decided by their *lib* tests, cargo stops at the first failing target, and
    # the output then holds no `ok` line at all — so those verdicts come out
    # right by luck rather than by rule.
    #
    # A non-zero exit is what "some test failed" means, and the string is kept
    # only to tell a failing suite from one that did not build.
    if done.returncode == 0:
        return "SURVIVED"
    if "test result: FAILED" in out or "panicked at" in out:
        return "killed"
    return "does not compile"


# The weakenings no input can distinguish from the original, with the argument.
#
# `depth == 0 -> depth <= 0` is in a loop whose `')' if depth == 0 => break` arm
# fires first, so depth never goes negative.
#
# **`"#![" -> "#!"` is not an equivalence**, though it can be argued as one:
# "the only other `#!`-initial line in Rust is a shebang, which balances and
# holds no lint name" — and this gate reads *lines*, so the interior of a
# multi-line raw string is one. A file holding
# `const S: &str = r#"\n#!allow(clippy::all)\n"#;` makes the widened rule report
# a false finding against it, measured. The corpus decides it.
#
# **A dead clause is not an entry here.** An `Option` read behind a guard that
# makes it `Some`, or a filter later guards already dominate, is a clause to
# say once rather than an untestable one. An entry here is an argument that no
# input can tell the difference; when the argument is "the code says this
# twice", the answer is to say it once.
EQUIVALENT = {
    # The `depth` this mutates is `let mut depth = 0_i32;` in the attribute
    # reader, decremented with a plain `-= 1`. It never goes below zero anyway,
    # and the reason is **the order of the two arms**: `')' if depth == 0 =>
    # break` is written above `')' => depth -= 1`, so the decrement is
    # unreachable at zero and `<= 0` is the same predicate as `== 0` for every
    # input there is. No test can tell them apart because no *value* can.
    #
    # **Not `holds_a_test`'s counter**, which really is a `usize` decremented
    # with `saturating_sub` and which this operator does not touch: an argument
    # resting on that one would cite a trigger — *if the counter ever stops
    # being a `usize` or stops saturating* — that can never fire here. A note
    # nobody can act on is worse than none, because the set exists to make
    # "a survivor nobody looked at" impossible and a false note reads exactly
    # like one somebody did.
    #
    # It is re-argued if those two arms are ever re-ordered, and
    # `every_accepted_survivor_rests_on_a_fact_this_tree_still_holds` holds them
    # in that order rather than leaving the trigger to prose.
    "nesting: depth == 0 -> depth <= 0",
    # **The application-side half of a rule whose live enforcer is a trigger.**
    # A sequence behind the tail is refused by `journal_entries_pkey` and reaches
    # `why_nothing_landed` through the unique-violation arm; a sequence ahead is
    # refused by `migrations/0010_a_sequence_is_the_next_one_and_nothing_else.sql`'s `BEFORE INSERT` trigger, whose SQLSTATE
    # 23514 is read back into `OutOfOrder` and re-diagnosed. So no input can tell
    # the clause's presence from its absence, and calling the code wrong would be
    # wrong — what is wrong is a coverage claim over a clause nothing reaches.
    #
    # **It is re-argued if `0010`'s trigger ever leaves, or stops being
    # `ENABLE ALWAYS`** — `§14.9` records that the role this stack ships with
    # owns the tables and can `ALTER TABLE … DISABLE TRIGGER ALL`, so the fact
    # this rests on is the migration's text, and
    # `every_accepted_survivor_rests_on_a_fact_this_tree_still_holds` holds it
    # rather than leaving the trigger to prose. A test instead of this note would
    # have to disable the trigger, which `check`'s protections census then
    # reports: too strong.
    "the sequence is the one after the tail",
    # The mutation is `["env", last]` -> `[_, last]`, so it changes an answer
    # only for a two-part name whose first part is something other than `env`
    # and whose second is a known format that is not one an environment is
    # written in. Reaching the exemption at all takes an `env` part or a `.env`
    # prefix; an `env` part in a two-part name whose first part is not `env` is
    # the second part, and `env` is not a known format, so the only way left is
    # the prefix — and a `.env...` name of exactly two parts is `.envSOMETHING`,
    # whose single non-empty part would have to be a known format beginning with
    # `env`. **One fact carries the whole equivalence: no known format begins
    # with `env`.** It is the same fact in both places, and the case that
    # matters is the shortest one — `foo.env` is a credential file, and the day
    # `env` becomes a known format the mutation exempts it while the original
    # reports it.
    #
    # **`KNOWN_FORMATS` is the list the equivalence actually rests on**, not
    # `AN_ENVIRONMENT_IS_WRITTEN_IN`, and `.env…` names do not all have three
    # parts — `.env` itself is `["", "env"]`. Dropping the `"env"` literal
    # matters only for a name whose last part is `env`, and such a name reaches
    # the guard only if `env` is a known format. Growing
    # `AN_ENVIRONMENT_IS_WRITTEN_IN` changes nothing; growing `KNOWN_FORMATS`
    # breaks the equivalence immediately, and the first name it lets through is
    # `.env` — the file this whole gate exists for.
    #
    # A trigger attached to the wrong list is a trigger that cannot fire, so it
    # is not left to prose:
    # `every_accepted_survivor_rests_on_a_fact_this_tree_still_holds` asserts
    # that no known format is spelled `env`.
    "the first part is `env`",
}


# **Under `target/`, and the reason is a gate this file trips.** At
# `xtask/.mutants-in-flight`, which is a file with no extension under a tree  # allow-reference: names `xtask/.mutants-in-flight`, a path this sentinel must not have
# `no_file_of_an_unknown_kind_sits_in_this_repository` walks, for the whole
# of a run that rule fails on a *pristine* tree, and `check::` is in the suite
# this script uses for `check.rs` and `policy.rs` — so operators are reported
# `killed` by the presence of this file rather than by any test, the hollow
# verdict the sentinel exists to remove. Measured:
# `touch xtask/.mutants-in-flight` alone gives
# `check::…::no_file_of_an_unknown_kind_sits_in_this_repository FAILED` and takes
# `cargo xtask check` down with it.
#
# `target/` is skipped by that walk — it sits beside a manifest and outside every
# tree the rule reads — and a fresh clone, which has no `target/`, correctly
# answers "no run in flight". What it costs is that `cargo clean` forgets a
# killed run, and the sources it left behind would then be found by `check`
# rather than by this warning.
SENTINEL = os.path.join(ROOT, "target/.mutants-in-flight")
"""Written before the first edit and unlinked when the tree is back.

**A `SIGKILL` cannot be caught, and the handlers below are not the whole
answer.** Measured: killing a run with `-9` leaves a weakening on disk, and the
*next* run reads it as `original`, hands it to every later mutation, and writes
it back as the restored baseline, which only a `git diff` read by hand would then
show. Even the `SIGTERM` handler that does exist restored the mutant, because
by then the mutant was what `original` held.

The backups already survive a kill: the `finally:` that unlinks them never runs.
Nothing named them, so nobody could find them. This file names them, and the
refusal below reads it.

**And they are beside it now, rather than in `/tmp`.** They were
`tempfile.NamedTemporaryFile(delete=False)`, which is a path with no owner: a
reboot, or anything that sweeps that directory, takes the pristine copies while
leaving this file naming them — so the one instruction the refusal gives points
at a file that is gone. Both live under `target/` now, so `cargo clean` forgets
the whole incident at once instead of half of it. That coupling is the cost of
the only safe location there is: a sentinel anywhere else is a file under a
walked tree, and the extensionless one tried at the repository root took
`check::` down for every run that saw it.
"""

BACKUPS = os.path.join(ROOT, "target/.mutants-backups")
"""Where the pristine text of each source is kept while a run is in flight.

One file per `SOURCES` key, named for the key, so the sentinel can name it and a
person can copy it back by hand. Removed with the sentinel when the run ends any
of the three ways a handler can see.

Measured on 2026-09-10, the whole recovery rather than the mechanism: a run
killed with `-9` four seconds in left `crates/nc-ids/src/flake.rs` holding
`Self(raw % (RANGE + 1))`, the next run refused with exit 2 instead of taking
that as the baseline, it named all twenty-three sources with an existing copy
beside each, and copying them back left every source byte-identical to a set
taken before the run.
"""


def refuse_if_a_previous_run_was_killed():
    """Stop rather than take a weakening as the baseline."""
    if not os.path.exists(SENTINEL):
        return False
    print(open(SENTINEL).read(), end="")
    print(
        "\nThe last run of this file was killed before it restored the tree, so one of\n"
        "the sources above may hold a weakening. Copy each one back from the backup\n"
        "beside it, delete the sentinel, and run again. Do not run the corpus first:\n"
        "a green corpus over a weakened source is exactly what this file exists to\n"
        "make impossible."
    )
    return True


# The packages whose suites refuse to run without a database, and the operators
# each of them judges.
#
# Derived from the commands rather than written twice: a package is named here
# because a command in this file runs it, and a suite of each one panics with
# *"DATABASE_URL is not set … skipping them silently is the failure
# `cargo xtask check` exists to prevent"* rather than standing aside.
#
# **Held both ways, because one direction lets the largest one out.** The
# kernel's `the_decision_point` suite panics in those words too, and judges
# more operators than every other command here together: a list missing it
# still refuses, because the refusal asks the variable and not this list, but
# names a fraction of what it is protecting.
# `the_runner_refuses_a_run_it_cannot_judge_its_stores_in` also walks
# `crates/*/tests/` for a suite that panics that way and refuses one this does
# not name.
NEEDS_THE_DATABASE = ("nc-journal", "nc-prune", "nc-kernel", "nc-local", "database::")


def refuse_if_the_database_is_absent():
    """Stop rather than report a suite that could not run as a kill.

    **A suite that cannot reach its database fails, and a failure is this
    file's word for `killed`.** Every operator whose command runs a package
    `NEEDS_THE_DATABASE` names, and every one behind the `database::` filter, is
    then a false kill — the weakening proved nothing and the tally says it did. Measured on
    2026-09-16: a whole run with `DATABASE_URL` unset came back
    `killed 353  SURVIVED 2  does not compile 0`, a sentence indistinguishable
    from a good run, in which eight of the forty-nine files had been judged by
    suites that all panicked before reading a line of the mutant.

    **What caught it was the one survivor written down as equivalent.** The
    `INSERT`'s ordering clause is argued as indistinguishable in this
    deployment, so the runner refuses a run in which it turns up *killed* — and
    that refusal was the only thing standing between this and a green
    acceptance. A guard that exists because one entry in `EQUIVALENT` happened
    to sit on a database-backed file is a guard by luck; this one is by rule.

    The variable and the socket, because a stale URL pointing at nothing gives
    exactly the same false kills as no URL at all. **Read, never derived**: the
    stack's port is the checkout's, and a second derivation of it here would be
    a second naming function, so a caller exports what `cargo xtask dev` prints.
    """
    url = os.environ.get("DATABASE_URL", "")
    behind = ", ".join(NEEDS_THE_DATABASE)
    if not url:
        print(
            "DATABASE_URL is not set, and the suites that judge "
            f"{behind}\nrefuse to run without it. A suite that cannot start fails, and a "
            "failure is\nthis file's word for `killed` — so every operator over those "
            "sources would be\nreported as covered while proving nothing. Run "
            "`cargo xtask dev` and\nexport what it prints."
        )
        return True
    where = url.rsplit("@", 1)[-1].split("/", 1)[0]
    host, _, port = where.partition(":")
    try:
        with socket.create_connection((host or "127.0.0.1", int(port or 5432)), timeout=5) as to:
            # **Ask it something, because a socket that is merely open answers
            # the wrong question.** Opened and asked nothing, this is satisfied
            # by any listening port: measured with `DATABASE_URL` pointed at the
            # log store's port — the guard returned False and the pruner's suite
            # then panicked on connect, which `run_corpus` reads as `killed`:
            # the stale-URL case the paragraph above promises to cover.
            #
            # An `SSLRequest` is the smallest thing a PostgreSQL server answers:
            # eight bytes, length then the code 80877103, and the reply is the
            # single byte `S` or `N`. Anything else, or nothing, is not a
            # PostgreSQL server. It is asked before any startup message, so no
            # session is opened and no credentials are sent.
            to.settimeout(5)
            to.sendall(b"\x00\x00\x00\x08\x04\xd2\x16\x2f")
            answer = to.recv(1)
        if answer not in (b"S", b"N"):
            print(
                f"DATABASE_URL names {where} and what answers there is not a PostgreSQL\n"
                f"server (it replied {answer!r} to an SSLRequest). The suites that judge\n"
                f"{behind} refuse to run without a database, and a suite that cannot start\n"
                "fails — which this file would report as `killed` for every operator over\n"
                "those sources. Export what `cargo xtask dev` prints for this checkout."
            )
            return True
    except (OSError, ValueError) as problem:
        print(
            f"DATABASE_URL names {where} and nothing answers there ({problem}).\n"
            f"The suites that judge {behind} refuse to run without a database, and\n"
            "a suite that cannot start fails — which this file would report as `killed`\n"
            "for every operator over those sources. Run `cargo xtask dev` and export\n"
            "what it prints."
        )
        return True
    return False


# The command that builds every plugin's component from the sources on disk.
BUILD_THE_PLUGINS = ["cargo", "run", "-q", "--locked", "-p", "xtask", "--", "build", "--plugins"]


def builds_the_plugins(source):
    """Whether judging `source` builds every plugin's component from the disk.

    **Such an operator leaves the component built from its weakening**, and the
    kernel's suites read that component for every operator over the kernel
    after it. `main` asks this after each verdict and builds again from the
    tree as handed, because `WEAKENINGS` is in the order operators were
    added, so nothing keeps the plugin's operators below the kernel's: in
    Stage 11's acceptance the first kernel operator written under them
    refused the whole run, 714 verdicts in, on a component its suite
    rightly called stale.
    """
    return BUILD_THE_PLUGINS in command_for(source)


def build_the_plugins():
    """Build every plugin's component from the tree as it is; True if it built."""
    done = subprocess.run(BUILD_THE_PLUGINS, cwd=ROOT, capture_output=True, text=True)
    if done.returncode != 0:
        print(done.stdout + done.stderr, end="")
    return done.returncode == 0


def refuse_if_the_plugins_do_not_build():
    """Stop rather than let the kernel's suites judge a component this tree did not build.

    **The kernel's suites read a file this run does not write.** Every operator
    over `crates/nc-kernel` is judged by `cargo test -p nc-kernel`, which loads
    the component `cargo xtask build --plugins` wrote and panics if the plugin's
    sources have changed since — and a panic is this file's word for `killed`.
    A component left behind by anything, the last operator of a previous run
    over the plugin's adapter included, would then call every one of them
    killed. So the component is built from the tree as handed before the first
    edit, and a tree whose plugins do not build is refused rather than judged.
    """
    if build_the_plugins():
        return False
    print(
        "`cargo xtask build --plugins` failed on the tree as it was handed, so the\n"
        "kernel's suites would read a component these sources did not build, and a\n"
        "suite that refuses a stale component fails — which this file would report as\n"
        "`killed` for every operator over the kernel. Make the plugins build first."
    )
    return True


def the_commands_judging(sources):
    """Every distinct command an operator over one of `sources` is judged by, once each."""
    seen = []
    for source in sources:
        for command in command_for(source):
            if command not in seen:
                seen.append(command)
    return seen


def refuse_if_a_suite_fails_unweakened(when):
    """Stop rather than count a suite that fails on the tree without a weakening as a kill.

    **A failure is this file's word for `killed`, so a suite that fails already
    must not be read as one.** `NEVER_REACHED` knows three sayings of a suite
    that stopped before the mutant; any other failure on the tree as it stands
    would be a kill for every operator judged by that suite. With one editor
    swap file beside `xtask/src/check.rs` — which `check::`'s rule on the kinds
    of file in this repository refuses — an operator that edits only a comment
    comes back `killed` and the run exits 0; a `__pycache__` or a flaky test
    does the same. So every command any operator is judged by runs on the tree
    `when` says, and one that fails refuses the run.

    Run before the first edit, and again once the tree is put back: a suite
    that starts failing in the middle of a run makes every kill since the last
    green control one that proved nothing, and the second control is what
    says so.
    """
    for command in the_commands_judging([source for source, *_ in WEAKENINGS]):
        try:
            done = subprocess.run(
                command, cwd=ROOT, capture_output=True, text=True, timeout=LONGEST
            )
        except subprocess.TimeoutExpired:
            print(f"`{' '.join(command)}` did not finish on the tree {when}.")
            return True
        if done.returncode != 0:
            print(done.stdout[-2000:] + done.stderr[-2000:], end="")
            print(
                f"\n`{' '.join(command)}` fails on the tree {when}, with no weakening on "
                "disk. A failure is this file's word for `killed`, so every operator it "
                "judges would be counted covered for a failure that was already there. "
                "Make it pass, and run again."
            )
            return True
    return False


def judged_by(names):
    """The suite a command in `ELSEWHERE` actually runs, named as a reader would."""
    package = names[names.index("-p") + 1]
    if "--" in names and names.index("--") + 1 < len(names):
        return f"{package}'s {names[names.index('--') + 1].rstrip(':')}"
    if "--lib" in names:
        return f"{package}'s unit tests"
    # **And a named integration target.** One entry runs
    # `--test the_registry_reaches_the_code`, which is not "all of xtask".
    if "--test" in names and names.index("--test") + 1 < len(names):
        return f"{package}'s {names[names.index('--test') + 1]} suite"
    # **And `--bin`, which narrows as hard as any of them.** `xtask` has five
    # test targets — the binary and four in `xtask/tests/` — so
    # `-p xtask --bin xtask` runs one of five, which is not "all of xtask"
    # either. It is deliberate and priced: `compiler_model`'s own note in
    # `SOURCES` records the wider command spending a whole integration suite
    # per operator to run a test that cannot fail. A sentence that hides the
    # narrowing hides the price with it.
    if "--bin" in names and names.index("--bin") + 1 < len(names):
        return f"{package}'s {names[names.index('--bin') + 1]} binary"
    return f"all of {package}"


def the_summary():
    """What this run covered, in one line — and what it did not.

    **Lifted out of `main` and reachable as `--summary`**, because the line is
    an argument about coverage and nothing could read it without a full run:
    the two defects below were each found by a person reading the sentence, one
    round apart, and the second sat in the half the first had not touched.
    """
    # **And what it did not reach, on the same line as what it did.** Every
    # other step in this repository reports the size of the set it inspected;
    # a tally with no denominator, `killed 85`, reads as coverage of the tree
    # rather than of the files this run edits. Saying so at every run is what
    # stops the number being read as more than it is.
    covered = ", ".join(sorted(os.path.basename(path) for path in SOURCES.values()))
    # **And which suite each was run against**, because they are not one.
    # A run that names a suite it did not use for every operator is the same
    # class of claim this whole script exists to refuse.
    plain = " and ".join(name.rstrip(":") for name in SUITE)
    # **Every filter the entry adds, not the last one.** A `SUITES` value is the
    # whole filter list a source is run behind, so `names[-1]` names whichever
    # filter happens to be written last: `policy` runs behind `policy::` *and*
    # `check::`, and "also against check" drops the one its own comment records
    # as turning false survivors into kills. What a reader needs is what the
    # entry adds to the default pair, which is what is subtracted here.
    beyond = {
        source: [name.rstrip(":") for name in names if name not in SUITE]
        for source, names in SUITES.items()
        if source in SOURCES
    }
    extra = ", ".join(
        "{} also against {}".format(
            os.path.basename(SOURCES[source]), " and ".join(adds)
        )
        for source, adds in sorted(beyond.items())
        if adds
    )
    # **And the sources whose judge is another crate's suite**, which the two
    # phrases above cannot describe: they name filters over one command.
    # **The file, not the key.** A key is a short name for a source rather than
    # its file name, so a line built from `ELSEWHERE`'s keys with an extension
    # glued on prints names no directory holds, and gives the *package* or a
    # cargo flag as the suite each is judged by. A summary that names files
    # nobody can open is the same claim this script exists to refuse.
    # **And the suite, which is the filter and not the flag.** `names[-1]` is
    # whatever ends the command — the package again, or a bare `--lib` where a
    # suite was promised. What names a suite is the filter after `--`, and where
    # there is none the whole package is the suite — which is what the line
    # should say.
    # **And the second command, where there is one.** `ALSO_RUN` is part of what
    # judges a source, so a sentence that named only the first would be the same
    # kind of half-true claim the two rules above refuse. It is written into
    # the entry's own phrase rather than appended to the line, because the reader
    # of this half splits the line on `, ` and a clause of its own would be read
    # as another source.
    away = ", ".join(
        "{} against {}{}".format(
            os.path.basename(SOURCES[source]),
            judged_by(names),
            f" and {judged_by(ALSO_RUN[source])}" if source in ALSO_RUN else "",
        )
        for source, names in sorted(ELSEWHERE.items())
        if source in SOURCES
    )
    tail = f", and {away}" if away else ""
    return (
        f"over {len(SOURCES)} file(s) and {len(WEAKENINGS)} operator(s) — {covered} — "
        f"and the {plain} suites, with {extra}{tail}"
    )


def main():
    # **The summary without the run**, so the sentence can be read by something
    # other than a person at the end of forty minutes.
    if "--summary" in sys.argv:
        print(the_summary())
        return 0
    if "--list" in sys.argv:
        for _, predicate, label, _, _ in WEAKENINGS:
            print(f"{predicate}: {label}")
        return 0
    if refuse_if_a_previous_run_was_killed():
        return 2
    # Before the first edit, because a run that cannot judge eight of its files
    # is one nobody should have to read the tally of.
    if refuse_if_the_database_is_absent():
        return 2
    # Before the first edit too, and for the same reason: the kernel's files are
    # judged against a component, and it has to be the one this tree builds.
    if refuse_if_the_plugins_do_not_build():
        return 2
    # And before the first edit too: a suite already failing is a kill for
    # every operator it judges. See `refuse_if_a_suite_fails_unweakened`.
    if refuse_if_a_suite_fails_unweakened("as it was handed"):
        return 2
    original = {name: open(path).read() for name, path in SOURCES.items()}
    os.makedirs(BACKUPS, exist_ok=True)
    backups = []
    named = []
    for name in SOURCES:
        backup = os.path.join(BACKUPS, f"{name}.rs")
        open(backup, "w").write(original[name])
        backups.append(backup)
        named.append(f"  {SOURCES[name]}\n    pristine text: {backup}\n")
    # **Written before the first edit**, so the window it covers is every moment
    # a source on disk is not the source this run was handed.
    open(SENTINEL, "w").write(
        f"{SENTINEL} is on disk, so the last run of this file was killed before\n"
        f"it restored the tree. One of these may hold a weakening:\n" + "".join(named)
    )
    tally = {
        "killed": 0,
        "SURVIVED": 0,
        "does not compile": 0,
        "not applicable": 0,
        "did not finish": 0,
    }
    outcomes = []
    seen = None

    def restore(*_):
        """Put every source back and build the plugins from it, whatever ended the run.

        **A `finally:` does not run when the process is signalled.** Killing a
        run is ordinary, because a pass runs the corpus suite once per operator
        and there are dozens of them — see `xtask/canaries/README.md`, which
        keeps the rule and deliberately keeps no duration. Killing one left
        `xtask/src/limits.rs` holding a weakening, and the next thing to
        read the tree read the mutant. `rsync -a` and `cp -a` then carry an
        mtime older than `target/`, so cargo does not rebuild and the planted
        clause survives inside a test binary that reports green.

        Registered for SIGTERM and SIGINT and called by the `finally:` too, so
        the three ways a run can end all leave the tree as they found it.

        **The fourth way is a `SIGKILL`, and no handler catches it.** What
        covers that is `SENTINEL`: written before the first edit, unlinked here
        last of all, and read by `refuse_if_a_previous_run_was_killed` at the
        top of the next run. Measured — a `-9` left `xtask/src/gates.rs` holding
        `.any(|(named, found)| found == digest)`, the repository half of a digest
        pin gone, and the next run named the file and its pristine backup rather
        than reading the weakening as `original` and writing it back.
        """
        for name, path in SOURCES.items():
            open(path, "w").write(original[name])
        subprocess.run(TOUCH, cwd=ROOT, check=False)
        # **And the component, built again from what was just put back.** An
        # operator over the plugin's sources rebuilds it from the weakening, so
        # a restore that stops at the sources leaves a component no source on
        # disk builds — and the next run reads it through the kernel's suites.
        if not build_the_plugins():
            print(
                "  the plugins did not build from the restored sources; run "
                "`cargo xtask build --plugins` before any suite reads a component."
            )
        # Last, so the sentinel outlives every write it is about.
        if os.path.exists(SENTINEL):
            os.unlink(SENTINEL)

    for caught in (signal.SIGTERM, signal.SIGINT, getattr(signal, "SIGHUP", None)):
        # **The hangup where the system has one.** Windows has no `SIGHUP`, and
        # naming it there raises before the first operator. Read by name, so a
        # system that has it catches it here with the other two.
        if caught is not None:
            signal.signal(caught, lambda number, frame: (restore(), sys.exit(128 + number)))
    try:
        for source, predicate, label, frm, to in WEAKENINGS:
            if predicate != seen:
                print(f"\n{predicate}")
                seen = predicate
            if original[source].count(frm) != 1:
                print(f"  ??  {label:<52}  the text is not unique or not present")
                tally["not applicable"] += 1
                continue
            # **Every source back to original, not just the one being edited.**
            # Writing only `source` leaves the *other* file holding the previous
            # weakening, so from the first `limits` entry onward every `gates`
            # run is red for a reason that is not the mutation — measured, and
            # reported `killed` by a corpus that had not noticed them.
            # A meta-gate that cannot tell its own leftovers from a kill is
            # exactly the defect it exists to find.
            for name, path in SOURCES.items():
                open(path, "w").write(original[name])
            open(SOURCES[source], "w").write(original[source].replace(frm, to))
            verdict = run_corpus(source)
            tally[verdict] += 1
            outcomes.append((label, verdict))
            mark = {
                "killed": "ok",
                "SURVIVED": "!!",
                "does not compile": "--",
                "did not finish": "!!",
            }[verdict]
            print(f"  {mark}  {label:<52}  {verdict}")
            # **And the component, built again from the tree as handed, before
            # the next operator.** This one built it from its weakening, and a
            # kernel operator after it would be judged by a suite reading a
            # component no source on disk builds. See `builds_the_plugins`.
            if builds_the_plugins(source):
                open(SOURCES[source], "w").write(original[source])
                if not build_the_plugins():
                    print(
                        f"\n  refused: the plugins did not build from the tree as "
                        f"handed after `{label}`, so every kernel operator after it "
                        "would be judged by a suite reading a component no source "
                        "on disk builds.",
                        flush=True,
                    )
                    raise SystemExit(2)
    finally:
        restore()
        for name in backups:
            os.unlink(name)
        # Last, and only if it is empty: a file left here is a copy somebody may
        # still need, and removing the directory around it would be the same
        # class of loss as a copy kept in `/tmp`.
        try:
            os.rmdir(BACKUPS)
        except OSError:
            pass
    print()
    print("  ".join(f"{name} {count}" for name, count in tally.items()))
    # **And once the tree is back**: a suite that started failing in the middle
    # of the run made the kills since the control before it worth nothing.
    if refuse_if_a_suite_fails_unweakened("put back after the run"):
        return 2
    print(the_summary())
    # **`not applicable` is a failure.** An operator whose text no longer
    # matches is a clause with no reported coverage, and a run that still exits
    # 0 lets operators rot unnoticed under any tally written about them — the
    # reason `xtask/canaries/README.md` writes none.
    # A weakening that cannot be applied is not a weakening that was survived and
    # it is not one that was killed: it is a hole, and a meta-gate that cannot
    # tell a rotted operator from a covered clause is the defect it exists to
    # find, one level down.
    # **`does not compile` is the same hole.** An operator whose weakening no
    # longer builds tests nothing — the corpus never ran. All three non-`killed`
    # outcomes cost the same, because all three mean the clause was not
    # decided.
    # **The survivor *set*, not the survivor count.** `!= sum(...)` makes a clean
    # tree exit 1 for ever, because some mutants are known-equivalent and always
    # survive — so the exit code says the same thing whether those survive or
    # thirty real ones do, and the only real signal is a person diffing the
    # printed list against a paragraph in another file. A gate whose verdict
    # cannot change is not a gate.
    #
    # Each accepted survivor is written down with the argument for why no input
    # can tell it from the original. A new one changes the set, and the set is
    # what decides.
    survived = {label for label, verdict in outcomes if verdict == "SURVIVED"}
    if survived != EQUIVALENT:
        for label in sorted(survived - EQUIVALENT):
            print(f"  !!  {label:<52}  survived and is not a written equivalent")
        for label in sorted(EQUIVALENT - survived):
            print(f"  ??  {label:<52}  is written as equivalent and was killed")
        return 1
    # A mutant that never finished is a mutant nobody judged, which is what
    # `not applicable` and `does not compile` already mean here.
    return (
        1
        if tally["not applicable"] or tally["does not compile"] or tally["did not finish"]
        else 0
    )


if __name__ == "__main__":
    sys.exit(main())
