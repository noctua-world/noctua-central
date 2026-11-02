// `NOCTUA.md §14.6`, rules 2 and 5 — the two that had no policy at all.
//
// `Agent.operator` and `Agent.budget_remaining` were in the schema and read by
// nothing, which `policy/core/README.md` recorded as a gap of the same shape as
// the chain-depth one: **a rule with no policy has no policy to lack a test**,
// so `§14.9`'s seventh invariant cannot report it. Criterion 11 is structurally
// blind to a missing rule; only a person reading `§14.6` beside `ls
// policy/core/` finds one.
//
// # What each of these is, and what it is not
//
// **Rule 5 — "budget is a permission, evaluated by the PDP. Not separate
// middleware."** That is the whole of it, and it is one policy: an agent with
// nothing left to spend is refused *here*, by the same decision that refuses
// everything else, and appears in the journal naming this rule. Middleware that
// checked a balance before calling the PDP would be a second authorization path
// with no entry in the log, which is what the sentence forbids.
//
// **Rule 2 — "a decision is the intersection of the agent's rights and the
// operator's rights at the moment of decision. Delegation, never impersonation —
// the log always shows both."** Two halves, and only one of them is a policy.
//
// *Never impersonation* is: an `Agent` principal whose request is not marked
// `via_agent` is a request that would be logged as the agent acting alone, and
// the sentence says the log always shows both. That is `core.an_agent_never_
// impersonates` below.
//
// *The intersection* is **not** a policy, and cannot be one. It compares what
// this request would decide for the agent with what the same request would
// decide for the operator, and a Cedar policy set evaluates one principal
// against one request — the second decision does not exist to be intersected
// with. It belongs to the decision point, which asks twice and takes the
// narrower answer: `commands.dispatch`, once agents act (Stage 15). It is the same
// reason `§14.9`'s sixth invariant is not a policy: a policy cannot make itself
// be consulted, and it cannot consult itself about somebody else either.
//
// Recording that here rather than writing a policy that looks like the rule and
// enforces half of it, because a core that looks complete is one nobody checks
// against `§14.6` again.

// nc:narrows: `principal is` — a type test and not a named entity. Budget is an
// `Agent` attribute and no other type has one, so this
// rule is about agents by construction rather than by exemption. Writing it for
// every principal would mean giving a `User` a budget, which is a product
// decision nobody has made.
//
// **In the scope, not in a condition**, and the difference is the whole of what
// the marker may excuse: a constraint on the first line is what somebody reads
// before anything else, and a `principal is Agent` inside a `when` is where an
// exemption hides. The two are the same test and only one of them is legible.
@id("core.budget_is_a_permission")
forbid (
  principal is Agent,
  action,
  resource
)
when { !(principal has budget_remaining) || principal.budget_remaining <= 0 };

// nc:narrows: `principal is` — a type test and not a named entity. Only an `Agent`
// can be delegated to, so only an `Agent` can
// impersonate. The rule has nothing to say about a `User` or a `Device` acting
// as itself, which is every request they make. In the scope, for the reason
// above.
@id("core.an_agent_never_impersonates")
forbid (
  principal is Agent,
  action,
  resource
)
when { !(context has via_agent) || !context.via_agent };

