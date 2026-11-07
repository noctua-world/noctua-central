// Invariant 3: **no manual grant without an expiry.**
//
// `valid_until: None` is permitted only for tuples with `source = "policy"` —
// those are derived from the pack and disappear when the pack changes. A
// perpetual *manual* grant is the permission nobody remembers giving and nobody
// thinks to remove, and it is refused at write time as well as here.
// **Every policy carries an `@id`.** `NOCTUA.md §14.9` requires each journal
// entry to record **which policy decided**, and without an annotation Cedar
// names them `policy0`, `policy1` — a number that changes the moment a file is
// added, so an entry written today would name a different rule tomorrow. The id
// is the policy's stable identity, and it is what reaches the journal.
@id("core.grants_expire")
forbid (
  principal,
  action,
  resource
)
when { context has grant && (!(context.grant has source) || context.grant.source != "policy") }
unless { context.grant has valid_until };

// And a window that has not opened is not a window either.
//
// **`valid_from` is in the schema, and this is what reads it.** The reference
// gives every tuple provenance — `granted_by`, `reason`, `valid_from`,
// `valid_until`, `source` — and without this policy the far end of the window
// is enforced and the near end is not. Measured without it, with `now =
// 2026-08-26`: a grant written for a visit in June 2027 authorizes reads,
// writes and administration *that day*, while the identical grant that ran out
// the January before is refused.
//
// Fail-closed in the same shape as its neighbour: a grant with no `valid_from`,
// or a request with no `now`, is refused rather than waved through. There is no
// grants table yet, so nothing behind this enforces it at write time either.
@id("core.grant_has_begun")
forbid (
  principal,
  action,
  resource
)
when {
  context has grant &&
  (!(context.grant has valid_from) ||
   !(context has now) ||
   context.now < context.grant.valid_from)
};

// And an expiry that has passed is not an expiry.
//
// **This policy's own `!(context has now)` guard cannot be the sole reason for
// any request, and that is written down rather than left to be rediscovered.**
// `core.grant_has_begun` above fires whenever `now` is absent — its second
// disjunct is the same test — so by the time this one's guard would decide,
// that one has already denied. Measured: deleting `!(context has now) ||` from
// this policy leaves the whole `nc-policy` suite green, and deleting it from
// *both* makes `guard-fails-open` fire for both. Deleting it from the policy
// above is caught, by a case built on a grant this one stands aside for.
//
// It stays because a policy has to fail closed on its own terms: the sibling
// that currently covers it is one edit away from not covering it, and this
// clause is what makes that edit survivable.
@id("core.grant_not_expired")
forbid (
  principal,
  action,
  resource
)
when {
  context has grant &&
  context.grant has valid_until &&
  (!(context has now) || context.grant.valid_until <= context.now)
};
