// Invariant 1: deny by default, and **fail closed**.
//
// Cedar denies anything no `permit` reaches, so the first half needs no policy.
// This is the second half: a request whose authorization state cannot be trusted
// is denied rather than allowed through.
//
// `policy_epoch` is what makes that checkable (`NOCTUA.md §14.8`). A local
// backend caches decisions, so it can authorize from stale state — somebody
// removed from a group keeps reading. Every decision carries the epoch it was
// made at, and an epoch of zero means the caller could not establish one at all.
// **That is the unavailable-decision-point case, and it denies.**
// **Every policy carries an `@id`.** `NOCTUA.md §14.9` requires each journal
// entry to record **which policy decided**, and without an annotation Cedar
// names them `policy0`, `policy1` — a number that changes the moment a file is
// added, so an entry written today would name a different rule tomorrow. The id
// is the policy's stable identity, and it is what reaches the journal.
@id("core.fail_closed")
forbid (
  principal,
  action,
  resource
)
when { !(context has policy_epoch) || context.policy_epoch <= 0 };
