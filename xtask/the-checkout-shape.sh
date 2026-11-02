#!/bin/sh
# What `cargo xtask check` does in a checkout, measured rather than reasoned.
#
# **CI has never run**, by the decision in `NOCTUA.md §5.3`: there is no
# repository to push this tree to. What a reader is owed instead is the *shape* a
# checkout produces, and until now that shape lived in a paragraph of
# `NOCTUA-PLAN.md` — measured once, by hand, and never again. A measurement
# nobody can reproduce decays into prose, and one of its numbers had already
# rotted between being pasted and being read.
#
# This is that measurement as a command. It reproduces the two things a checkout
# is and a developer's tree is not:
#
#   * **no workspace above it.** `sections::workspace_of` walks up for a
#     `.noctua-workspace` marker; a checkout has none, so the steps that read the
#     documents above this repository announce that they could not.
#   * **no sibling `noctua-design`.** The design system is a separate repository
#     and a checkout of this one does not bring it.
#
# It does **not** reproduce a cold machine, but it does run the step CI runs
# before the gate: `cargo xtask dev` brings up a stack that belongs to the copy,
# migrated, and the copy's gate is run against that and nothing else. A
# database that has applied nothing is a failure and not an announcement —
# `check.rs`'s `Record` says so — so it would not be part of this shape anyway.
#
# **`cargo xtask dev` and `cargo xtask check` and nothing else** — the two CI
# runs — because `xtask/src/workflow.rs` refuses any other cargo invocation
# reachable from the justfile. That is also
# why the copy needs no explicit build: the alias is `run --locked --package
# xtask --`, so cargo rebuilds the binary in the copy — which matters, because
# `env!("CARGO_MANIFEST_DIR")` is baked at compile time and a copied `target/`
# runs a binary that reads the tree it was built in.
set -eu

# **It must not run inside a checkout it made.** The last thing this script does
# is `cargo xtask check` in the copy, and `check` drives this script — so in the
# one shape this script exists to produce, a checkout with no `.noctua-workspace`
# above it, the marker walk that stopped the second level never fires. Every
# level then copies the tree and starts another. Measured on F5's seventh pass,
# in a checkout under `/tmp`: three levels and 3.9 GB in under five minutes,
# still growing, and it stops when the disk does — `$1` is given at every level,
# so `keep` is non-empty and no level removes its own copy.
#
# A recursion lives in the process tree, so the guard is there and not on disk:
# the copy is a faithful checkout, which is the whole point of it, and nothing
# written into it could tell the difference. The name is exported below, once,
# immediately before the `cargo xtask check` that can come back here.
case ${NOCTUA_THE_CHECKOUT_SHAPE:-} in
  ?*)
    echo "this is already running inside a checkout this script made" >&2
    echo "($NOCTUA_THE_CHECKOUT_SHAPE). The last step here is" >&2
    echo "\`cargo xtask check\`, and \`check\` drives this script, so a second" >&2
    echo "level would copy the tree again and a third would copy that one." >&2
    echo "Run it from a tree this script has not already copied." >&2
    exit 3;;
esac

# **One leading slash, because two are a different path to POSIX and the same
# directory to the kernel.** `cd //x && pwd -P` answers `//x` verbatim — the
# standard leaves exactly two leading slashes implementation-defined and both
# `dash` and `bash` keep them. Three or more collapse to one by the standard and
# two are the only special case; nothing in this tree wants the special case, so
# the answer is one slash.
#
# **Written once, and spent on every operand the guards compare.** Round
# sixty-four found `//` in front of a *destination* walking past all four
# containment guards at once, and collapsed it — on the destination. `$here` is
# built by `pwd -P` too, and the shell hands `pwd` whatever `$0` was spelled
# with: reach this script through `sh //<root>/xtask/the-checkout-shape.sh` and
# `$here` carries the doubled slash while the destination does not, so
# `"$here"|"$here"/*` and `"$scratch"/*` match nothing again — the repository
# itself, anything inside it, and anything containing it, and the two lines
# after the guards are an `rm -rf` and a `cpio`. One collapse was tested and the
# other was decided by nothing in either direction. Measured in round
# sixty-five, one character apart, against a scratch repository. So the rule is
# a function rather than a paragraph, and both operands are spent through it.
one_leading_slash() {
  a_path_being_collapsed=$1
  while :; do
    case $a_path_being_collapsed in
      //*) a_path_being_collapsed=${a_path_being_collapsed#/};;
      *) break;;
    esac
  done
  printf '%s' "$a_path_being_collapsed"
}

# **`CDPATH=` and `--`, because `cd` with a relative operand is not quiet.** When
# `CDPATH` is set — `.:..:~` is an ordinary interactive setting — `cd` searches it
# and **prints the directory it found on standard output**, inside this command
# substitution. `$here` then holds the path twice, separated by a newline, and the
# confirmation below refuses the documented invocation with a two-line path in the
# message. Measured in round sixty-seven from the repository root, `sh` and `bash`
# alike: `CDPATH=".:.." sh xtask/the-checkout-shape.sh <scratch>` exits 2 where the
# same command with `CDPATH` unset does the work. `--` because a repository whose
# path begins with `-` is a path, not an option.
here=$(one_leading_slash "$(CDPATH= cd -P -- "$(dirname -- "$0")/.." && pwd -P)")

# **And `$here` is checked against the one thing that can confirm it: this
# file.** `pwd -P` resolves every symlink in a path *to a directory*, and
# `dirname` has already thrown the filename away — so nothing above can see that
# the script itself was reached through a link, or by a bare name `PATH`
# resolved, or on standard input. In each of those `$0` names a place this
# repository is not, and `$here` becomes some other directory entirely.
#
# That is worse than any single guard being wrong. The two containment guards
# below read `$here` from opposite sides — "is the destination inside it" and
# "does the destination contain it" — so a `$here` that is somewhere else
# satisfies **both at once**, and the marker walk is no backstop because a
# checkout, which is the very thing this script produces, has no marker above
# it. Measured in round sixty-six against a throwaway repository: a symlink to
# this script on a user's own command path, and `sh the-checkout-shape.sh`
# found on `PATH` from an unrelated directory, each walked past all four guards
# and reached `rm -rf` of the repository *root*. It was wrong in the other
# direction too, refusing an ordinary scratch that names nothing inside.
#
# Round sixty-five wrote down that the canonicalisation rule is spent on every
# operand the guards compare, and then spent it on the two paths without asking
# where the second one came from. This is that operand.
#
# There is no `realpath` here, for the reason the paragraphs below give — it is
# not on every machine this has to run on. So `$here` is not *resolved*, it is
# **confirmed**: the directory a correct `$here` names is the one holding this
# file, at the path this file is kept at, and a directory that does not hold it
# is not this repository whatever else is true of it.
# **The kernel is asked whether that is *this file*, not whether a file is
# there.** `-f` follows symlinks, so the very link that makes `$here` wrong
# satisfies a test for a file at that path — measured in round sixty-seven, two
# ways, each reaching the destructive line with the guards silent: a symlink to
# this script placed at `<somewhere>/xtask/the-checkout-shape.sh`, and a
# **hard** link at the same place. Round sixty-six closed the one arrangement
# where the link's grandparent has no `xtask/` and left the class, which is the
# fourth consecutive round something has reached `rm -rf` through this
# derivation.
#
# **A third arrangement was written here as a danger and is not one.** A plain
# directory symlink `<somewhere>/xtask` pointing at this repository's own
# `xtask/` — the kind of convenience link people really make — was recorded as
# reaching `rm -rf` with the guards silent. It does not: `cd -P` above resolves
# that link *physically before* it spends the `..`, so `$here` comes out as this
# repository and the confirmation passes, which is correct. Measured in round
# seventy-one. What that arrangement is good for is holding the `-P` itself:
# give the decoy a `Cargo.toml` and a `crates/` of its own and every marker
# answers through the link, so with the `-P` gone `$here` becomes the decoy, the
# two containment guards compare against the wrong tree from both sides and
# agree, and the marker walk is no backstop because a checkout has none above it
# either — exit 77 where it should be 2. That is what
# `a_link_to_this_script_does_not_make_its_directory_the_repository` drives now.
#
# `-ef` compares device and inode. A symlink, a hard link, a bind mount and a
# second checkout all answer *no* to it and all answer *yes* to `-f`. It is not
# in the POSIX `test` list, but it is in `dash`, `bash`, `ksh` and `busybox`,
# which is every shell this can run under; on one that lacks it the test errors,
# which is non-zero, which refuses — the safe direction, and the one a guard
# before `rm -rf` must fail in.
# **And a link to this file is this file, which one marker cannot tell.** `-ef`
# compares device and inode *after* following symlinks, so a symlink placed at
# `<somewhere>/xtask/the-checkout-shape.sh` answers yes to it — and a hard link
# answers yes by construction. Both were measured reaching the destructive line
# with the single-marker confirmation in place. What distinguishes the
# repository from a directory holding a link to one file in it is the rest of
# the repository: a workspace manifest, `xtask`'s own manifest, and the crates
# directory. Nobody arrives at all four by making a convenience link, which is
# the accident this guards against; and if any of them is ever renamed this
# refuses loudly rather than guessing, which is the direction a guard before
# `rm -rf` must fail in.
the_repository_holds() {
  for held in \
    "xtask/the-checkout-shape.sh" \
    "xtask/Cargo.toml" \
    "Cargo.toml" \
    "crates"
  do
    # No `||` anywhere in this file: the gate that reads it refuses a command
    # whose failure has an alternative, and it is right to — that is how a
    # verdict gets thrown away. Written as `if`, which says the same thing and
    # says it where a reader looks.
    if [ ! -e "$1/$held" ]; then
      return 1
    fi
  done
  if ! [ "$0" -ef "$1/xtask/the-checkout-shape.sh" ]; then
    return 1
  fi
  return 0
}
if ! the_repository_holds "$here"; then
  echo "this script was reached as $0, so the repository would be $here — and that" >&2
  echo "directory is not the repository this script belongs to. Every guard below" >&2
  # No backticks in this message: inside double quotes they are command
  # substitution, and this script is the wrong place to learn that twice.
  echo "compares against it, and a removal follows them. Run the script by its" >&2
  echo "own path inside the checkout." >&2
  exit 2
fi

scratch=${1:-/tmp/nc-checkout-shape-$$}

# **Absolute before it is compared, and before it is walked up from.** The guard
# below compares the argument against an absolutised `$here`, so a *relative*
# one matched nothing however far inside this repository it pointed: `sh
# xtask/the-checkout-shape.sh copy` passed the guard and then `rm -rf`'d and
# copied the repository into a directory inside itself. And `dirname` on a
# relative path converges on `.` rather than on `/`, so the marker walk below
# never ended. Both reported on 2026-09-13 by a reviewer reading a script
# nothing runs.
case $scratch in
  /*) ;;
  *) scratch=$PWD/$scratch;;
esac

# **Resolved before it is compared.** Absolute is not the same as canonical:
# `<root>/xtask/../probe` and a symlink pointing at this repository are both
# absolute, both name a place inside it, and both walked straight past a
# comparison over characters. Measured on 2026-09-14 by a reviewer driving this
# guard in a fake checkout: the same directory was refused written plainly and
# accepted written with `..` or through a link, and the next two lines are an
# `rm -rf` and a 1.5 GB `cpio` of this repository into itself.
#
# The deepest ancestor that exists is resolved with `cd`+`pwd -P`, which follows
# every link and spends every `..`; the part that does not exist yet is carried
# along as a tail. No `realpath`, which is not on every machine this has to run
# on, and no `-e` test, which would refuse a target that is not there yet.
tail=
probe=$scratch
while [ ! -d "$probe" ]; do
  parent=$(dirname "$probe")
  if [ "$parent" = "$probe" ]; then
    break
  fi
  tail=$(basename "$probe")${tail:+/$tail}
  probe=$parent
done
scratch=$(cd "$probe" && pwd -P)

# **The tail is normalised, not merely carried.** Resolving only the deepest
# ancestor that exists leaves every `..` in the part that does not exist yet
# unspent, and `mkdir -p` and `cpio` spend it through the kernel afterwards. So
# `<somewhere that is not there>/../<inside this repository>` was compared, and
# walked up from, as a path that names nowhere — while the copy landed inside the
# repository all the same. Measured on 2026-09-14 by a reviewer driving this
# guard from a scratch repository: the same directory refused written plainly and
# accepted written through one absent component and a `..`, defeating *both*
# guards below with the same two characters.
#
# The prefix above is already canonical — `pwd -P` has followed every link and
# spent every `..` — so popping a `..` off it lexically cannot step through a
# link into somewhere else, which is the one thing that makes textual
# normalisation wrong in general. Nothing in the tail exists as a directory, by
# the loop's own condition, so there is no link in it to follow.
#
# **And the kernel answers again at every step it can.** The paragraph above
# claimed "nothing in the tail exists as a directory, by the loop's own
# condition, so there is no link in it to follow", and that is false: the probe
# loop tests whole *prefix paths*, not components, so an absent component
# followed by `..` leaves a perfectly real symlink sitting in the tail.
# Composing the two spellings this file already refuses separately —
# `<absent>/../<link>/probe` — produced a path that is absolute, looks
# canonical, and is not; the guards compared it, the marker walk climbed it, and
# `rm -rf` landed somewhere else. Measured end to end in round sixty-four
# against a throwaway repository: it was destroyed through the link and copied
# into itself, which is the third consecutive round this script has been walked
# past.
#
# Three rounds of canonicalising the argument *textually* have each closed one
# spelling and left the next, so this stops reasoning about the path. A `..` is
# spent lexically — it must be, because what it pops may not exist — and then,
# the moment what has been built so far exists, `cd`+`pwd -P` is asked again.
# The kernel is what will resolve this path for `rm -rf`, and it is the only
# thing that can say where a link points.
rest=$tail
while [ -n "$rest" ]; do
  part=${rest%%/*}
  case $rest in
    */*) rest=${rest#*/};;
    *) rest=;;
  esac
  case $part in
    ''|.) ;;
    ..) scratch=$(dirname "$scratch");;
    # `${scratch%/}` so the root does not become `//name`; every other path
    # loses nothing, having no trailing slash to lose.
    *) scratch=${scratch%/}/$part;;
  esac
  if [ -d "$scratch" ]; then
    scratch=$(cd "$scratch" && pwd -P)
  fi
done

# **And the destination through the same collapse.** `//` in front of *any*
# destination made every comparison below match nothing at once: the repository
# itself, anything inside it, anything containing it, and the root. Four guards,
# six rounds of closed spellings, and two characters that walk past all of them.
# Measured in round sixty-four, against a script that had already been repaired
# twice that afternoon — and the round-sixty-three paragraph even lists `//`
# among the spellings that reached `rm -rf`, while the repair it shipped strips
# a *trailing* slash and never saw it. The rule itself is written once, above
# `here`, where the reason it has two operands is written down.
scratch=$(one_leading_slash "$scratch")

# **Not under the workspace**, which is the whole point: a scratch directory
# inside it would find the marker by walking up.
#
# **A separator, not a bare prefix.** `"$here"*` is a comparison over characters
# and not over directories, so a sibling whose name merely begins with this
# repository's — `<root>-probe`, `<root>.old`, the copy a developer makes by
# appending a word — was refused as though it sat inside. The two cases below
# are the directory itself and anything under it, which is what "inside" means.
case $scratch in
  "$here"|"$here"/*) echo "the copy must sit outside this repository, not at $scratch" >&2; exit 2;;
esac

# **Outside is two directions, and only one of them had been asked about.** Four
# rounds closed "inside" — relative, `..`, a link, a sibling sharing a prefix —
# and none asked about *above*. A scratch that contains this repository passes
# the case above reading left to right, and the next lines are an `rm -rf` of it:
# `sh xtask/the-checkout-shape.sh ..` from `xtask/` destroys the repository, and
# the directory above the workspace destroys every repository in it. The marker
# walk below does not save that either — only the workspace root carries
# `.noctua-workspace`, so everything above it is unmarked. Measured on
# 2026-09-14 by a reviewer driving this from a scratch repository: the
# repository and an unrelated sibling were gone, and `checkout` sat where they
# had been. **There is no git in this tree** (`NOCTUA.md §5.3`), so what that
# spends is not recoverable.
#
# The caller names what is destroyed; that is the contract, and naming `/tmp/x`
# destroys `/tmp/x`. What this refuses is a name that destroys something the
# caller did **not** name.
# **And the root, which the case below cannot express.** `"$scratch"/*` builds the
# pattern `//*` when `$scratch` is `/`, and `//*` matches no ordinary absolute
# path — so `/` walked past both guards and reached the `rm -rf` while `/tmp`,
# which contains strictly less, was refused. The marker walk does not save it
# either: `above` starts at `//checkout` and `dirname //checkout` is `/`, so the
# loop ends after one step. Reported in round sixty-three by a reviewer driving
# the guard from a scratch repository, with `/` and with `/tmp/..`, which
# resolves to the same place.
#
# Written as its own case because the obvious repair does not work: bash does not
# match a pattern built from a quoted empty expansion, so `case $here in
# "${scratch%/}"/*)` matches nothing at all and refuses everything *less* than
# the root instead.
case ${scratch%/} in
  "") echo "the copy must sit outside this repository, and $scratch contains it" >&2; exit 2;;
esac
case $here in
  "$scratch"/*) echo "the copy must sit outside this repository, and $scratch contains it" >&2; exit 2;;
esac

# A marker anywhere above the copy would defeat it. `/tmp` has none; say so if it
# ever does rather than reporting a shape that is not the shape.
#
# **Before the copy and not after it.** This walk stood beneath a 1.5 GB `cpio`,
# so a target that was never going to be a checkout was destroyed and rebuilt
# first and told afterwards. Walking a path is textual — `dirname` needs nothing
# on disk — so there is no reason to pay for the copy before refusing.
above=$scratch/checkout
while [ "$above" != "/" ]; do
  parent=$(dirname "$above")
  # A `dirname` that answers what it was given has stopped climbing, which is
  # what a relative path does at `.`. The absolutising above makes that
  # unreachable; this is what makes it unreachable *and* finite.
  if [ "$parent" = "$above" ]; then
    break
  fi
  above=$parent
  if [ -f "$above/.noctua-workspace" ]; then
    echo "a workspace marker sits at $above, so this copy is not a checkout" >&2
    exit 2
  fi
done

rm -rf "$scratch"
# **The copy sits one level down and the transcript one level up**, because the
# root of a checkout is where a file named by nothing is invisible to every kind
# rule — and `nothing_unnamed_sits_at_the_top_of_this_repository` refuses one,
# rightly. The first draft wrote `check.log` into the copy and the copy failed
# its own gate for holding it.
mkdir -p "$scratch/checkout"
# `target/` is left behind on purpose: a checkout has none, and copying one would
# hand the copy a binary baked to this tree.
( cd "$here" && find . -path ./target -prune -o -print | cpio -pdm --quiet "$scratch/checkout" )

cd "$scratch/checkout"
# **Run plainly, with `set -e` left on and no `||` and no `if`.**
# `xtask/src/workflow.rs` refuses all three in anything the justfile reaches, and
# is right to about each: `set +e` stops a block minding its own failures, `||`
# swallows one, and a cargo invocation inside an `if` is a gate whose running is
# decided at run time. Those rules do not distinguish *capturing* a verdict from
# *choosing* whether to produce one, and they should not have to.
#
# So the verdict is this script's own exit status, and the transcript reaches a
# reader through a trap rather than through a line after the command — which is
# what a `set -e` abort would skip.
#
# **And the copy goes with it, because nothing removed it.** Each is 1.5 GB and
# three were found sitting in `/tmp` on one machine, from three runs nobody
# thought about again: the transcript was printed and the tree was left behind.
# A caller who names a directory keeps it — that is what the argument above is
# for, and it is how a copy is kept for inspection — and the default one, which
# nobody named, goes as soon as the transcript has been printed.
keep=${1:+the caller named it}
# **And the copy's stack goes before the copy, from inside it and nowhere
# else.** `dev --down` removes the containers *and volumes* of the checkout it
# is run in — the compose project is named for that checkout's path — so the
# `cd` is joined to it by `&&`, and a copy that cannot be entered takes nothing
# down. It runs only once there is a transcript, because the transcript is
# opened by the `dev` that brings the stack up: a run stopped before that has
# no stack to take down, and the drives of this file in `check.rs`, which stop
# above that line, never reach `cargo` at all. A `down` that fails stops the
# trap before the copy is removed, which is the right way round: the copy is
# the only place that stack can be taken down from.
trap 'cat "$scratch/check.log" 2>/dev/null; [ -f "$scratch/check.log" ] && cd "$scratch/checkout" && cargo xtask dev --down; case $keep in "") rm -rf "$scratch";; esac' EXIT
# **Exported here, above both calls in the copy.** `check` is the one that can
# come back: the guard at the top of this file reads it; nothing else does.
NOCTUA_THE_CHECKOUT_SHAPE=$scratch
export NOCTUA_THE_CHECKOUT_SHAPE
# **A stack of the copy's own, because the gate asks about its own.** CI runs
# `cargo xtask dev` in the checkout and then the gate, so the log store the gate
# asks docker about is that checkout's container. This used to run the gate
# against the developer's stack through `DATABASE_URL`, and the step that reads
# where *this project's* `logs` container mounts its volume found none — the
# project is named for the checkout's path (`project_for` in
# `xtask/src/dev.rs`), and the copy's path is not the developer's — so the copy
# failed on one line a checkout in CI never would. Found in Stage 11's
# acceptance.
#
# On ports of its own, because two stacks bind the same two by default and the
# second `dev` fails on the port; and with `DATABASE_URL` removed, so the gate
# derives the copy's database from `POSTGRES_PORT` the way it does in CI. A
# port that is taken fails `dev` loudly, in Docker's own words.
POSTGRES_PORT=55432
LOGS_PORT=59428
export POSTGRES_PORT LOGS_PORT
unset DATABASE_URL
cargo xtask dev > "$scratch/check.log" 2>&1
cargo xtask check >> "$scratch/check.log" 2>&1

# **The announcements, as a set.** One more arriving unremarked is the failure
# this exists to catch: every other census in this tree is an equality for the
# same reason, and the number here was written in three documents as *four* while
# the run produced five.
#
# **And this paragraph spelled the count too.** It said *a fifth one*, left from
# an earlier count and already one behind `expected=` below — and the census that
# holds the three documents to `expected=` does not read this file's prose, so
# two more copies of the number sat inside the very file the number is defined
# in. Found on F5's seventh pass, which moved `expected=` and all three documents
# together and got `602 passed` with the message telling a stranger the wrong
# thing. No prose in this file names the count now: the only number is
# `expected=`, the message interpolates it, and a test holds the message to
# naming none of its own.
# **`awk` rather than `grep -c … || true`**, for the same reason: `||` after a
# command is how a failure is swallowed, and `grep` exits non-zero when it
# matches nothing — which is exactly the case this number is about. `awk` counts
# and exits zero, so the count is the only thing that decides.
found=$(awk '/^  warn  skipped:/ { seen++ } END { print seen + 0 }' "$scratch/check.log")
# **`expected` below is the count on a machine holding the two pinned tags**, and
# it is the only place in this file the number is written. What it counts: the
# steps that cannot be answered without the workspace above this repository, and
# the one that cannot be answered without the design system beside it — none of
# which depends on this host.
#
# The image-pin step is the one that does depend on it: it asks the local store
# what each pinned tag resolves to, and a tag nobody has pulled is announced
# rather than compared. So a host that has only ever run `cargo xtask dev` makes
# **two more**, because compose resolves `tag@sha256:...` by the digest and the
# tag never reaches the store.
#
# That precondition is written here rather than left for whoever first runs this
# somewhere else, because the message below used to tell them the wrong thing.
expected=5
echo
echo "the checkout shape: $found announcement(s), and the gate passed"
if [ "$found" -ne "$expected" ]; then
  echo "expected $expected announcement(s) and this checkout made $found." >&2
  echo "All $expected of them are steps that cannot be answered without the" >&2
  echo "workspace above this repository, or without the design system beside" >&2
  echo "it. If you got more than that, the extra ones are the pinned images:" >&2
  echo "this host has not pulled their tags, and \`cargo xtask dev\` does not" >&2
  echo "pull them. Run" >&2
  echo "  docker pull postgres:18-alpine" >&2
  echo "  docker pull victoriametrics/victoria-logs:v1.52.0" >&2
  echo "and run this again. Otherwise a new announcement is a gate that has" >&2
  echo "quietly become local-only, and a missing announcement is a gate that" >&2
  echo "has stopped announcing: update this number and the documents that" >&2
  echo "state it in the same edit." >&2
  exit 1
fi
echo "a checkout passes, and says what it could not do."
