// Invariant 5: **destructive actions require quorum.**
//
// **`Delete`, and only `Delete`.** One person must not be able to delete alone,
// whatever else they hold.
//
// The quorum itself — who counts, how many, and the grace period — is a pack
// property: `family` is 2 of 3, `org` is higher (`§14.4`). What the core fixes
// is that there must be one.
//
// **What this does not yet cover, and why.** `§14.7`'s status transitions —
// suspending, freezing, declaring an account deceased — are destructive in the
// sense that matters, and they are a `Write` on a `User` rather than a `Delete`,
// so `action == Action::"Delete"` misses them.
//
// Widening it to every action against a `User` would demand quorum to *read* a
// profile. Narrowing it correctly needs the decision to know that this
// particular write is destructive — `reversibility`, which
// `reference/identity-and-authorization.md §4`'s context does not carry, and
// that file outranks `NOCTUA.md` (`§0.4`). Adding a field to it is not a change
// to make while transcribing it. The gap is recorded in `README.md` beside the
// two other rules waiting on the same thing.
// **Every policy carries an `@id`.** `NOCTUA.md §14.9` requires each journal
// entry to record **which policy decided**, and without an annotation Cedar
// names them `policy0`, `policy1` — a number that changes the moment a file is
// added, so an entry written today would name a different rule tomorrow. The id
// is the policy's stable identity, and it is what reaches the journal.
// nc:narrows: `action ==`, and the action is the rule. `Delete` is the shape of a
// destructive action this schema can
// express, and the README's own table records that as a *proxy* for the rule
// rather than the rule. Widening it needs either a `reversibility` context key —
// a change to `reference/identity-and-authorization.md`, which outranks
// `NOCTUA.md` — or the decision point refusing on its own account. That is
// `commands.dispatch`, which does so for an irreversible command's confirmation
// and asks a quorum of nothing this rule does not.
@id("core.destructive_needs_quorum")
forbid (
  principal,
  action == Action::"Delete",
  resource
)
unless { context has quorum_met && context.quorum_met };

// And the one §14.6 adds on top: an irreversible action taken through an agent
// needs a human confirmation the **kernel** issued from real UI interaction —
// never an argument the agent could fill in.
// nc:narrows: `action ==`, the same proxy, for the same reason and with the same limit
// recorded in `policy/core/README.md`: `Delete` is what this schema can say, and
// anything else a command declares `irreversible` in `catalog/commands.toml` is
// beyond it until the reference gains the key.
@id("core.irreversible_needs_a_human")
forbid (
  principal,
  action == Action::"Delete",
  resource
)
when { !(context has via_agent) || context.via_agent }
unless { context has confirmed_by_human && context.confirmed_by_human };
