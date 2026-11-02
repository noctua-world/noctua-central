// The `family` profile's permits over the realm itself, and over an account.
//
// The decision point at `commands.dispatch` names a `Realm` as the resource of
// a command that acts on the whole realm rather than on one thing in it: the
// journal verified (`Read`), the tables pruned (`Delete`), a person's own step
// undone or redone (`Write`). `permissions.cedar` beside this file reaches a
// `Realm` only through `family.a_guardian_administers_the_realm`, for `Grant`
// and `Revoke` and with no membership test — so until these existed a `Read`,
// `Write` or `Delete` on a realm was refused by Cedar's default.
//
// **Membership is the rule here, not a second answer to which realm.** The
// read permit in `permissions.cedar` leaves realm scoping to the decision
// point, because a `Content` belongs to a realm and re-deriving which one is a
// question already answered. When the resource *is* the realm, "is this person
// in it" is what the permit is about: each one below that reaches a realm says
// `principal in resource`, and `Realm::"office"` in `../../world.json` — a
// realm no member of this household belongs to — is what the cases deny them.
// The guardian permit in `permissions.cedar` asks no such thing, so a guardian
// of this household is allowed `Grant` and `Revoke` on that realm; a case in
// `tests/permissions.toml` pins the answer as it stands.
//
// **A constraint no case can pin is not written.** The case runner removes each
// scope constraint of a permit in turn and requires some case to change its
// answer (`nc_policy::run`). Where the rest of the permit already decides the
// same thing for every request the schema can build, that removal changes
// nothing, and the constraint is one nothing could ever show working — so it is
// left out and the reason is written beside the permit instead.
//
// What these deliberately do not do: they widen nothing the core refuses.
// Cedar's `forbid` beats a `permit`, so a guardian's prune still needs the
// quorum `core.destructive_needs_quorum` asks for, and a step taken on an
// expired grant is still refused by `core.grant_not_expired`.

// A guardian reads the realm she keeps: the journal verified is the first
// command that reads it.
//
// **No `resource is Realm`.** `principal in resource` already holds it: a
// principal can be in a `Group` or a `Realm` and nothing else the schema
// declares, and `Group` is not a resource any action applies to — so the only
// other resource this reaches is the guardian herself, by `in`'s reflexivity,
// and reading her own account is what the last permit in this file allows
// anyway. The type test would be a constraint no request can show working.
@id("family.a_guardian_reads_the_realm")
permit (
  principal in Group::"role.guardian",
  action == Action::"Read",
  resource
)
when { principal in resource };

// A guardian prunes the realm she keeps, and only the realm.
//
// The quorum is the core's (`core.destructive_needs_quorum`) and this permit
// does not repeat it: a pack that restated a core rule would be the second copy
// of it, and the narrower one on the day the two disagree.
//
// **`resource is Realm` stays, and it is not the redundancy above.** `Delete` on
// the guardian's own `User` is reachable by `in`'s reflexivity and nothing else
// permits it, so without the type test a guardian could delete her own account
// under this rule. The case that pins it is in `tests/realm.toml`.
@id("family.a_guardian_prunes_the_realm")
permit (
  principal in Group::"role.guardian",
  action == Action::"Delete",
  resource is Realm
)
when { principal in resource };

// A member steps back and forward through their own history: undo and redo,
// which reverse only the steps of the principal who asks (`NOCTUA.md §11.5`).
//
// **A member is a person: `principal is User` is this permit's own
// constraint, and it is not the redundancy above.** A device and an agent are
// in the realm as much as a person is, so `principal in resource` lets both
// through; the type test is what refuses them. It shipped deleted once, with a
// note saying no request could show it working — and an attested device and an
// agent acting openly as one were both allowed to write the realm. The cases
// that pin it are in `tests/realm.toml`, and they say
// `the_principal_is_the_boundary`, because a case refused for its kind of
// principal can only be paired with an `allow` for another kind.
//
// It is also what keeps every device off the realm. The unattested-device
// floor in `accounts.cedar` refuses an unattested one whatever it asks, and
// its own case decides that on `Content`: with this type test in place, a
// realm is not a resource any permit lets a device reach.
@id("family.a_member_steps_through_their_own_history")
permit (
  principal is User,
  action == Action::"Write",
  resource is Realm
)
when { principal in resource };

// A member reads their own account.
//
// **Neither `principal is User` nor `resource is User`.** `principal == resource`
// holds both: `User` is the one type the schema declares as a principal and as
// a resource, so the equality is a `User` reading itself and nothing else.
// Either type test beside it would be a constraint no request can show working.
@id("family.a_member_reads_their_own_account")
permit (
  principal,
  action == Action::"Read",
  resource
)
when { principal == resource };
