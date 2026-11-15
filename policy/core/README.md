# The invariant core

These policies hold in **every** pack (`NOCTUA.md §14.9`,
`reference/identity-and-authorization.md §9`). A pack may add to them and may
never contradict them, which is what lets one platform serve a family, a company
and a collective without becoming three products.

`NOCTUA.md §14.9` names seven invariants, and `§14.6` contributes several of its <!-- allow-count: the invariants §14.9 lists, which is fixed in that document rather than by this tree -->
own that belong here — rules 2, 3 and 5 have rows below, and rule 4 rides with
invariant 5 in `quorum.cedar`. **Some are policies and live in this directory; the rest are not**, and
saying which is which is the point of this file. The table is the record; `ls
policy/core/*.cedar` is the count.

| # | Invariant | Where it lives |
|---|---|---|
| 1 | Deny by default, and fail closed | Cedar's own model, plus `fail_closed.cedar` |
| 2 | No permission without provenance | `provenance.cedar` |
| 3 | No manual grant without an expiry | `grants_expire.cedar` |
| 4 | Nobody escalates themselves | `no_self_escalation.cedar` |
| 5 | Destructive actions require quorum | `quorum.cedar` |
| 6 | Re-evaluation on every request | **Not a policy.** It is where the decision point sits — `commands.dispatch` (`crates/nc-kernel/src/dispatch.rs`) and, from Stage 22, the gateway's middleware, and nowhere else (`§14.2`). A policy cannot make itself be consulted |
| §14.6, 2 | Delegation, never impersonation | `agent_authority.cedar`. An `Agent` principal on a request not marked `via_agent` would be logged as the agent acting alone, and the rule is that the log always shows both. The **intersection** half of the same rule is **not a policy** — see below |
| §14.6, 3 | An actor chain is limited to 2 | `chain_depth.cedar`. Agent calling agent calling agent is how privilege leaks unnoticed; the number in the schema and in every case file's context binds nothing without that policy |
| §14.6, 5 | Budget is a permission | `agent_authority.cedar`. Evaluated by the PDP and named in the journal like any other refusal. Middleware that checked a balance before calling the PDP would be a second authorization path with no entry in the log, which is what the rule forbids |
| 7 | Every policy has a denial test | **Not a policy.** It is a gate: `cargo xtask check` fails when a policy — attributed by its `@id`, wherever the case is written — has no case that both expects `deny` and names it, in `because` for a `forbid` or in `no_permit_matched` for a `permit`. A case that merely expects `deny` is not enough, because Cedar denies by default (`§14.9, invariant 1`) |

**Every policy here is a `forbid`, and `cargo xtask check` holds this sentence.**
Cedar denies by default, so an invariant that *restricts* is a `forbid` and an
invariant that *permits* would be a `permit` — and none of these permits
anything. The step reads the effect of every policy loaded from this directory,
with a floor under it so a prefix that matched nothing could not agree with any
file; a `permit` here is refused by name.

What the step holds is the policies, because nothing holds the prose. Measured:
this sentence changed to say the core is made of `permit`s, one row of the
table above pointed at the wrong file, and the marker count four screens down
made wrong in both its numbers leave the whole gate green, with every number
in the run byte-identical. Two gates read this file and neither
asks whether what it says is true: `counts.rs` judges a numeral only where the
noun is one of the seven it counts, and `references.rs` resolves paths and `§`
citations while its own header says a sentence's *truth* is not a question a
gate can put.

**What is still prose, said plainly rather than left to be discovered.** The
invariant-to-file table above is about meaning — that `provenance.cedar` is what
makes invariant 2 true is a judgement, and no gate makes it. The marker
distribution below is held one gate over, by the per-file hatch counts, which
move when a marker does but say nothing about which policy holds it. A core made of `permit` rules would be a core
that grants, which is the opposite of what a floor is for.

## What the core does not yet reach, and why

Two rules here are narrower than the invariant they carry, and both are narrow
for the same reason: **the decision does not know whether an action is
reversible.**

**One half of `§14.6`'s rule 2 is not a policy, and cannot be.** The
intersection of an agent's rights and its operator's compares what a request
would decide for the agent with what the same request would decide for the
operator, and a Cedar policy set evaluates one principal against one request —
the second decision does not exist to be intersected with. It belongs to the
decision point, which asks twice and takes the narrower answer: that is
`commands.dispatch`, once agents act (`NOCTUA-PLAN.md`, Stage 15). Until then it
decides every dispatch with `via_agent` false and an empty actor chain, so there
is no second decision to take. The other half, *delegation never
impersonation*, is `core.an_agent_never_impersonates` in
`agent_authority.cedar`; rule 5, budget as a permission, is
`core.budget_is_a_permission` beside it.

`reference/identity-and-authorization.md §4` gives the context `now`,
`via_agent`, `confirmed_by_human`, `actor_chain_depth`, `policy_epoch`,
`quorum_met` and `grant` — and `reversibility` is not among them. That file
**outranks `NOCTUA.md`** (`§0.4`), so adding a field to it is not something to do
while transcribing it.

That list carries no count, for the reason `xtask/src/counts.rs`'s header
gives: a claim written with no number cannot rot, and "keys" is not a noun that
gate can answer — every entry of its `COUNTABLE` is answered by a directory and
a glob. A miscount here would under-report the normative document in the
paragraph that justifies why invariants 5 and 5b stop at `Delete`.

| Invariant | What is enforced | What is not |
|---|---|---|
| 5 — destructive actions require quorum | `Delete` | `§14.7`'s status transitions: suspending, freezing, declaring an account deceased are a `Write` on a `User`, and are destructive in the sense that matters |
| 5b — an irreversible action needs a human | `Delete` | by a policy, anything else a command declares `irreversible` — which the decision point holds instead, for every command, below |
| `reference/identity-and-authorization.md §3` — every entity belongs to exactly one realm | no policy here. In the `family` pack, the permits in `realm.cedar`, whose resource is the `Realm` itself, each require `principal in resource`, so a member of another realm reads, prunes and undoes nothing realm-wide. **The decision point enforces it for everything else**, by what it hands Cedar: `nc-authz`'s `Directory::slice` holds only the realm's entities, and `Pdp::decide` denies a principal or a resource the slice does not hold | nothing in the policies alone, which are what the gate decides every case with, over the whole world. `reference/identity-and-authorization.md §3` is unqualified: "Cross-realm access requires an explicit federation tuple; there is no implicit inheritance." Measured with a second realm added: an outsider reads the family's `realm` and `public` content, and a family member reads the other realm's, all by `family.a_member_reads_shared_content` — which constrains the *resource* and never the principal. And `family.a_guardian_administers_the_realm` constrains neither the resource nor membership, so a guardian of one realm is allowed `Grant` and `Revoke` on another `Realm` (`ana Revoke Realm::"office"` answers `Allow` to the gate, and a case in `policy/packs/family/tests/permissions.toml` pins it). `family.a_member_names_files` asks no membership either, so a person of another realm is allowed the `naming` feature, which a case in `policy/packs/family/tests/features.toml` pins |
| `§14.7` — a frozen account is frozen | nothing here. In the `family` pack, `family.a_frozen_account_acts_on_nothing` refuses everything to a `User` whose `status` is not `active`, `dormant` or `presumed_inactive` — `suspended`, `deceased`, and any value it does not know | a pack's floor and not the core's, so a realm whose profile selects another pack holds nothing — `org` and `collective` carry no policy yet. A `Device` or an `Agent` acting for a frozen account is not refused. And the invariant 5 row records the other half — that *making* the transition escapes the quorum rule |

**Realm isolation has no floor, and what stands behind it is the decision
point.** The pack's read permit argues it deliberately — a policy that re-derived
the realm would be a second answer to a question the entity store already
answers — but that is a `permit`-side argument, and this file argues the opposite
for realm isolation: a `forbid` in the core cannot be overridden by a `permit` in
a pack. No core `forbid` holds it, so the gate's cases, decided over the whole
world, still answer `allow` for an outsider.

The decision point is what enforces it, structurally rather than as a rule:
`nc-authz`'s `Directory::slice` hands Cedar only the entities of the realm asked
about, and `Pdp::decide` denies a principal or a resource the slice does not
hold, whatever the policies would answer. Both questions
`policy/packs/family/tests/permissions.toml` pins as `allow` are denied that way,
by `an_entity_of_another_realm_is_denied_even_where_a_permit_reads_nothing_of_it`
in `crates/nc-authz/tests/the_embedded_pack_decides.rs`. Asking the directory
about the kernel's own realm, and no other, is the enforcement point's —
`commands.dispatch`. The two cases stay, because they pin Cedar's answer without
the directory: the day a core `forbid` arrives, they flip and somebody decides
deliberately rather than discovering it.

`Delete` is the shape of an irreversible action this schema can express, and it
is a proxy rather than the rule. Closing the gap means either a `reversibility`
context key — a change to the reference, which is the developer's to make — or a
decision point that refuses to dispatch an irreversible command on its own
account. The second is built: `commands.dispatch` refuses a command the catalog
or its plugin declares irreversible, once Cedar has allowed it, unless the caller presents a
confirmation the kernel issued for that command and those arguments
(`kernel.irreversible_needs_confirmation`, in
`crates/nc-kernel/src/dispatch.rs`). A quorum is still the policies' to ask for.

Recorded here rather than left in a comment, because a core that looks complete
is one nobody checks against `§14.9` again.

## Every condition fails closed, and what that costs

Cedar **skips a policy whose condition errors** and answers from the rest — so a
`forbid` that dereferences `context.policy_epoch` does not refuse a request
without one, it allows it. Measured with such a condition and no schema passed
to the authorizer:

```text
with the schema:    refused: expected the record to have an attribute `policy_epoch`
without the schema: Allow    errors: ["… record does not have the attribute …"]
```

Every condition here is written `!(x has y) || …` or `unless { x has y && … }`,
so a missing attribute forbids. Two consequences worth knowing:

- **The decision point must pass the schema too.** `build_request` always does,
  which is why no case shows the difference, and so does `nc-authz`: its `Pdp::decide`
  builds every request against the schema `nc_policy::compile` hands it beside
  the policies, and denies one the schema refuses before any policy runs. The
  guard is the belt, not the braces.
- **A guard's boundary is a request no case can be**, because a case's context is
  built against the schema. `nc-policy`'s
  `a_request_missing_a_context_attribute_is_refused_with_no_schema_to_catch_it`
  builds it without one, and requires `Deny`. The condition-mutation check
  excludes exactly that shape and says so.

## The operator half of §14.6, 2 is not impersonation

`principal = User::"ana"` with `via_agent = true` reads like impersonation — a
request claiming a chain while naming no agent. It is not: `§14.6, 2` makes the decision *the intersection of the agent's
rights and the operator's*, so the decision point asks twice, and that request is
the **operator half** of an agent-mediated decision. A `forbid` on it breaks
the mechanism the invariant exists to implement — measured against the cases —
so the core carries none, and this paragraph says why before somebody writes
one.

What §14.6, 2 *does* leave to the decision point, and so to the stage in which
agents act: asking twice and taking the narrower answer.
A Cedar policy set evaluates one principal against one request, so the second
decision does not exist to be intersected with — the paragraph at the top of this
file says so, and this is the same boundary from the other end.

## What the narrowing gate does not check

`cargo xtask check` refuses a core policy that turns itself off — by entity type
(`principal is Agent`, `resource == Content::"x"`, or a `has` on an attribute
only one type declares, which is the same test in different words) or by action
(`action == Action::"Read"`). It reads the policy as a syntax tree rather than as
text, and looks *inside* every operand: `(if true then principal else principal)
is Agent` and `principal in [Group::"role.guardian"]` are the same tests wrapped,
and each defeats a check that asks only whether an operand *is* a bare
variable.

**A scope constraint can be excused with `nc:narrows: <reason>`; a condition
cannot.** Five markers here, in three files — one in `no_self_escalation`, two in
`quorum`, two in `agent_authority` — and each says why on the line above. The distinction is not
decoration: a marker that excused the whole policy would let the one written
for `core.destructive_needs_quorum`'s `action == Action::"Delete"` also excuse
a `|| resource is Feature` added to its `unless` — measured — which is one
person removing a platform capability alone. A constraint in the scope is on the policy's first
line, where a reader meets it. A condition is where an exemption hides.

**The reason quotes the constraint it excuses, backticks and all** — `` `action
==` ``, `` `principal is` `` — and not the subject. Naming only the subject is
the same defect one level down: `principal is Agent` is a type test, which is
what `§14.6`'s *delegation, never impersonation* is about, and
`principal == Agent::"ana-assistant"` is the same invariant holding for exactly
one entity in the world. Both are "the principal", and a reason written for the
first would excuse the second with the gate green. The backticks are required because
prose reaches the reader too: *"the action in its scope"* contains `action in`,
and a rule an ordinary sentence satisfies by accident is not a rule.

**And it quotes it on the marker's own line.** The lines under a marker are the
reason a person reads; the line the marker is written on is the reason the gate
reads. They are kept apart because prose quotes a constraint to warn against
it: `no_self_escalation`'s marker explains itself by saying that a blanket
exemption would let `core.provenance` be rescoped to `action == Action::"Read"`,
and read as part of the reason, that sentence excuses exactly that rescoping of
the policy it sits on, with no finding at all. Every marker here quotes on its own line, which is where
an excuse belongs: beside the thing it excuses.

That is also why `agent_authority.cedar` writes `forbid (principal is Agent, …)`
rather than `when { principal is Agent && … }`. The two are the same test and
only one of them is legible.

**It does not judge a context condition, and that is deliberate.** Every
invariant here is written with one — `core.provenance` is
`forbid … unless { context has grant }` — so a check that called a context escape
a narrowing would report `core.provenance` for doing exactly what it exists to
do. Whether `unless { context.something }` is a rule's own definition or a way
around it is a question about meaning, and nothing structural separates the two.
A reviewer reading a new core policy has to answer it; the gate cannot.

**An action constraint is excused per policy, never wholesale.**
`action in [Action::"Grant", Action::"Revoke"]` is how
`core.no_self_escalation` says what it governs — true of that policy, and not a
rule. Measured with action constraints exempt as a kind: `core.provenance`
rescoped to `action == Action::"Read"` gives every `Write`, `Delete`, `Share`,
`Grant` and `Revoke` a pass with no grant at all, and passes the whole gate.
Every provenance case uses `Read`, so case coverage cannot see it either. So the
exemption is written in the file, per policy, rather than assumed for a whole
kind of constraint.

## The tests

Each `<name>.cedar` has a `<name>.toml` in `tests/` beside it, by convention —
but the gate does not depend on the name. Cases are read from every `tests/*.toml`
under `policy/`, and coverage is attributed **by `@id`**, so a policy that moves
to another file keeps its denial test and a file holding two policies is
understood as holding two. What must exist is a denial case per **policy**, not
per file. A case is a principal, an action, a resource, a context and the
decision it must produce — run through the same Cedar the kernel decides with
(`cedar-policy`, one version for both), against `../schema.cedarschema`.

