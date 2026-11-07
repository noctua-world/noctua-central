// The `family` profile's defaults over content and over who may grant.
//
// `reference/identity-and-authorization.md §3`: the profile selects a pack, and
// the packs "share an invariant core (§9) and differ in their defaults". This is
// the only pack implemented; `../org/` and `../collective/` exist beside it
// because `NOCTUA.md §14.4` requires the indirection from the first commit —
// "or the other two become a rewrite". `realm.cedar` beside this file holds the
// pack's permits to read, prune and step through the realm itself, and
// `accounts.cedar` its floors about who may act at all. The one permit here
// that reaches a `Realm` is the guardian's, below, for `Grant` and `Revoke`.
//
// **These are the permits, and the core is what constrains them.** Cedar denies
// by default and a `forbid` beats a `permit`, so a pack can widen what is
// allowed and can never contradict the floor in `../../core/`. That precedence
// is the mechanism, not the convention.
//
// **Realm scoping is not checked here, deliberately.** The reference is explicit
// that "every entity belongs to exactly one realm. Cross-realm access requires
// an explicit federation tuple; there is no implicit inheritance" — so the
// request reaching Cedar is already scoped to one realm by the decision point at
// `commands.dispatch`, whose directory hands Cedar that realm's entities and no
// other's (`NOCTUA.md §14.3`). A policy that re-derived the realm
// would be a second answer to a question already answered, and the two would
// eventually disagree.

// A guardian administers the realm's permissions.
//
// Roles are groups (`Group::"role.billing"` in §4), which is why this is a
// membership test and not an attribute. Note what it does *not* carve out:
// granting to oneself. That exclusion lives in the core as
// `core.no_self_escalation`, where no pack can drop it.
//
// Nor does it name a resource type or ask membership, so a guardian of this
// household is allowed `Grant` and `Revoke` on any `Realm`, one she is not in
// included. That is the realm scoping above, left to the decision point; the
// permits in `realm.cedar` are the ones that ask `principal in resource`, and
// `tests/permissions.toml` pins this answer as it stands.
@id("family.a_guardian_administers_the_realm")
permit (
  principal in Group::"role.guardian",
  action in [Action::"Grant", Action::"Revoke"],
  resource
);

// Anything shared with the household is readable by the household.
//
// `sensitivity` is not a label: `NOCTUA.md §13.3` keys encryption off it, so
// `private` and `restricted` are encrypted before they are stored. This permit
// stops at `realm` for that reason — the two it omits are the two the platform
// cannot read either.
@id("family.a_member_reads_shared_content")
permit (
  principal,
  action == Action::"Read",
  resource
)
when {
  resource is Content &&
  (resource.sensitivity == "public" || resource.sensitivity == "realm")
};

// An owner governs what is theirs.
@id("family.an_owner_governs_their_own_content")
permit (
  principal,
  action in [Action::"Read", Action::"Write", Action::"Delete", Action::"Share"],
  resource
)
when { resource is Content && resource.owner == principal };

// **Handing out encrypted content needs the same quorum a deletion needs.**
//
// `reference/identity-and-authorization.md §6`: revoking access to content
// already delivered is impossible. Handing out private or restricted content is
// key distribution, and it is the most irreversible thing this schema can
// express — `Content` carries `sensitivity` in `§4`'s own schema, so this needs
// nothing invented.
//
// Measured without it: `User::"ana"` sharing `Content::"vault"`
// (`sensitivity: "restricted"`) with `quorum_met = false` is permitted, and so
// is granting over the same resource. Invariant 5's core rule is
// `action == Action::"Delete"`, and `../../core/README.md` records why widening
// *that* needs a `reversibility` context key the reference does not carry.
//
// **Both verbs, and both labels.** `Share` alone leaves `Grant` open, and
// `restricted` alone leaves `private`, while §6 names the pair in one breath
// ("private or restricted content: encrypted with a random per-content key")
// and the read permit twenty lines up stops at `realm` for exactly that reason.
// Measured with a rule covering `Share` and `restricted` alone, and
// `quorum_met = false` throughout:
//
//   ana Share Content::"vault"   -> Deny  by family.sharing_restricted_…
//   ana Grant Content::"vault"   -> Allow by family.a_guardian_administers_the_realm
//   ana Share Content::"diary"   -> Allow by family.an_owner_governs_their_own_content
//   ana Grant Content::"recipe"  -> Allow  — carla's private content, which ana
//                                   may not read: `ana Read Content::"recipe"`
//                                   is a Deny two lines above it
//
// The last line is the one that matters: a guardian could hand a third party a
// key to content the guardian herself is refused, with no quorum and no rule
// naming it. `Grant` is the verb that distributes the key.
//
// **Here rather than in the core, and that is a rule.** A core
// invariant holds for every principal, every action and every resource the
// schema declares; this one is about `Content` and says so, which the narrowing
// gate correctly refuses in a core file. A pack is where a rule about one kind
// of resource belongs, and a `forbid` here still beats every `permit` above it.
//
// **And it says `resource is Content` rather than leaning on `has sensitivity`.**
// Fail-closed still: a `Content` whose label this cannot read needs the quorum.
// What it must not do is fire on a `User` or a `Feature` — a guardian granting a
// member access to a feature is ordinary administration, and §6's irrevocability
// argument is about content keys, not about permission tuples.
//
// nc:narrows: `action in` — the two verbs *are* the rule. This floor is about
// handing somebody else the key to encrypted content, and `Share` and `Grant`
// are the only actions in the schema that do it: `Read` does not distribute a
// key, `Write` and `Delete` act on content already held, and `Revoke` takes
// access away. Naming them is this rule saying what it governs, not taking an
// action out from under it. A verb that distributes a key added to §4 later
// belongs in this list, and that is the edit this marker exists to be read at.
@id("family.giving_out_encrypted_content_needs_quorum")
forbid (
  principal,
  action in [Action::"Share", Action::"Grant"],
  resource
)
when {
  resource is Content &&
  (!(resource has sensitivity) ||
   resource.sensitivity == "restricted" ||
   resource.sensitivity == "private")
}
unless { context has quorum_met && context.quorum_met };
