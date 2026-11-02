// Invariant 4: **nobody escalates themselves.**
//
// `forbid` on `Grant` and `Revoke` when the resource is the principal. Without
// it, every other control is one request away from being edited by the person it
// constrains — and the request would be perfectly well formed, perfectly
// authenticated, and perfectly logged.
//
// It is a `forbid` rather than an omitted `permit` on purpose: a `forbid` in the
// core cannot be overridden by a `permit` in a pack, and Cedar's precedence is
// what makes "the packs may add and may never contradict" true rather than
// merely intended.
// **Every policy carries an `@id`.** `NOCTUA.md §14.9` requires each journal
// entry to record **which policy decided**, and without an annotation Cedar
// names them `policy0`, `policy1` — a number that changes the moment a file is
// added, so an entry written today would name a different rule tomorrow. The id
// is the policy's stable identity, and it is what reaches the journal.
// nc:narrows: `action in` — the action *is* the rule. "Nobody escalates themselves" is about
// granting and revoking, and a `forbid` covering every action would refuse a
// person reading her own record. The constraint is the invariant's subject
// matter rather than an exemption from it — and it is written here because a
// blanket exemption for action constraints let `core.provenance` be rescoped to
// `action == Action::"Read"`, which switched invariant 2 off for every write.
@id("core.no_self_escalation")
forbid (
  principal,
  action in [Action::"Grant", Action::"Revoke"],
  resource
)
when { principal == resource };
