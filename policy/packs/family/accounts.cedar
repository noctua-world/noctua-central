// The `family` profile's floors about who may act at all.
//
// Both are `forbid`s, so no permit reaches past them, written here or later:
// an account that is frozen, or a device that is not attested, is refused
// whatever it asks for and whatever it would otherwise be allowed.
//
// **Every condition fails closed.** Cedar skips a policy whose condition
// errors and answers from the rest, so a `forbid` that read `principal.status`
// directly would *allow* a request whose principal carried no status, through
// whichever permit came next. Each attribute is guarded with
// `!(principal has x) || …`, so its absence refuses — the shape
// `../../core/README.md` records for every core invariant.

// An account that is frozen acts on nothing.
//
// `NOCTUA.md §14.7`: a deceased person's account "is frozen and never reused";
// suspension is the same stop, applied by a person rather than by succession.
// The roles such an account held migrate later — until they do, a guardian's
// account is still in `Group::"role.guardian"`, which is exactly when this rule
// matters.
//
// **Written as the states that may act, not the ones that may not.** `status`
// is a `String` in the schema and nothing checks it against the five values
// `§14.7` names, so a rule listing `suspended` and `deceased` would let every
// other spelling through — a typo, a different case, a state a later stage adds
// — and the next permit would allow it. `NOCTUA.md §2.4` says that if a check
// cannot be performed the answer is no, so the live states are named instead.
// They are `active`, and `dormant` and `presumed_inactive`, the steps of the
// succession ladder before anybody has decided anything. Every other value is
// frozen, `suspended` and `deceased` among them.
//
// What it deliberately does not reach: a `Device` or an `Agent` acting for a
// frozen account. Neither carries a `status`, and whether a person's devices
// and agents stop with them is the revocation of `NOCTUA.md §14.5` — a
// credential withdrawn where it is served, not re-derived from its owner on
// every request — which this file does not answer.
@id("family.a_frozen_account_acts_on_nothing")
forbid (
  principal is User,
  action,
  resource
)
when {
  !(principal has status) ||
  !(principal.status == "active" ||
    principal.status == "dormant" ||
    principal.status == "presumed_inactive")
};

// A device that is not attested acts on nothing.
//
// A device is a principal because it is an installed local backend holding a
// key (`reference/identity-and-authorization.md §4`); `attested` is what says
// that key was vouched for. An unattested one is a key somebody presented and
// nobody confirmed, so it reads nothing — not even what the household has made
// public.
//
// **Written `when { … || !principal.attested }`, not `unless { … }`.**
// `unless { principal.attested }` fails open for a device with no `attested`;
// `unless { principal has attested && … }` guards it, but a `has` on an
// attribute only `Device` declares is a type test in a condition, which the
// pack-floor rule refuses in a `forbid`. Beside the attribute it guards, in a
// disjunction, the same `has` is read as what it is: the fail-closed guard.
@id("family.an_unattested_device_acts_on_nothing")
forbid (
  principal is Device,
  action,
  resource
)
when { !(principal has attested) || !principal.attested };
