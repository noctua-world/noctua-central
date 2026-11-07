#!/bin/sh
# What one whole gate run adds to the temporary directory, and nothing is the
# answer this holds.
#
# **Not a step inside `cargo xtask check`, and the reason is written in
# `xtask/src/check.rs`**: the temporary directory is shared with every other
# process on this machine, so a gate counting it would go red for somebody
# else's mess. What a recipe can do is watch the *difference* across one run,
# which is attributable to this run and reproducible by anyone.
#
# **The gate runs unconditionally and the report is in a trap.** Measuring
# around `cargo test` is not an option, because a second thing that runs the
# suite is a second gate; `|| true` and `set +e` are not, because they throw away
# the verdict that is the point of running one; and an `if` is not, because a
# command inside a condition may not run at all. A trap on EXIT needs none of
# those: the gate runs, its status is this script's status, and the count is
# printed whichever way it went. That matters, because **a failing run is the
# one that leaks** — the removal a test writes at its own foot is the line a
# failing test never reaches.
#
# It takes no argument and removes nothing outside what it lists.
set -eu
cd "$(dirname "$0")/.."

# **Where the tests actually write.** Every scratch goes through
# `std::env::temp_dir()`, which reads `TMPDIR` — so a hard-coded `/tmp` reports
# a number about a directory nothing wrote to, wrong in both directions at once.
where=${TMPDIR:-/tmp}

before=$(mktemp)
after=$(mktemp)
trap 'find "$where" -maxdepth 1 -name "nc-*" | sort > "$after"; comm -13 "$before" "$after"; printf "%s director(ies) left behind, and 0 is what this holds\n" "$(comm -13 "$before" "$after" | wc -l)"; rm -f "$before" "$after"' EXIT

find "$where" -maxdepth 1 -name "nc-*" | sort > "$before"

cargo xtask check

find "$where" -maxdepth 1 -name "nc-*" | sort > "$after"
left=$(comm -13 "$before" "$after" | wc -l)
[ "$left" -eq 0 ]
