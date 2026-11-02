# The `family` pack

`reference/identity-and-authorization.md §3` gives this profile four defaults:
**`guardian_restriction` active, autonomy by age, succession on, low quorum
(2 of 3)**.

Two of the four are here, and part of a third. `guardian_restriction` is
`family.a_guardian_administers_the_realm`, and the quorum is the core's
`core.destructive_needs_quorum` — the *threshold* (2 of 3) is not, since Cedar
is told whether quorum was met and not how it is counted. The decision point
reads it as met only from an approval the kernel issued for the one dispatch
(`crates/nc-kernel/src/dispatch.rs`), and counting the approvers is the
approval system's, which `NOCTUA-PLAN.md` builds in Stage 23; until then only a
test door issues one.
Of succession, the end of the ladder is here: an account acts only while its
`status` is `active`, `dormant` or `presumed_inactive`, so one that is
`deceased` or `suspended`, or holds a value this pack does not know, acts on
nothing (`family.a_frozen_account_acts_on_nothing`).

## What each file holds

Every rule is a `permit` unless it says otherwise, and every `forbid` beats every
`permit` — in this pack and in the core — so a permit here widens what is
allowed and never reaches past a floor.

| File | Rules | What they are about |
|---|---|---|
| `permissions.cedar` | `family.a_guardian_administers_the_realm`, `family.a_member_reads_shared_content`, `family.an_owner_governs_their_own_content`, and the `forbid` `family.giving_out_encrypted_content_needs_quorum` | content and permissions: who reads what the household shares, who changes what they own, who grants, and the quorum for handing out an encrypted content key. The guardian's `Grant` and `Revoke` name no resource type and ask no membership, so they reach any `Realm`, one she is not in included — Cedar's answer over the whole world, pinned by a case and recorded in `../../core/README.md`, and one the decision point in `nc-authz` denies, because a realm's slice holds no other realm |
| `realm.cedar` | `family.a_guardian_reads_the_realm`, `family.a_guardian_prunes_the_realm`, `family.a_member_steps_through_their_own_history`, `family.a_member_reads_their_own_account` | the realm itself as the resource — the commands that act on the whole realm (the journal verified, the tables pruned, a step undone or redone) — and a person reading their own account. Each permit here that reaches the realm requires `principal in resource`, so a member of another realm is refused them; the undo and redo permit also requires `principal is User`, so a device or an agent in the realm is refused a person's history |
| `accounts.cedar` | the `forbid`s `family.a_frozen_account_acts_on_nothing` and `family.an_unattested_device_acts_on_nothing` | who may act at all: a frozen account and a device nobody attested are refused whatever they ask |

**Each file is also a line of `crates/nc-authz/src/embedded.rs`**, in the pack's
own list: a kernel decides with the pack compiled into it, and a policy on disk
that no line embeds would pass every case here and be enforced by no kernel —
which that file's test refuses, in both directions.

Each `.cedar` file has its cases in `tests/`, under the same name. Every rule has
a denial case, and every constraint of every permit is pinned by a case that
changes its answer when the constraint is removed; a constraint no request the
schema can build could ever show working is left out of the permit rather than
given a case that could not exist, and the permit says which, beside it.

A `forbid` here is a floor, and `cargo xtask check` refuses one that names an
entity. It may say what kind of thing it is about, in its scope, or in a
condition beside the read of that subject's attribute it guards
(`resource is Content && resource.sensitivity …`). A type test elsewhere in a
condition is refused: beside one disjunct, it takes every other kind of
resource out from under the floor. The gate asks about reads across the whole
rule, so each floor's cases are also written on every kind of resource a
permit reaches.

A type test on *who is asking* is not one of those. A request from a device is
refused by `principal is User` for being a device, so the `allow` it pairs with
is necessarily another kind of principal's; such a case says
`the_principal_is_the_boundary = true`, and the runner asks the same request as
the principal of some such `allow` and requires it permitted. That is how the
history permit's type test is pinned: without the flag it is a constraint no
request can show working, and deleting it — which lets an attested device and
an agent undo a person's history — leaves every case green. Either boundary flag belongs on a `no_permit_matched`
denial only. On any other case nothing would judge it, so the runner refuses
it.

## What is deliberately not here yet

**Autonomy by age.** The rule is real and the attribute for it exists —
`User.birth_date`, the date and never a computed age (`NOCTUA.md §14.3`). What
does not exist is a way to write it correctly: Cedar's `duration` has no calendar
units and no division, so "at least eighteen years old" can only be spelled as a
number of days. Every constant available is wrong — `365` per year lets a child
through up to five days early, `366` holds an adult out by nearly two weeks — and
an age gate that is wrong by days is worse than one that is visibly absent.

It arrives with the age attributes `NOCTUA-PLAN.md §P27.3` defers, as either
an age band derived at decision time and passed in the context, or a Cedar
extension of ours. Neither is a stored age: `§14.3`'s rule is that the age is
derived per request, not that the derivation happens inside Cedar.

**The succession ladder itself.** `§14.7`'s dormant → presumed-inactive →
deceased transitions need something that writes `status`, and nothing does until
Stage 23 — so what is here is the frozen state at the end of the ladder, not the
quorum of successors that moves an account onto it. A device or an agent acting
for a frozen account is not refused by this pack either: whether they stop with
their owner is `§14.5`'s revocation, a credential withdrawn where it is served.

Both are recorded here rather than left as an empty file, because a pack that
looks complete is one nobody checks against the reference again.

## The version

`pack.toml` holds the pack's version and nothing else. A realm whose profile is
`family` pins it as `family@<version>` in its `policy_pack` (`NOCTUA.md §14.4`),
and `cargo xtask check` refuses a realm in `policy/world.json` that pins any
other. Nothing here remembers what a policy said before it changed
(`NOCTUA.md §5.3`), so no gate raises the version for you: raising it, with
every such realm's pin in the same edit, is the reviewed migration §14.4 asks
for.
