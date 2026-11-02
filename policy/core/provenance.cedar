// Invariant 2: **no permission without provenance.**
//
// Every relation tuple is a `Grant` carrying `granted_by`, `reason`,
// `valid_from` and `source` (`reference/identity-and-authorization.md §4`). A
// permission is never a boolean column, because a boolean cannot answer "who
// decided this, and why" — and that question is the whole of an audit.
//
// A decision that rests on no grant at all is a decision nobody can account for.
// **Every policy carries an `@id`.** `NOCTUA.md §14.9` requires each journal
// entry to record **which policy decided**, and without an annotation Cedar
// names them `policy0`, `policy1` — a number that changes the moment a file is
// added, so an entry written today would name a different rule tomorrow. The id
// is the policy's stable identity, and it is what reaches the journal.
@id("core.provenance")
forbid (
  principal,
  action,
  resource
)
unless { context has grant };
